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
    kind = "MinecraftSet"
)]
#[kube(shortname = "mcset", namespaced)]
pub struct MinecraftSetSpec {
    /// number of identical servers to create
    pub replicas: i32,

    /// options for the server runner
    pub runner: RunnerOptions,

    /// options for Kubernetes
    pub container: ContainerOptions,

    /// options to pass to proxies that select this MinecraftSet
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

    let name = ResourceExt::name(&mcset);
    let ns = ResourceExt::namespace(&mcset)
        .ok_or(MyceliumError("failed to get namespace".into()))?;
    let owner_reference = OwnerReference {
        controller: Some(true),
        ..crate::objects::object_to_owner_reference::<MinecraftSet>(mcset.metadata.clone())?
    };

    generic_reconcile(
        vec![
            EnvVar {
                name: String::from("MYCELIUM_RUNNER_KIND"),
                value: Some(String::from("game")),
                value_from: None,
            },
            EnvVar {
                name: String::from("MYCELIUM_PLUGINS"),
                value: Some(mcset.spec.runner.plugins.clone().unwrap_or(vec![]).join(",")),
                value_from: None,
            },
        ],
        IntOrString::Int(25565),
        name.clone(),
        ns.clone(),
        ctx.clone(),
        owner_reference,
        "mcset".to_string(),
        mcset.spec.replicas.clone(),
        mcset.spec.container,
        mcset.spec.runner,
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
