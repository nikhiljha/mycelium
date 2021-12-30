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
    helpers::{jarapi::get_download_url, manager::Data, telemetry},
    objects::{
        generic_reconcile, make_volume, make_volume_mount, ConfigOptions, ContainerOptions,
        RunnerOptions,
    },
    Error, Result,
};
use crate::Error::MyceliumError;

#[derive(CustomResource, Serialize, Deserialize, Default, Debug, PartialEq, Clone, JsonSchema)]
#[kube(
    group = "mycelium.njha.dev",
    version = "v1beta1",
    kind = "MinecraftProxy",
    plural = "minecraftproxies"
)]
#[kube(shortname = "mcproxy", namespaced)]
pub struct MinecraftProxySpec {
    pub replicas: i32,
    pub r#type: String,
    pub runner: RunnerOptions,
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

    let name = ResourceExt::name(&mcproxy);
    let ns = ResourceExt::namespace(&mcproxy)
        .ok_or(MyceliumError("failed to get namespace".into()))?;
    let owner_reference = OwnerReference {
        controller: Some(true),
        ..crate::objects::object_to_owner_reference::<MinecraftProxy>(mcproxy.metadata.clone())?
    };
    let tags: BTreeMap<String, String> = mcproxy.labels().clone();

    generic_reconcile(
        vec![
            EnvVar {
                name: String::from("MYCELIUM_RUNNER_KIND"),
                value: Some(String::from("proxy")),
                value_from: None,
            },
            EnvVar {
                name: String::from("MYCELIUM_PLUGINS"),
                value: Some(mcproxy
                    .spec
                    .runner
                    .plugins
                    .clone()
                    .unwrap_or(vec![])
                    .into_iter()
                    .chain(vec![format!(
                        "https://www.ocf.berkeley.edu/~njha/artifacts/mycelium-velocity-plugin-{}-all.jar",
                        env!("CARGO_PKG_VERSION"),
                    )].into_iter())
                    .collect::<Vec<String>>().join(",")),
                value_from: None,
            },
            EnvVar {
                name: String::from("MYCELIUM_ENV"),
                value: Some(
                    tags.get("mycelium.njha.dev/env")
                        .unwrap_or(&String::from("development"))
                        .clone(),
                ),
                value_from: None,
            },
            EnvVar {
                name: String::from("MYCELIUM_PROXY"),
                value: Some(
                    tags.get("mycelium.njha.dev/proxy")
                        .unwrap_or(&String::from("cluster"))
                        .clone(),
                ),
                value_from: None,
            },
            EnvVar {
                name: String::from("MYCELIUM_ENDPOINT"),
                value: Some(env::var("MYCELIUM_ENDPOINT").unwrap()),
                value_from: None,
            },
            EnvVar {
                name: String::from("MYCELIUM_RUNNER_JAR_URL"),
                value: Some(get_download_url(
                    &mcproxy.spec.r#type,
                    &mcproxy.spec.runner.jar.version,
                    &mcproxy.spec.runner.jar.build,
                )),
                value_from: None,
            },
            EnvVar {
                name: String::from("K8S_NAMESPACE"),
                value: Some(ns.clone()),
                value_from: None,
            },
        ],
        IntOrString::Int(25577),
        name.clone(),
        ns.clone(),
        ctx.clone(),
        owner_reference,
        "mcproxy".to_string(),
        mcproxy.spec.replicas,
        mcproxy.spec.container,
        mcproxy.spec.runner,
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
