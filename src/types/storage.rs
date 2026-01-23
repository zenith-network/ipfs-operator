use crate::containers;
use crate::crd::NodeSpec;
use crate::identity::Identity;
use crate::types::common::{Common, ipfs_cluster_ports, ipfs_ports};
use k8s_openapi::ByteString;
use k8s_openapi::api::apps::v1::{StatefulSet, StatefulSetSpec};
use k8s_openapi::api::core::v1::{PodSpec, PodTemplateSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use kube::api::{ObjectMeta, Patch, PatchParams};
use kube::core::ErrorResponse;
use kube::{Api, Client, Error};
use operator_common::external_address_name;
use operator_common::types::{configmap, load_balancer, secret, statefulset};
use rand::{Rng, thread_rng};
use std::collections::BTreeMap;
use std::string::ToString;
use tracing::{Level, event, info, instrument};

#[instrument(skip(client))]
pub async fn deploy(
    client: Client,
    name: String,
    namespace: String,
    spec: NodeSpec,
    labels: (BTreeMap<String, String>, BTreeMap<String, String>),
) -> Result<StatefulSet, Error> {
    // Ensure secrets exist
    match upsert_peer_info(client.clone(), &name, &namespace, labels.clone()).await {
        Ok(_) => {}
        Err(err) => {
            return Err(Error::Api(ErrorResponse {
                status: "Failed".to_string(),
                message: err.to_string(),
                reason: "Failed to create load balancers for p2p port".to_string(),
                code: 418,
            }));
        }
    };

    match upsert_cluster_secret(client.clone(), &name, &namespace, labels.clone()).await {
        Ok(_) => {}
        Err(err) => {
            return Err(Error::Api(ErrorResponse {
                status: "Failed".to_string(),
                message: err.to_string(),
                reason: "Failed to create load balancers for p2p port".to_string(),
                code: 418,
            }));
        }
    };

    let bootstrap_name = spec
        .bootstrap_name
        .clone()
        .unwrap_or("bootstrap".to_string());

    let mut volumes = containers::ipfs::volumes(&name, &bootstrap_name);
    let mut containers = vec![containers::ipfs::container(&name, &spec)];

    let mut pvc = vec![containers::pvc(
        "ipfs-data",
        spec.ipfs.persistence.clone(),
        labels.clone(),
    )];

    let mut ports = ipfs_ports();

    if let Some(ipfs_cluster) = spec.ipfs_cluster.clone() {
        pvc.push(containers::pvc(
            "ipfs-cluster-data",
            ipfs_cluster.persistence.clone(),
            labels.clone(),
        ));

        volumes.extend(containers::ipfs_cluster::volumes(&name).iter().cloned());
        containers.push(containers::ipfs_cluster::container(&name, &ipfs_cluster));

        ports.append(&mut ipfs_cluster_ports());
    }

    let mut common = Common::new(
        client.clone(),
        name.clone(),
        Some(bootstrap_name.clone()),
        namespace.clone(),
        spec.clone(),
        labels.clone(),
    )
    .await?;
    common.create_lb(client.clone(), ports).await?;
    common.generate_configs(client.clone()).await?;

    // Definition of the deployment. Alternatively, a YAML representation could be used as well.
    let object: StatefulSet = StatefulSet {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            namespace: Some(namespace.to_owned()),
            labels: Some(labels.0.clone()),
            ..ObjectMeta::default()
        },
        spec: Some(StatefulSetSpec {
            replicas: Some(spec.replicas),
            service_name: "ipfsnode".to_owned(),
            selector: LabelSelector {
                match_expressions: None,
                match_labels: Some(labels.1.clone()),
            },
            template: PodTemplateSpec {
                spec: Some(PodSpec {
                    containers,
                    volumes: Some(volumes),
                    ..PodSpec::default()
                }),
                metadata: Some(ObjectMeta {
                    labels: Some(labels.0.clone()),
                    ..ObjectMeta::default()
                }),
            },

            volume_claim_templates: Some(pvc),
            ..StatefulSetSpec::default()
        }),
        ..StatefulSet::default()
    };

    event!(Level::INFO, name, namespace, "Creating StatefulSet");

    let statefulset_api: Api<StatefulSet> = Api::namespaced(client, namespace.as_str());
    let params = PatchParams::apply(&name);
    statefulset_api
        .patch(&name, &params, &Patch::Apply(&object))
        .await
}

