use std::{
    array::IntoIter,
    collections::{BTreeMap, HashMap},
    env,
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
    kind = "MinecraftProxy",
    plural = "minecraftproxies"
)]
#[kube(shortname = "mcproxy", namespaced)]
pub struct MinecraftProxySpec {
    pub replicas: i32,
    pub r#type: String,
    pub proxy: RunnerOptions,
    pub container: ContainerOptions,
}

#[instrument(skip(ctx), fields(trace_id))]
pub async fn reconcile(
    mcproxy: MinecraftProxy,
    ctx: Context<Data>,
) -> Result<ReconcilerAction, Error> {
    let trace_id = telemetry::get_trace_id();
    Span::current().record("trace_id", &field::display(&trace_id));
    let start = Instant::now();

    let client = ctx.get_ref().client.clone();
    ctx.get_ref().state.write().expect("last_event").last_event = Utc::now();
    let name = ResourceExt::name(&mcproxy);
    let ns = ResourceExt::namespace(&mcproxy).expect("failed to get mcproxy namespace");
    let configs: Vec<ConfigOptions> = mcproxy.spec.proxy.config.clone().unwrap_or(vec![]);

    let owner_reference = OwnerReference {
        controller: Some(true),
        ..crate::objects::object_to_owner_reference::<MinecraftProxy>(mcproxy.metadata.clone())?
    };
    let labels = BTreeMap::from_iter(IntoIter::new([(
        String::from("mycelium.njha.dev/mcproxy"),
        name.clone(),
    )]));
    let mut plugins: Vec<String> = mcproxy.spec.proxy.plugins.clone().unwrap_or(vec![]);
    // TODO: Point this at the jar cache, which is pointed at a CI server.
    plugins.push(format!(
        "https://www.ocf.berkeley.edu/~njha/artifacts/mycelium-velocity-plugin-{}-all.jar",
        env!("CARGO_PKG_VERSION"),
    ));
    let tags: BTreeMap<String, String> = mcproxy.labels().clone();
    let mut volume_mounts: Vec<VolumeMount> = configs.iter().map(make_volume_mount).collect();
    let mut volumes: Vec<Volume> = configs.iter().map(make_volume).collect();
    if let Some(volume) = mcproxy.spec.container.volume {
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
            replicas: Some(mcproxy.spec.replicas),
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(labels.clone()),
                    annotations: Some(vec![("prometheus.io/port".into(), "8080".into()),
                                           ("prometheus.io/scrape".into(), "true".into())]
                        .into_iter().collect()),
                    ..ObjectMeta::default()
                }),
                spec: Some(PodSpec {
                    security_context: mcproxy.spec.container.security_context,
                    containers: vec![Container {
                        name: name.clone(),
                        image: Some(format!("harbor.ocf.berkeley.edu/mycelium/runner:{}", env!("CARGO_PKG_VERSION"))),
                        image_pull_policy: Some(String::from("IfNotPresent")),
                        resources: mcproxy.spec.container.resources,
                        env: Some(vec![EnvVar {
                            name: String::from("MYCELIUM_RUNNER_KIND"),
                            value: Some(String::from("proxy")),
                            value_from: None,
                        }, EnvVar {
                            name: String::from("MYCELIUM_FW_TOKEN"),
                            value: Some(String::from(&ctx.get_ref().config.forwarding_secret)),
                            value_from: None,
                        }, EnvVar {
                            name: String::from("MYCELIUM_PLUGINS"),
                            value: Some(plugins.join(",")),
                            value_from: None,
                        }, EnvVar {
                            name: String::from("K8S_NAMESPACE"),
                            value: Some(ns.clone()),
                            value_from: None,
                        }, EnvVar {
                            name: String::from("MYCELIUM_ENV"),
                            value: Some(tags.get("mycelium.njha.dev/env")
                                .unwrap_or(&String::from("development")).clone()),
                            value_from: None,
                        }, EnvVar {
                            name: String::from("MYCELIUM_PROXY"),
                            value: Some(tags.get("mycelium.njha.dev/proxy")
                                .unwrap_or(&String::from("global")).clone()),
                            value_from: None,
                        }, EnvVar {
                            name: String::from("MYCELIUM_ENDPOINT"),
                            value: Some(env::var("MYCELIUM_ENDPOINT").unwrap()),
                            value_from: None,
                        }, EnvVar {
                            name: String::from("MYCELIUM_RUNNER_JAR_URL"),
                            value: Some(get_download_url(&mcproxy.spec.r#type, &mcproxy.spec.proxy.jar.version, &mcproxy.spec.proxy.jar.build)),
                            value_from: None,
                        }, EnvVar {
                            name: String::from("MYCELIUM_JVM_OPTS"),
                            value: mcproxy.spec.proxy.jvm,
                            value_from: None,
                        }]),
                        volume_mounts: Some(configs.iter().map(make_volume_mount).collect()),
                        ..Container::default()
                    }],
                    volumes: Some(configs.iter().map(make_volume).collect()),
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
            selector: Some(labels),
            ports: Some(vec![ServicePort {
                protocol: Some(String::from("TCP")),
                port: 25565,
                target_port: Some(IntOrString::Int(25577)),
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
        .proxy_reconcile_duration
        .with_label_values(&[])
        .observe(duration);
    ctx.get_ref().metrics.proxy_handled_events.inc();
    info!("Reconciled MinecraftProxy \"{}\" in {}", name, ns);

    Ok(ReconcilerAction {
        requeue_after: None,
    })
}