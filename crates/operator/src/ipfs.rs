use std::collections::BTreeMap;

use kube::Client;
use operator_common::{
    Error,
    types::configmap::{self, get_data},
};
use serde_json::Value;
use tracing::{info, instrument};

use crate::identity::Identities;

const OPERATOR_NAMESPACE: &str = "ipfs-system";

#[instrument(skip(client, identities))]
pub async fn generate_config(
    client: Client,
    name: &str,
    identities: Identities,
    external_addrs: BTreeMap<String, String>,
    bootstrap_list: Vec<String>,
    labels: BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, Error> {
    let conf = get_data(client, "default-ipfs-config", OPERATOR_NAMESPACE).await?;

    if !conf.contains_key("config") {
        return Err(Error::ConfigMapError(
            "ConfigMap missing data at key 'config'".to_string(),
        ));
    }

    let mut configs: BTreeMap<String, String> = BTreeMap::new();

    for (idx, id) in identities.ids {
        let mut config: Value = serde_json::from_str(conf["config"].as_str())?;
        config["Identity"]["PeerID"] = Value::String(id.peer_id);
        config["Identity"]["PrivKey"] = Value::String(id.priv_key);
        let external_addr = external_addrs
            .get(&idx)
            .ok_or(Error::ExternalAddressMissing(format!(
                "{idx} doesn't exist"
            )))?;
        config["Addresses"]["Announce"] = Value::Array(vec![Value::String(format!(
            "/ip4/{external_addr}/tcp/4001"
        ))]);
        config["Bootstrap"] = Value::Array(
            bootstrap_list
                .iter()
                .map(|x| Value::String(x.to_string()))
                .collect(),
        );
        config["AutoConf"]["Enabled"] = Value::Bool(false);
        config["Routing"]["Type"] = Value::String("dht".to_string());
        config["AutoTLS"]["Enabled"] = Value::Bool(false);
        config["Swarm"]["Transports"]["Network"]["Websocket"] = Value::Bool(false);
        config["DNS"]["Resolvers"] = Value::Null;
        config["Routing"]["DelegatedRouters"] = Value::Array(vec![]);
        config["Ipns"]["DelegatedPublishers"] = Value::Array(vec![]);

        configs.insert(idx, serde_json::to_string_pretty(&config)?);
    }

    info!("bootstrap_list: {bootstrap_list:?}");
    Ok(configs)
}

#[instrument(skip(identities))]
pub async fn get_bootstrap_list(
    identities: Identities,
    external_addrs: BTreeMap<String, String>,
) -> Result<Vec<String>, Error> {
    if identities.len() != external_addrs.len() {
        return Err(Error::ConfigMapError(
            "Number of identities and external addresses do not match".to_string(),
        ));
    }

    let mut bootstrap_list: Vec<String> = Vec::new();

    for (idx, id) in identities.ids.iter() {
        let external_addr = external_addrs
            .get(idx)
            .ok_or(Error::ExternalAddressMissing(format!(
                "{idx} doesn't exist"
            )))?;
        bootstrap_list.push(format!("/ip4/{external_addr}/tcp/4001/ipfs/{}", id.peer_id));
    }

    Ok(bootstrap_list)
}

#[instrument(skip(client))]
pub async fn copy_default_startup_scripts(
    client: Client,
    name: &str,
    namespace: &str,
    labels: BTreeMap<String, String>,
) -> Result<(), Error> {
    let scripts = get_data(
        client.clone(),
        "default-startup-scripts",
        OPERATOR_NAMESPACE,
    )
    .await?;

    if !scripts.contains_key("start_ipfs") || !scripts.contains_key("entrypoint.sh") {
        return Err(Error::ConfigMapError(
            "ConfigMap missing data at key 'start_ipfs' or 'entrypoint.sh'".to_string(),
        ));
    }

    configmap::deploy(
        client.clone(),
        &format!("{name}-startup-scripts"),
        namespace,
        scripts,
        labels,
    )
    .await?;

    Ok(())
}