#[instrument(skip(client))]
pub async fn delete(client: Client, name: String, namespace: String) -> Result<(), Error> {
    event!(Level::INFO, name, namespace, "Deleting StatefulSet");
    statefulset::delete(client.clone(), name.clone(), namespace.clone()).await?;

    configmap::delete(
        client.clone(),
        external_address_name(&name),
        namespace.clone(),
    )
    .await?;

    configmap::delete(client.clone(), format!("{name}-configs"), namespace.clone()).await?;

    configmap::delete(
        client.clone(),
        format!("{name}-identities"),
        namespace.clone(),
    )
    .await?;

    secret::delete(
        client.clone(),
        format!("ipfs-cluster-{name}-cluster-secret"),
        namespace.clone(),
    )
    .await?;

    secret::delete(
        client.clone(),
        format!("ipfs-cluster-{name}-peer-info"),
        namespace.clone(),
    )
    .await?;

    configmap::delete(
        client.clone(),
        format!("{name}-startup-scripts"),
        namespace.clone(),
    )
    .await?;

    // service::delete_cluster_ips(client.clone(), name.clone(), namespace.clone()).await?;
    load_balancer::delete(client, name, namespace).await
}

#[instrument(skip(client))]
pub async fn upsert_peer_info(
    client: Client,
    name: &str,
    namespace: &str,
    labels: (BTreeMap<String, String>, BTreeMap<String, String>),
) -> Result<(), operator_common::Error> {
    match secret::get_data(
        client.clone(),
        format!("ipfs-cluster-{name}-peer-info").as_str(),
        &namespace,
    )
    .await
    {
        Ok(i) => {
            if i.contains_key("bootstrap-peer-id") && i.contains_key("bootstrap-peer-priv-key") {
                return Ok(());
            } else {
                {}
            }
        }
        Err(_) => {}
    };

    let identity = Identity::new()?;

    let data = BTreeMap::from([
        (
            "bootstrap-peer-id".to_string(),
            ByteString(identity.peer_id.into_bytes()),
        ),
        (
            "bootstrap-peer-priv-key".to_string(),
            ByteString(identity.priv_key.into_bytes()),
        ),
    ]);

    secret::deploy(
        client,
        format!("ipfs-cluster-{name}-peer-info").as_str(),
        namespace,
        data,
        labels.0,
    )
    .await?;

    Ok(())
}

#[instrument(skip(client))]
pub async fn upsert_cluster_secret(
    client: Client,
    name: &str,
    namespace: &str,
    labels: (BTreeMap<String, String>, BTreeMap<String, String>),
) -> Result<(), operator_common::Error> {
    match secret::get_data(
        client.clone(),
        format!("ipfs-cluster-{name}-cluster-secret").as_str(),
        &namespace,
    )
    .await
    {
        Ok(i) => {
            if i.contains_key("cluster-secret") {
                return Ok(());
            } else {
                {}
            }
        }
        Err(_) => {}
    };

    let mut cluster_secret = [0u8; 32];
    thread_rng().try_fill(&mut cluster_secret[..])?;
    let encoded_secret = hex::encode(cluster_secret);

    info!("Generated cluster secret: {}", encoded_secret);

    let data = BTreeMap::from([(
        "cluster-secret".to_string(),
        ByteString(encoded_secret.into()),
    )]);

    secret::deploy(
        client,
        format!("ipfs-cluster-{name}-cluster-secret").as_str(),
        namespace,
        data,
        labels.0,
    )
    .await?;

    Ok(())
}
