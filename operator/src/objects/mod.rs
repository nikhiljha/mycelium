use std::{
    array::IntoIter,
    collections::{BTreeMap, HashMap},
    fmt::Debug,
    iter::Map,
    ops::Range,
    path::Path,
    sync::{Arc, RwLock},
    time::Duration,
};

use chrono::{DateTime, Utc};
use futures::{future::BoxFuture, FutureExt, StreamExt};
use k8s_openapi::{
    api::{
        apps::v1::{StatefulSet, StatefulSetSpec},
        core::v1::{
            ConfigMapVolumeSource, Container, EnvVar, PersistentVolumeClaim, PodSecurityContext,
            PodSpec, PodTemplateSpec, ResourceRequirements, SecurityContext, Service, ServicePort,
            ServiceSpec, Volume, VolumeMount,
        },
    },
    apimachinery::pkg::{
        apis::meta::v1::{LabelSelector, ObjectMeta, OwnerReference},
        util::intstr::IntOrString,
    },
};
use kube::{
    api::{ListParams, Patch, PatchParams},
    Api, Client, Resource, ResourceExt,
};
use kube_runtime::{
    controller::{Context, ReconcilerAction},
    Controller,
};
use prometheus::{
    default_registry, proto::MetricFamily, register_histogram_vec, register_int_counter,
    HistogramOpts, HistogramVec, IntCounter,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, event, field, info, instrument, trace, warn, Level, Span};

use crate::{
    helpers::{manager::Data, metrics::Metrics, state::State},
    objects::minecraft_set::MinecraftSetSpec,
    Error, MinecraftProxy, MinecraftSet,
};
use crate::Error::MyceliumError;
use crate::helpers::jarapi::get_download_url;

pub mod minecraft_proxy;
pub mod minecraft_set;

#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Clone, JsonSchema)]
pub struct ConfigOptions {
    /// name of configmap to mount
    pub name: String,

    /// location relative to the Minecraft root to mount the configmap
    pub path: String,
}

#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContainerOptions {
    /// resource requirements for the java pod
    pub resources: Option<ResourceRequirements>,

    /// volume to mount to the minecraft root (only useful for replicas = 1)
    pub volume: Option<Volume>,

    /// volume claim template to use for the minecraft root (overrides the volume field if set)
    pub volume_claim_template: Option<PersistentVolumeClaim>,

    /// nodes that the java pod can be scheduled on
    pub node_selector: Option<BTreeMap<String, String>>,

    /// pod security context for the minecraft server (should be restrictive)
    pub security_context: Option<PodSecurityContext>,
}

#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Clone, JsonSchema)]
pub struct RunnerOptions {
    /// server jar to download and run
    pub jar: VersionTriple,

    /// space separated options to pass to the JVM (i.e. -Dsomething=something -Dother=other)
    pub jvm: Option<String>,

    /// configmaps to mount inside the minecraft root
    pub config: Option<Vec<ConfigOptions>>,

    /// list of plugin URLs to download on server start
    pub plugins: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Clone, JsonSchema)]
pub struct VersionTriple {
    /// type of jar (currently only `paper` or `velocity` is supported)
    pub r#type: String,

    /// version according to the PaperMC API
    pub version: String,

    /// build according to the PaperMC API
    pub build: String,
}

pub fn make_volume_mount(co: &ConfigOptions) -> VolumeMount {
    return VolumeMount {
        name: co.name.clone(),
        mount_path: String::from(
            Path::new("/config/")
                .join(&co.path)
                .to_str()
                .expect("mount path"),
        ),
        ..VolumeMount::default()
    };
}

pub fn make_volume(co: &ConfigOptions) -> Volume {
    return Volume {
        name: co.name.clone(),
        config_map: Some(ConfigMapVolumeSource {
            name: Some(co.name.clone()),
            ..ConfigMapVolumeSource::default()
        }),
        ..Volume::default()
    };
}

pub fn object_to_owner_reference<K: Resource<DynamicType = ()>>(
    meta: ObjectMeta,
) -> Result<OwnerReference, Error> {
    Ok(OwnerReference {
        api_version: K::api_version(&()).to_string(),
        kind: K::kind(&()).to_string(),
        name: meta.name.ok_or(MyceliumError("failed to get name".into()))?,
        uid: meta.uid.ok_or(MyceliumError("failed to get uid".into()))?,
        ..OwnerReference::default()
    })
}

