use std::{
    env,
    sync::{Arc, RwLock},
    time::Duration,
};

use futures::{future::BoxFuture, FutureExt, StreamExt};
use k8s_openapi::api::apps::v1::StatefulSet;
use kube::{api::ListParams, Api, Client};
use kube_runtime::{
    controller::{Context, ReconcilerAction},
    Controller,
};
use prometheus::{default_registry, proto::MetricFamily};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::{
    helpers::{metrics::Metrics, state::State},
    objects,
    objects::{
        minecraft_proxy::MinecraftProxy,
        minecraft_set::{MinecraftSet, MinecraftSetSpec},
    },
    Error,
};
use crate::objects::minecraft_proxy::MinecraftProxySpec;

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
    /// lifecycle interface for mycelium CRDs returns both (a `Manager`, a
    /// future to be awaited) `fn main()` will await the future, exiting when
    /// this future returns
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
            config: MyceliumConfig {
                forwarding_secret: env::var("MYCELIUM_FW_TOKEN").unwrap(),
                runner_image: env::var("MYCELIUM_RUNNER_IMAGE").unwrap(),
            },
        });
        let proxy_context = Context::new(Data {
            client: client.clone(),
            metrics: metrics.clone(),
            state: state.clone(),
            config: MyceliumConfig {
                forwarding_secret: env::var("MYCELIUM_FW_TOKEN").unwrap(),
                runner_image: env::var("MYCELIUM_RUNNER_IMAGE").unwrap(),
            },
        });

        let mcsets = Api::<MinecraftSet>::all(client.clone());
        let mcproxies = Api::<MinecraftProxy>::all(client.clone());
        let statesets = Api::<StatefulSet>::all(client.clone());
        // ensure CRD is installed
        mcsets.list(&ListParams::default().limit(1)).await.expect(
            "are the crds installed? install them with: mycelium-crdgen | kubectl apply -f -",
        );

        // return the controller
        let set_controller = Controller::new(mcsets, ListParams::default())
            .owns(statesets.clone(), ListParams::default())
            .run(
                crate::objects::minecraft_set::reconcile,
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
            .run(
                crate::objects::minecraft_proxy::reconcile,
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

        (
            Self {
                state,
                metrics,
                client: client.clone(),
            },
            set_controller,
            proxy_controller,
        )
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
    pub async fn get_sets(&self, ns: String, name: String) -> Result<Vec<VelocityServerEntry>, Error> {
        let proxy_api: Api<MinecraftProxy> = Api::namespaced(self.client.clone(), &ns);
        let proxy: MinecraftProxy = proxy_api.get(&name).await?;
        let proxy_spec: MinecraftProxySpec = proxy.spec;

        let label_selector = proxy_spec.selector.unwrap_or_default()
            .match_labels.unwrap_or_default()
            .iter().map(|i| format!("{}={}", i.0, i.1))
            .collect::<Vec<String>>().join(",");

        let mcset_api: Api<MinecraftSet> = Api::namespaced(self.client.clone(), &ns);
        let objects = mcset_api.list(&ListParams::default().labels(&label_selector)).await?;

        Ok(objects.items.iter().flat_map(|set: &MinecraftSet| {
            let spec: &MinecraftSetSpec = &set.spec;
            let proxy = spec.proxy.clone().unwrap_or_default();
            (0..spec.replicas)
                .map(move |val| -> VelocityServerEntry {
                    VelocityServerEntry {
                        address: format!(
                            "{0}-{1}.{0}.{2}.svc.cluster.local",
                            set.metadata.name.clone().unwrap(),
                            val,
                            set.metadata.namespace.clone().unwrap()
                        ),
                        host: proxy.hostname.clone(),
                        name: format!("{}-{}", set.metadata.name.clone().unwrap(), val),
                        priority: proxy.priority.clone(),
                    }
                })
                .into_iter()
        }).collect())
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
    /// priority for default list
    pub priority: Option<u32>,
}

#[derive(Clone)]
pub struct MyceliumConfig {
    /// velocity forwarding secret
    pub(crate) forwarding_secret: String,
    /// runner image
    pub(crate) runner_image: String,
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
