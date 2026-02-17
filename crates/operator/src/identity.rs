use base64::prelude::*;
use kube::Client;
use libp2p::PeerId;
use operator_common::{
    Error,
    types::configmap::{self},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt::Display};
use tracing::{debug, error, instrument};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identities {
    pub ids: BTreeMap<String, Identity>,
}

impl Identities {
    #[instrument(skip(client))]
    pub async fn new(
        client: Client,
        name: &str,
        namespace: &str,
        replicas: i32,
        labels: BTreeMap<String, String>,
    ) -> Result<Self, Error> {
        let mut identities: BTreeMap<String, Identity> = BTreeMap::new();

        if let Some(id) =
            configmap::get_data_opt(client.clone(), &format!("{name}-identities"), namespace)
                .await?
        {
            for (key, json) in id {
                identities.insert(key, serde_json::from_str::<Identity>(&json)?);
            }
        } else {
            debug!("Creating new identities");
            for idx in 0..replicas {
                identities.insert(format!("{name}-{idx}"), Identity::new()?);
            }

            configmap::deploy(
                client.clone(),
                &format!("{name}-identities"),
                namespace,
                identity_to_string(&identities)?,
                labels,
            )
            .await?;
        };

        Ok(Self { ids: identities })
    }

    pub async fn get(client: Client, name: &str, namespace: &str) -> Result<Self, Error> {
        let identities =
            string_to_identity(&configmap::get_data(client.clone(), name, namespace).await?)?;
        Ok(Self { ids: identities })
    }
}

async fn _create(
    name: &str,
    start: usize,
    end: usize,
) -> Result<BTreeMap<String, Identity>, Error> {
    let mut identities: BTreeMap<String, Identity> = BTreeMap::new();
    for idx in start..end {
        identities.insert(format!("{name}-{idx}"), Identity::new()?);
    }
    Ok(identities)
}

impl Identities {
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    // pub fn as_string(&self) -> Result<BTreeMap<String, String>, Error> {
    //     self.ids
    //         .iter()
    //         .map(|(k, v)| Ok((k.clone(), serde_json::to_string(v)?)))
    //         .collect()
    // }

    // pub fn as_identities(&self) -> BTreeMap<String, Identity> {
    //     self.ids.clone()
    // }
}

fn string_to_identity(
    input: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, Identity>, serde_json::Error> {
    input
        .iter()
        .map(|(k, json)| Ok((k.clone(), serde_json::from_str::<Identity>(json)?)))
        .collect()
}

fn identity_to_string(
    input: &BTreeMap<String, Identity>,
) -> Result<BTreeMap<String, String>, serde_json::Error> {
    input
        .iter()
        .map(|(k, v)| Ok((k.clone(), serde_json::to_string(v)?)))
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub peer_id: String,
    pub priv_key: String,
}

impl Identity {
    pub fn new() -> Result<Self, Error> {
        let keypair_raw = libp2p::identity::ed25519::Keypair::generate();
        let keypair = libp2p::identity::Keypair::from(keypair_raw);
        let secret = match keypair.to_protobuf_encoding() {
            Ok(secret) => secret,
            Err(err) => {
                return Err(Error::DecodeKeyError(err.to_string()));
            }
        };
        let peer_id = PeerId::from(keypair.public()).to_string();
        Ok(Self {
            peer_id,
            priv_key: BASE64_STANDARD.encode(secret),
        })
    }
}

impl Display for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let id = match serde_json::to_string(&self) {
            Ok(json) => json,
            Err(err) => {
                error!("Failed to serialize Identity: {}", err);
                return Err(std::fmt::Error);
            }
        };
        write!(f, "{id}")
    }
}
