use base64::prelude::*;
use kube::Client;
use libp2p::PeerId;
use operator_common::{
    ActionType, Error,
    types::configmap::{self, get_data},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt::Display};
use tracing::{debug, error, instrument, warn};

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
        action: ActionType,
        labels: BTreeMap<String, String>,
    ) -> Result<Self, Error> {
        debug!("Creating new identities, action: {:?}", action);
        let identities = match action {
            ActionType::Create => _create(name, 0, replicas as usize).await?,
            ActionType::Update => {
                let mut identities = match &get_data(
                    client.clone(),
                    &format!("{name}-identities"),
                    &namespace,
                )
                .await
                {
                    Ok(i) => string_to_identity(i)?,
                    Err(_) => {
                        warn!(
                            "Creating {name}-identities even though it really should have existed!"
                        );
                        _create(name, 0, replicas as usize).await?
                    }
                };

                let identities_count = identities.len();

                if identities_count > replicas as usize {
                    for idx in (replicas as usize)..identities_count {
                        identities.remove(&format!("{name}-{idx}"));
                    }
                    identities
                } else if identities_count < replicas as usize {
                    _create(name, identities_count, replicas as usize).await?
                } else {
                    identities
                }
            }
        };

        configmap::deploy(
            client.clone(),
            &format!("{name}-identities"),
            &namespace,
            identity_to_string(&identities)?,
            labels,
        )
        .await?;

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
