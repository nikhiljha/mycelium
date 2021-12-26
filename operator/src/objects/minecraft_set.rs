use std::{
    array::IntoIter,
    collections::{BTreeMap, HashMap},
    iter::FromIterator,
    sync::Arc,
};

use chrono::prelude::*;
use futures::{future::BoxFuture, FutureExt, StreamExt};
use k8s_openapi::{
    api::{
        apps::v1::{StatefulSet, StatefulSetSpec},
        core::v1::{
            Container, EnvVar, PodSpec, PodTemplateSpec, ResourceRequirements, Service,
            ServicePort, ServiceSpec, Volume, VolumeMount,
        },
    },
    apimachinery::pkg::{
        apis::meta::v1::{LabelSelector, ObjectMeta, OwnerReference},
        util::intstr::IntOrString,
    },
};
use kube::{
    api::{Api, ListParams, Patch, PatchParams, ResourceExt},
    client::Client,
    CustomResource, Resource,
};
use kube_runtime::controller::{Context, Controller, ReconcilerAction};
use maplit::hashmap;
use prometheus::{
    default_registry, proto::MetricFamily, register_histogram_vec, register_int_counter,
    HistogramOpts, HistogramVec, IntCounter,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::{
    sync::RwLock,
    time::{Duration, Instant},
};
use tracing::{debug, error, event, field, info, instrument, trace, warn, Level, Span};

use crate::{
    helpers::{manager::Data, telemetry},
    objects::{make_volume, make_volume_mount, ConfigOptions, ContainerOptions, RunnerOptions},
    Error, Result,
};
use crate::helpers::jarapi::get_download_url;

#[derive(CustomResource, Serialize, Deserialize, Default, Debug, PartialEq, Clone, JsonSchema)]
#[kube(
    group = "mycelium.njha.dev",
    version = "v1alpha1",
    kind = "MinecraftSet"
)]
#[kube(shortname = "mcset", namespaced)]
pub struct MinecraftSetSpec {
    pub replicas: i32,
    pub r#type: String,
    pub game: RunnerOptions,
    pub container: ContainerOptions,
    pub proxy: ProxyOptions,
}

#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Clone, JsonSchema)]
pub struct ProxyOptions {
    /// configures the proxy to create a forced host for the MinecraftSet
    pub hostname: Option<String>,
}

#[instrument(skip(ctx), fields(trace_id))]
pub async fn reconcile(mcset: MinecraftSet, ctx: Context<Data>) -> Result<ReconcilerAction, Error> {
    let trace_id = telemetry::get_trace_id();
    Span::current().record("trace_id", &field::display(&trace_id));
    let start = Instant::now();

    let client = ctx.get_ref().client.clone();
    // Note: This will only error with PoisonError, which is unrecoverable and so we
    // should panic.
    ctx.get_ref().state.write().expect("last_event").last_event = Utc::now();
    let name = ResourceExt::name(&mcset);
    let ns = ResourceExt::namespace(&mcset).expect("failed to get mcset namespace");
    let configs: Vec<ConfigOptions> = mcset.spec.game.config.unwrap_or(vec![]);

    let owner_reference = OwnerReference {
        controller: Some(true),
        ..crate::objects::object_to_owner_reference::<MinecraftSet>(mcset.metadata.clone())?
    };
    let labels = BTreeMap::from_iter(IntoIter::new([(
        String::from("mycelium.njha.dev/mcset"),
        name.clone(),
    )]));
    let mut volume_mounts: Vec<VolumeMount> = configs.iter().map(make_volume_mount).collect();
    let mut volumes: Vec<Volume> = configs.iter().map(make_volume).collect();
    if let Some(volume) = mcset.spec.container.volume {
        let name = volume.name.clone();
        volumes.push(volume);
        volume_mounts.push(VolumeMount {
            mount_path: "/data".to_string(),
            name,
            ..VolumeMount::default()
        });
    }
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
            replicas: Some(mcset.spec.replicas.clone()),
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(labels.clone()),
                    ..ObjectMeta::default()
                }),
                spec: Some(PodSpec {
                    security_context: mcset.spec.container.security_context,
                    containers: vec![Container {
                        name: name.clone(),
                        image: Some(format!("harbor.ocf.berkeley.edu/mycelium/runner:{}", env!("CARGO_PKG_VERSION"))),
                        image_pull_policy: Some(String::from("IfNotPresent")),
                        resources: mcset.spec.container.resources,
                        env: Some(vec![EnvVar {
                            name: String::from("MYCELIUM_RUNNER_KIND"),
                            value: Some(String::from("game")),
                            value_from: None,
                        }, EnvVar {
                            name: String::from("MYCELIUM_FW_TOKEN"),
                            value: Some(String::from(&ctx.get_ref().config.forwarding_secret)),
                            value_from: None,
                        }, EnvVar {
                            name: String::from("MYCELIUM_PLUGINS"),
                            value: Some(mcset.spec.game.plugins.unwrap_or(vec![]).join(",")),
                            value_from: None,
                        }, EnvVar {
                            name: String::from("MYCELIUM_RUNNER_JAR_URL"),
                            value: Some(get_download_url(&mcset.spec.r#type, &mcset.spec.game.jar.version, &mcset.spec.game.jar.build)),
                            value_from: None,
                        }, EnvVar {
                            name: String::from("MYCELIUM_JVM_OPTS"),
                            value: mcset.spec.game.jvm,
                            value_from: None,
                        }]),
                        volume_mounts: Some(volume_mounts),
                        ..Container::default()
                    }],
                    volumes: Some(volumes),
                    ..PodSpec::default()
                }),
                ..PodTemplateSpec::default()
            },
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
                target_port: Some(IntOrString::Int(25565)),
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
            &kube::api::PatchParams::apply("mycelium.njha.dev"),
            &kube::api::Patch::Apply(&service),
        )
        .await?;

    let duration = start.elapsed().as_millis() as f64 / 1000.0;
    ctx.get_ref()
        .metrics
        .set_reconcile_duration
        .with_label_values(&[])
        .observe(duration);
    ctx.get_ref().metrics.set_handled_events.inc();
    info!("Reconciled MinecraftSet \"{}\" in {}", name, ns);

    // TODO: Do we need to check back if this succeeded & no changes were made?
    // i.e. Do we want to revert manual edits to StatefulSets or Services on a
    // timer?
    Ok(ReconcilerAction {
        requeue_after: None,
    })
}
