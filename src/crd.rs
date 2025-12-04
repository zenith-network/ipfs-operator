use crate::{Diagnostics, Metrics};
use kube::{Client, CustomResource};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, sync::Arc};
use tokio::sync::RwLock;

/// Generate the Kubernetes wrapper struct `Document` from our Spec and Status struct
///
#[derive(CustomResource, Serialize, Deserialize, Debug, PartialEq, Clone, JsonSchema)]
#[kube(
    group = "gevulot.com",
    version = "v1",
    kind = "IpfsNode",
    plural = "ipfsnodes",
    derive = "PartialEq",
    namespaced
)]
pub struct NodeSpec {
    pub replicas: i32,
    pub image: Image,
    pub persistence: Persistence,
    pub rust_log: String,
    pub kind: NodeKind,
    pub p2p_port: Option<i32>,
}

impl Default for NodeSpec {
    fn default() -> Self {
        Self {
            replicas: 1,
            image: Image::default(),
            persistence: Persistence::default(),
            rust_log: "info".to_string(),
            kind: NodeKind::default(),
            p2p_port: Some(4001),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Image {
    pub repository: Option<String>,
    pub tag: Option<String>,
    pub pull_policy: Option<String>,
}

impl Default for Image {
    fn default() -> Self {
        Self {
            repository: Some("docker.io/ipfs/kubo".to_string()),
            tag: Some("latest".to_string()),
            pull_policy: Some("IfNotPresent".to_string()),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Clone, JsonSchema)]
pub enum NodeKind {
    #[default]
    Storage,
    BootStrap,
}

impl Display for NodeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeKind::BootStrap => write!(f, "bootstrap"),
            NodeKind::Storage => write!(f, "storage"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Persistence {
    pub access_mode: Option<String>,
    pub size: Option<String>,
    pub storage_class_name: Option<String>,
    pub existing_claim: Option<String>,
}

impl Default for Persistence {
    fn default() -> Self {
        Self {
            access_mode: Some("RWO".to_string()),
            size: Some("10Gi".to_string()),
            storage_class_name: Some("standard-rwo".to_string()),
            existing_claim: Default::default(),
        }
    }
}

// #[derive(Serialize, Deserialize, Debug, PartialEq, Clone, JsonSchema)]
// #[cfg_attr(test, derive(Default))]
// #[serde(rename_all = "camelCase")]
// pub struct Ingress {
//     pub host: String,
//     pub path: String,
//     pub service_port: i32,
// }

// Context for our reconciler
#[derive(Clone)]
pub struct Context {
    /// Kubernetes client
    pub client: Client,
    /// Diagnostics read by the web server
    pub diagnostics: Arc<RwLock<Diagnostics>>,
    /// Prometheus metrics
    pub metrics: Metrics,
}
