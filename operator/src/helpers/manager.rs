use std::sync::{Arc, RwLock};
use std::time::Duration;

use futures::future::BoxFuture;
use futures::FutureExt;
use futures::StreamExt;
use k8s_openapi::api::apps::v1::StatefulSet;
use kube::{Api, Client};
use kube::api::ListParams;
use kube_runtime::Controller;
use kube_runtime::controller::{Context, ReconcilerAction};
use prometheus::default_registry;
use prometheus::proto::MetricFamily;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::{Error, objects};
use crate::helpers::metrics::Metrics;
use crate::helpers::state::State;
use crate::objects::minecraft_proxy::MinecraftProxy;
use crate::objects::minecraft_set::{MinecraftSet, MinecraftSetSpec};

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
    /** lifecycle interface for mycelium CRDs returns both (a `Manager`, a future to be awaited) `fn main()` will await the future, exiting when this future returns */
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

pub fn error_policy(error: &Error, _ctx: Context<Data>) -> ReconcilerAction {
    warn!("reconcile failed: {:?}", error);
    ReconcilerAction {
        requeue_after: Some(Duration::from_secs(360)),
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

#[derive(Clone)]
pub struct MyceliumConfig {
    /// velocity forwarding secret
    pub(crate) forwarding_secret: String,
}

#[derive(Clone)]
pub struct Data {
    /// kubernetes API client
    pub(crate) client: Client,
    /// in memory state
    pub(crate) state: Arc<RwLock<State>>,
    /// prometheus metrics
    pub(crate) metrics: Metrics,
    /// parsed configuration
    pub(crate) config: MyceliumConfig,
}