pub async fn generic_reconcile(
    env: Vec<EnvVar>,
    port: IntOrString,
    name: String,
    ns: String,
    ctx: Context<Data>,
    owner_reference: OwnerReference,
    shortname: String,
    replicas: i32,
    container: ContainerOptions,
    runner: RunnerOptions,
) -> Result<(), Error> {
    let client = ctx.get_ref().client.clone();
    // Note: This will only error with PoisonError, which is unrecoverable and so we
    // should panic.
    ctx.get_ref().state.write().expect("last_event").last_event = Utc::now();

    let labels = BTreeMap::from_iter(IntoIter::new([(
        format!("mycelium.njha.dev/{}", shortname),
        name.clone(),
    )]));
    let configs = runner.config.unwrap_or(vec![]);
    let mut volume_mounts: Vec<VolumeMount> = configs.iter().map(make_volume_mount).collect();
    let mut volumes: Vec<Volume> = configs.iter().map(make_volume).collect();
    let mut tpl_volume: Vec<PersistentVolumeClaim> = vec![];

    if let Some(volume_tpl) = container.volume_claim_template {
        volume_mounts.push(VolumeMount {
            mount_path: "/data".to_string(),
            name: volume_tpl.metadata.clone().name
                .ok_or(MyceliumError("volumeClaimTemplate name".into()))?,
            ..VolumeMount::default()
        });
        tpl_volume.push(volume_tpl);
    } else if let Some(volume) = container.volume {
        let name = volume.name.clone();
        volumes.push(volume);
        volume_mounts.push(VolumeMount {
            mount_path: "/data".to_string(),
            name,
            ..VolumeMount::default()
        });
    }

    let env: Vec<EnvVar> = vec![
        EnvVar {
            name: String::from("MYCELIUM_JVM_OPTS"),
            value: runner.jvm,
            value_from: None,
        },
        EnvVar {
            name: String::from("MYCELIUM_FW_TOKEN"),
            value: Some(String::from(&ctx.get_ref().config.forwarding_secret)),
            value_from: None,
        },
        EnvVar {
            name: String::from("MYCELIUM_RUNNER_JAR_URL"),
            value: Some(get_download_url(
                &runner.jar.r#type,
                &runner.jar.version,
                &runner.jar.build,
            )),
            value_from: None,
        },
    ].into_iter().chain(env).collect();
    let statefulset = StatefulSet {
        metadata: ObjectMeta {
            name: Some(name.clone()),
            owner_references: Some(vec![owner_reference.clone()]),
            ..ObjectMeta::default()
        },
        spec: Some(StatefulSetSpec {
            selector: LabelSelector {
                match_labels: Some(labels.clone()),
                ..LabelSelector::default()
            },
            service_name: name.clone(),
            replicas: Some(replicas),
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(labels.clone()),
                    annotations: Some(vec![("prometheus.io/port".into(), "9970".into()),
                                           ("prometheus.io/scrape".into(), "true".into())]
                        .into_iter().collect()),
                    ..ObjectMeta::default()
                }),
                spec: Some(PodSpec {
                    security_context: container.security_context,
                    containers: vec![Container {
                        name: name.clone(),
                        tty: Some(true),
                        stdin: Some(true),
                        image: Some(String::from(&ctx.get_ref().config.runner_image)),
                        image_pull_policy: Some(String::from("IfNotPresent")),
                        resources: container.resources,
                        env: Some(env),
                        volume_mounts: Some(volume_mounts),
                        ..Container::default()
                    }],
                    volumes: Some(volumes),
                    ..PodSpec::default()
                }),
                ..PodTemplateSpec::default()
            },
            volume_claim_templates: Some(tpl_volume),
            ..StatefulSetSpec::default()
        }),
        status: None,
    };

    let service = Service {
        metadata: ObjectMeta {
            name: Some(name.clone()),
            owner_references: Some(vec![owner_reference]),
            ..ObjectMeta::default()
        },
        spec: Some(ServiceSpec {
            // https://kubernetes.io/docs/concepts/services-networking/service/#headless-services
            cluster_ip: Some(String::from("None")),
            selector: Some(labels),
            ports: Some(vec![ServicePort {
                protocol: Some(String::from("TCP")),
                port: 25565,
                target_port: Some(port),
                ..ServicePort::default()
            }]),
            ..ServiceSpec::default()
        }),
        status: None,
    };

    kube::Api::<StatefulSet>::namespaced(client.clone(), &ns)
        .patch(
            &name,
            &PatchParams::apply("mycelium.njha.dev"),
            &Patch::Apply(&statefulset),
        )
        .await?;

    kube::Api::<Service>::namespaced(client.clone(), &ns)
        .patch(
            &name,
            &PatchParams::apply("mycelium.njha.dev"),
            &Patch::Apply(&service),
        )
        .await?;

    Ok(())
}
