use crate::types::{bootstrap, storage, AnnounceAddr};
use crate::{telemetry, Context, IpfsNode, Metrics, NodeKind};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use kube::{
    api::{Api, ListParams, ResourceExt},
    client::Client,
    runtime::{
        controller::{Action, Controller},
        events::Reporter,
        finalizer::{finalizer, Event as Finalizer},
        watcher::Config,
    },
};
use operator_common::{
    labels, selector_labels,
    types::{configmap, service, statefulset},
    ActionType, Error, Result,
};
use serde::Serialize;
use std::sync::Arc;
use tokio::{sync::RwLock, time::Duration};
use tracing::*;

pub static IPFSNODE_FINALIZER: &str = "ipfsnodes.gevulot.com/finalizer";

#[instrument(skip(ctx, ipfsnode), fields(trace_id))]
async fn reconcile(ipfsnode: Arc<IpfsNode>, ctx: Arc<Context>) -> Result<Action> {
    let trace_id = telemetry::get_trace_id();
    Span::current().record("trace_id", &field::display(&trace_id));
    let _timer = ctx.metrics.count_and_measure();
    ctx.diagnostics.write().await.last_event = Utc::now();
    let ns = ipfsnode.namespace().unwrap(); // ipfsnode is namespace scoped
    let ipfsnodes: Api<IpfsNode> = Api::namespaced(ctx.client.clone(), &ns);

    info!("Reconciling IpfsNode \"{}\" in {}", ipfsnode.name_any(), ns);
    finalizer(&ipfsnodes, IPFSNODE_FINALIZER, ipfsnode, |event| async {
        match event {
            Finalizer::Apply(ipfsnode) => ipfsnode.reconcile(ctx.clone()).await,
            Finalizer::Cleanup(ipfsnode) => ipfsnode.cleanup(ctx.clone()).await,
        }
    })
    .await
    .map_err(|e| Error::FinalizerError(Box::new(e)))
}

fn error_policy(ipfsnode: Arc<IpfsNode>, error: &Error, ctx: Arc<Context>) -> Action {
    warn!("reconcile failed: {:?}", error);
    ctx.metrics.reconcile_failure(&ipfsnode, error);
    Action::requeue(Duration::from_secs(5 * 60))
}

impl IpfsNode {
    // Reconcile (for non-finalizer related changes)
    async fn reconcile(&self, ctx: Arc<Context>) -> Result<Action> {
        let client = ctx.client.clone();
        let namespace = self.namespace().unwrap();
        let name = self.name_any();

        let action = if self.metadata.finalizers.is_none() {
            ActionType::Create
        } else {
            ActionType::Update
        };

        match self.spec.kind {
            NodeKind::BootStrap => {
                bootstrap::deploy(
                    client.clone(),
                    name.clone(),
                    namespace.clone(),
                    self.spec.clone(),
                    action,
                    (
                        labels(name.clone(), NodeKind::BootStrap.to_string()),
                        selector_labels(name.clone(), NodeKind::BootStrap.to_string()),
                    ),
                )
                .await?;
            }
            NodeKind::Storage => {
                storage::deploy(
                    client.clone(),
                    name.clone(),
                    namespace.clone(),
                    self.spec.clone(),
                    AnnounceAddr::StandardCluster("1.2.3.4".to_string()),
                    (
                        labels(name.clone(), NodeKind::Storage.to_string()),
                        selector_labels(name.clone(), NodeKind::Storage.to_string()),
                    ),
                )
                .await?;
            }
        }

        Ok(Action::requeue(Duration::from_secs(5 * 60)))
    }

    // Finalizer cleanup (the object was deleted, ensure nothing is orphaned)
    async fn cleanup(&self, ctx: Arc<Context>) -> Result<Action> {
        let client = ctx.client.clone();
        let namespace = self.namespace().unwrap();
        let name = self.name_any();

        for idx in 0..self.spec.replicas {
            service::delete(
                client.clone(),
                format!("{name}-p2p-{idx}"),
                namespace.clone(),
            )
            .await?;
        }

        configmap::delete(
            client.clone(),
            format!("{name}-external-addresses"),
            namespace.clone(),
        )
        .await?;

        statefulset::delete(client.clone(), name.clone(), namespace.clone()).await?;

        Ok(Action::await_change())
    }
}

/// Diagnostics to be exposed by the web server
#[derive(Clone, Serialize)]
pub struct Diagnostics {
    #[serde(deserialize_with = "from_ts")]
    pub last_event: DateTime<Utc>,
    #[serde(skip)]
    pub reporter: Reporter,
}
impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            last_event: Utc::now(),
            reporter: "ipfs-operator".into(),
        }
    }
}

/// State shared between the controller and the web server
#[derive(Clone, Default)]
pub struct State {
    /// Diagnostics populated by the reconciler
    diagnostics: Arc<RwLock<Diagnostics>>,
    /// Metrics registry
    registry: prometheus::Registry,
}

/// State wrapper around the controller outputs for the web server
impl State {
    /// Metrics getter
    pub fn metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.registry.gather()
    }

    /// State getter
    pub async fn diagnostics(&self) -> Diagnostics {
        self.diagnostics.read().await.clone()
    }

    // Create a Controller Context that can update State
    pub fn to_context(&self, client: Client) -> Arc<Context> {
        Arc::new(Context {
            client,
            metrics: Metrics::default().register(&self.registry).unwrap(),
            diagnostics: self.diagnostics.clone(),
        })
    }
}

/// Initialize the controller and shared state (given the crd is installed)
pub async fn run(state: State) {
    let client = Client::try_default()
        .await
        .expect("failed to create kube Client");
    let ipfsnode = Api::<IpfsNode>::all(client.clone());
    if let Err(e) = ipfsnode.list(&ListParams::default().limit(1)).await {
        error!("CRD is not queryable; {e:?}. Is the CRD installed?");
        info!("Installation: cargo run --bin crdgen | kubectl apply -f -");
        std::process::exit(1);
    }
    Controller::new(ipfsnode, Config::default().any_semantic())
        .shutdown_on_signal()
        .run(reconcile, error_policy, state.to_context(client))
        .filter_map(|x| async move { Result::ok(x) })
        .for_each(|_| futures::future::ready(()))
        .await;
}
