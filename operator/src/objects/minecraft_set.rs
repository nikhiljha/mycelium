use std::{collections::HashMap, sync::Arc};
use std::array::IntoIter;
use std::collections::BTreeMap;
use std::iter::FromIterator;

use chrono::prelude::*;
use futures::{future::BoxFuture, FutureExt, StreamExt};
use k8s_openapi::api::apps::v1::{StatefulSet, StatefulSetSpec};
use k8s_openapi::api::core::v1::{Container, EnvVar, PodSpec, PodTemplateSpec, Service, ServicePort, ServiceSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta, OwnerReference};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::{
    api::{Api, ListParams, Patch, PatchParams, ResourceExt},
    client::Client,
    CustomResource,
    Resource,
};
use kube_runtime::controller::{Context, Controller, ReconcilerAction};
use maplit::hashmap;
use prometheus::{
    default_registry, HistogramOpts, HistogramVec, IntCounter,
    proto::MetricFamily, register_histogram_vec, register_int_counter,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::{
    sync::RwLock,
    time::{Duration, Instant},
};
use tracing::{debug, error, event, field, info, instrument, Level, Span, trace, warn};

use crate::{Error, Result};
use crate::helpers::manager::Data;
use crate::helpers::telemetry;
use crate::objects::{ConfigOptions, make_volume, make_volume_mount};

#[derive(CustomResource, Serialize, Deserialize, Default, Debug, PartialEq, Clone, JsonSchema)]
#[kube(group = "mycelium.njha.dev", version = "v1alpha1", kind = "MinecraftSet")]
#[kube(shortname = "mcset", namespaced)]
pub struct MinecraftSetSpec {
    pub replicas: i32,
    pub r#type: String,
    pub config: Option<Vec<ConfigOptions>>,
    pub plugins: Option<Vec<String>>,
    pub proxy: Option<ProxyOptions>,
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
    // TODO: This will panic on failure. Although it *should* never fail, this should still be fixed.
    ctx.get_ref().state.write().expect("last_event").last_event = Utc::now();
    // TODO: This will panic on failure. Although it *should* never fail, this should still be fixed.
    let name = ResourceExt::name(&mcset);
    let ns = ResourceExt::namespace(&mcset).expect("failed to get mcset namespace");
    let configs: Vec<ConfigOptions> = mcset.spec.config.unwrap_or(vec![]);

    let owner_reference = OwnerReference {
        controller: Some(true),
        ..crate::objects::object_to_owner_reference::<MinecraftSet>(mcset.metadata.clone())?
    };
    let labels = BTreeMap::from_iter(
        IntoIter::new([(String::from("mycelium.njha.dev/mcset"), name.clone())])
    );
    let statefulset = StatefulSet {
        metadata: ObjectMeta {
            name: Some(name.clone()),
            owner_references: vec![owner_reference.clone()],
            ..ObjectMeta::default()
        },
        spec: Some(StatefulSetSpec {
            selector: LabelSelector {
                match_labels: labels.clone(),
                ..LabelSelector::default()
            },
            service_name: name.clone(),
            replicas: Some(mcset.spec.replicas.clone()),
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: labels.clone(),
                    ..ObjectMeta::default()
                }),
                spec: Some(PodSpec {
                    containers: vec![Container {
                        name: name.clone(),
                        image: Some(String::from("ci.njha.dev/mycelium/runner:latest")),
                        image_pull_policy: Some(String::from("IfNotPresent")),
                        env: vec![EnvVar {
                            name: String::from("MYCELIUM_RUNNER_KIND"),
                            value: Some(String::from("game")),
                            value_from: None,
                        }, EnvVar {
                            name: String::from("MYCELIUM_FW_TOKEN"),
                            value: Some(String::from(&ctx.get_ref().config.forwarding_secret)),
                            value_from: None,
                        }, EnvVar {
                            name: String::from("MYCELIUM_PLUGINS"),
                            value: Some(mcset.spec.plugins.unwrap_or(vec![]).join(",")),
                            value_from: None,
                        }],
                        volume_mounts: configs.iter().map(make_volume_mount).collect(),
                        ..Container::default()
                    }],
                    volumes: configs.iter().map(make_volume).collect(),
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
            owner_references: vec![owner_reference],
            ..ObjectMeta::default()
        },
        spec: Some(ServiceSpec {
            // https://kubernetes.io/docs/concepts/services-networking/service/#headless-services
            cluster_ip: Some(String::from("None")),
            selector: labels,
            ports: vec![ServicePort {
                protocol: Some(String::from("TCP")),
                port: 25565,
                target_port: Some(IntOrString::Int(25565)),
                ..ServicePort::default()
            }],
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
        .await
        .unwrap();

    kube::Api::<Service>::namespaced(client.clone(), &ns)
        .patch(
            &name,
            &kube::api::PatchParams::apply("mycelium.njha.dev"),
            &kube::api::Patch::Apply(&service),
        )
        .await
        .unwrap();

    let duration = start.elapsed().as_millis() as f64 / 1000.0;
    ctx.get_ref()
        .metrics
        .set_reconcile_duration
        .with_label_values(&[])
        .observe(duration);
    ctx.get_ref().metrics.set_handled_events.inc();
    info!("Reconciled MinecraftSet \"{}\" in {}", name, ns);

    /*
     * TODO: Do we need to check back if this succeeded & no changes were made?
     * i.e. Do we want to revert manual edits to StatefulSets or Services on a timer?
     */
    Ok(ReconcilerAction {
        requeue_after: None,
    })
}
