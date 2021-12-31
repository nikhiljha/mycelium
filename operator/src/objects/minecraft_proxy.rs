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
use k8s_openapi::api::core::v1::{EnvVarSource, ObjectFieldSelector};
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
    /// number of identical proxies to create
    pub replicas: i32,

    /// options for the server runner
    pub runner: RunnerOptions,

    /// options for Kubernetes
    pub container: Option<ContainerOptions>,

    /// what MinecraftSets to add to this proxy (only matchLabels is supported)
    pub selector: Option<LabelSelector>,
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
                name: String::from("MYCELIUM_ENDPOINT"),
                value: Some(env::var("MYCELIUM_ENDPOINT").unwrap()),
                value_from: None,
            },
            EnvVar {
                name: String::from("K8S_NAMESPACE"),
                value: None,
                value_from: Some(EnvVarSource {
                    field_ref: Some(ObjectFieldSelector {
                        api_version: None,
                        field_path: "metadata.namespace".to_string()
                    }),
                    ..EnvVarSource::default()
                }),
            },
            EnvVar {
                name: String::from("K8S_NAME"),
                value: Some(name.clone()),
                value_from: Some(EnvVarSource {
                    field_ref: Some(ObjectFieldSelector {
                        api_version: None,
                        field_path: "metadata.name".to_string()
                    }),
                    ..EnvVarSource::default()
                }),
            },
        ],
        IntOrString::Int(25577),
        name.clone(),
        ns.clone(),
        ctx.clone(),
        owner_reference,
        "mcproxy".to_string(),
        mcproxy.spec.replicas,
        mcproxy.spec.container.unwrap_or_default(),
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
