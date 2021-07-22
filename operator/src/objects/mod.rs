use std::collections::HashMap;
use std::fmt::Debug;
use std::iter::Map;
use std::ops::Range;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use futures::FutureExt;
use futures::StreamExt;
use k8s_openapi::api::apps::v1::StatefulSet;
use k8s_openapi::api::core::v1::{ConfigMapVolumeSource, Volume, VolumeMount};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};
use kube::{Api, Client, Resource};
use kube::api::ListParams;
use kube_runtime::Controller;
use kube_runtime::controller::{Context, ReconcilerAction};
use prometheus::{
    default_registry, HistogramOpts, HistogramVec, IntCounter,
    proto::MetricFamily, register_histogram_vec, register_int_counter,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, event, field, info, instrument, Level, Span, trace, warn};

use crate::Error;
use crate::MinecraftProxy;
use crate::MinecraftSet;
use crate::objects::minecraft_set::MinecraftSetSpec;

pub mod minecraft_set;
pub mod minecraft_proxy;

#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Clone, JsonSchema)]
pub struct ConfigOptions {
    pub name: String,
    pub path: String,
}

pub fn make_volume_mount(co: &ConfigOptions) -> VolumeMount {
    return VolumeMount {
        name: co.name.clone(),
        mount_path: String::from(Path::new("/config/").join(&co.path).to_str().expect("mount path")),
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

fn object_to_owner_reference<K: Resource<DynamicType=()>>(
    meta: ObjectMeta,
) -> Result<OwnerReference, Error> {
    Ok(OwnerReference {
        api_version: K::api_version(&()).to_string(),
        kind: K::kind(&()).to_string(),
        name: meta.name.unwrap(),
        uid: meta.uid.unwrap(),
        ..OwnerReference::default()
    })
}

fn error_policy(error: &Error, _ctx: Context<Data>) -> ReconcilerAction {
    warn!("reconcile failed: {:?}", error);
    ReconcilerAction {
        requeue_after: Some(Duration::from_secs(360)),
    }
}

/// a manager that owns a Controller
#[derive(Clone)]
pub struct Manager {
    /// in memory state
    state: Arc<RwLock<State>>,
    /// prometheus metrics
    metrics: Metrics,
    /// kube api
    client: Client,
}

impl Manager {
    /** lifecycle interface for mycelium CRDs
                  * returns both (a `Manager`, a future to be awaited)
                  * `fn main()` will await the future, exiting when this future returns */
    pub async fn new() -> (Self, BoxFuture<'static, ()>, BoxFuture<'static, ()>) {
        let client = Client::try_default().await.expect("create client");
        let metrics = Metrics::new();
        let state = Arc::new(RwLock::new(State::new()));
        // TODO: Get forwarding secret from a config file or something, which will
        // be passed during deployment.
        let set_context = Context::new(Data {
            client: client.clone(),
            metrics: metrics.clone(),
            state: state.clone(),
            config: MyceliumConfig { forwarding_secret: "TODO-INSECURE".to_string() },
        });
        let proxy_context = Context::new(Data {
            client: client.clone(),
            metrics: metrics.clone(),
            state: state.clone(),
            config: MyceliumConfig { forwarding_secret: "TODO-INSECURE".to_string() },
        });

        let mcsets = Api::<MinecraftSet>::all(client.clone());
        let mcproxies = Api::<MinecraftProxy>::all(client.clone());
        let statesets = Api::<StatefulSet>::all(client.clone());
        // ensure CRD is installed
        mcsets
            .list(&ListParams::default().limit(1))
            .await
            .expect("are the crds installed? install them with: mycelium-crdgen | kubectl apply -f -");

        // return the controller
        let set_controller = Controller::new(mcsets, ListParams::default())
            .owns(statesets.clone(), ListParams::default())
            .run(crate::objects::minecraft_set::reconcile,
                 error_policy,
                 set_context,
            )
            .for_each(|res| async move {
                match res {
                    Ok(o) => info!("reconciled {:?}", o),
                    Err(e) => warn!("reconcile failed: {}", e),
                }
            })
            .boxed();

        let proxy_controller = Controller::new(mcproxies, ListParams::default())
            .owns(statesets.clone(), ListParams::default())
            .run(crate::objects::minecraft_proxy::reconcile,
                 error_policy,
                 proxy_context,
            )
            .for_each(|res| async move {
                match res {
                    Ok(o) => info!("reconciled {:?}", o),
                    Err(e) => warn!("reconcile failed: {}", e),
                }
            })
            .boxed();

        (Self { state, metrics, client: client.clone() }, set_controller, proxy_controller)
    }

    /// metrics getter
    pub fn metrics(&self) -> Vec<MetricFamily> {
        default_registry().gather()
    }

    /// state getter
    pub async fn state(&self) -> State {
        self.state.read().expect("state getter").clone()
    }

    /// velocity server getter
    pub async fn velocity(&self, env: String, tag: String, ns: String) -> Vec<VelocityServerEntry> {
        let mcsets: Api<MinecraftSet> = Api::namespaced(self.client.clone(), &ns);
        let res = mcsets.list(
            &ListParams::default()
                .labels(&*format!("mycelium.njha.dev/proxy={}", tag))
                .labels(&*format!("mycelium.njha.dev/env={}", env))
        ).await.unwrap();
        let servers: Vec<VelocityServerEntry> = res.items.iter().flat_map(|set: &MinecraftSet| {
            let spec: &MinecraftSetSpec = &set.spec;
            (0..spec.replicas).map(move |val| -> VelocityServerEntry {
                VelocityServerEntry {
                    address: format!("{0}-{1}.{0}.{2}.svc.cluster.local", set.metadata.name.clone().unwrap(), val, set.metadata.namespace.clone().unwrap()),
                    host: None,
                    name: set.metadata.name.clone().unwrap(),
                }
            }).into_iter()
        }).collect();
        servers
    }
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct VelocityServerEntry {
    /// IP Address or DNS Name of minecraft server
    pub address: String,
    /// optional forced host
    pub host: Option<String>,
    /// unique name for server
    pub name: String,
}

/// prometheus metrics exposed on /metrics
#[derive(Clone)]
pub struct Metrics {
    pub set_handled_events: IntCounter,
    pub proxy_handled_events: IntCounter,
    pub set_reconcile_duration: HistogramVec,
    pub proxy_reconcile_duration: HistogramVec,
}

impl Metrics {
    fn new() -> Self {
        let set_reconcile_histogram = register_histogram_vec!(
            "mcset_controller_reconcile_duration_seconds",
            "The duration of mcset reconcile to complete in seconds",
            &[],
            vec![0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.]
        ).unwrap();

        let proxy_reconcile_histogram = register_histogram_vec!(
            "mcproxy_controller_reconcile_duration_seconds",
            "The duration of mcproxy reconcile to complete in seconds",
            &[],
            vec![0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.]
        ).unwrap();

        Metrics {
            set_handled_events: register_int_counter!("mcset_controller_handled_events", "mcset handled events").unwrap(),
            proxy_handled_events: register_int_counter!("proxy_controller_handled_events", "proxy handled events").unwrap(),
            set_reconcile_duration: set_reconcile_histogram,
            proxy_reconcile_duration: proxy_reconcile_histogram,
        }
    }
}

/// in-memory reconciler state exposed on /state
#[derive(Clone, Serialize)]
pub struct State {
    #[serde(deserialize_with = "from_ts")]
    pub last_event: DateTime<Utc>,
}

impl State {
    fn new() -> Self {
        State {
            last_event: Utc::now(),
        }
    }
}

#[derive(Clone)]
pub struct MyceliumConfig {
    /// velocity forwarding secret
    forwarding_secret: String,
}

#[derive(Clone)]
pub struct Data {
    /// kubernetes API client
    client: Client,
    /// in memory state
    state: Arc<RwLock<State>>,
    /// prometheus metrics
    metrics: Metrics,
    /// parsed configuration
    config: MyceliumConfig,
}