use crate::containers;
use crate::crd::NodeSpec;
use crate::identity::Identities;
use crate::ipfs::{generate_config, get_bootstrap_list};
use k8s_openapi::api::apps::v1::{StatefulSet, StatefulSetSpec};
use k8s_openapi::api::core::v1::{PodSpec, PodTemplateSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use kube::api::{ObjectMeta, Patch, PatchParams};
use kube::core::ErrorResponse;
use kube::{Api, Client, Error};
use operator_common::types::{configmap, load_balancer, service, statefulset};
use operator_common::{ActionType, external_address_name};
use std::collections::BTreeMap;
use std::string::ToString;
use tracing::{Level, event, info, instrument};

/// Creates a new deployment of `n` pods with the `inanimate/echo-server:latest` docker image inside,
/// where `n` is the number of `replicas` given.
/// Note: It is assumed the resource does not already exists for simplicity. Returns an `Error` if it does.
/// # Arguments
/// - `client` - A Kubernetes client to create the deployment with.
/// - `name` - Name of the deployment to be created
/// - `replicas` - Number of pod replicas for the Deployment to contain
/// - `namespace` - Namespace to create the Kubernetes Deployment in.
#[instrument(skip(client))]
pub async fn deploy(
    client: Client,
    name: String,
    namespace: String,
    spec: NodeSpec,
    action: ActionType,
    labels: (BTreeMap<String, String>, BTreeMap<String, String>),
) -> Result<StatefulSet, Error> {
    // Create p2p port
    match load_balancer::create(
        client.clone(),
        name.clone(),
        namespace.clone(),
        spec.kind.to_string(),
        spec.replicas,
        service::Port {
            name: "p2p".to_string(),
            port: spec.p2p_port.unwrap_or(4001),
            protocol: "TCP".to_string(),
        },
        action,
    )
    .await
    {
        Ok(_) => {}
        Err(err) => {
            return Err(Error::Api(ErrorResponse {
                status: "Failed".to_string(),
                message: err.to_string(),
                reason: "Failed to create load balancers for p2p port".to_string(),
                code: 418,
            }));
        }
    }

    let external_addrs = match load_balancer::get_external_ips(
        client.clone(),
        name.clone(),
        namespace.clone(),
        service::Port {
            name: "p2p".to_string(),
            port: spec.p2p_port.unwrap_or(4001),
            protocol: "TCP".to_string(),
        },
        spec.replicas,
    )
    .await
    {
        Ok(ips) => ips,
        Err(err) => {
            return Err(Error::Api(ErrorResponse {
                status: "Failed".to_string(),
                message: err.to_string(),
                reason: "Failed to create load balancers".to_string(),
                code: 418,
            }));
        }
    };

    configmap::deploy(
        client.clone(),
        external_address_name(&name).as_str(),
        &namespace,
        external_addrs.clone(),
        labels.0.clone(),
    )
    .await?;

    let identities = match Identities::new(
        client.clone(),
        &name,
        &namespace,
        spec.replicas,
        action,
        labels.0.clone(),
    )
    .await
    {
        Ok(i) => i,
        Err(err) => {
            return Err(Error::Api(ErrorResponse {
                status: "Failed".to_string(),
                message: err.to_string(),
                reason: "Failed to get identities".to_string(),
                code: 418,
            }));
        }
    };

    let bootstrap_name = spec
        .bootstrap_name
        .clone()
        .unwrap_or("bootstrap".to_string());

    let bootstrap_identities =
        match Identities::get(client.clone(), &bootstrap_name, &namespace).await {
            Ok(i) => i,
            Err(err) => {
                return Err(Error::Api(ErrorResponse {
                    status: "Failed".to_string(),
                    message: err.to_string(),
                    reason: "Failed to get identities".to_string(),
                    code: 418,
                }));
            }
        };

    let bootstrap_external_addrs = match load_balancer::get_external_ips(
        client.clone(),
        bootstrap_name.clone(),
        namespace.clone(),
        service::Port {
            name: "p2p".to_string(),
            port: spec.p2p_port.unwrap_or(4001),
            protocol: "TCP".to_string(),
        },
        spec.replicas,
    )
    .await
    {
        Ok(ips) => ips,
        Err(err) => {
            return Err(Error::Api(ErrorResponse {
                status: "Failed".to_string(),
                message: err.to_string(),
                reason: "Failed to create load balancers".to_string(),
                code: 418,
            }));
        }
    };

    let bootstrap_list = match get_bootstrap_list(
        &name,
        bootstrap_identities.clone(),
        bootstrap_external_addrs.clone(),
    )
    .await
    {
        Ok(i) => i,
        Err(err) => {
            return Err(Error::Api(ErrorResponse {
                status: "Failed".to_string(),
                message: err.to_string(),
                reason: "Failed to get bootstrap list".to_string(),
                code: 418,
            }));
        }
    };

    info!(
        "Bootstrap list generated successfully: {:?}",
        bootstrap_list
    );

    let configs = match generate_config(
        client.clone(),
        &name,
        identities,
        external_addrs,
        bootstrap_list,
        labels.0.clone(),
    )
    .await
    {
        Ok(config) => config,
        Err(err) => {
            return Err(Error::Api(ErrorResponse {
                status: "Failed".to_string(),
                message: err.to_string(),
                reason: "Failed to generate config".to_string(),
                code: 418,
            }));
        }
    };

    configmap::deploy(
        client.clone(),
        &format!("{name}-configs"),
        &namespace,
        configs.clone(),
        labels.0.clone(),
    )
    .await?;

    let mut volumes = containers::ipfs::volumes(&name, &bootstrap_name);
    let mut containers = vec![containers::ipfs::container(&name, &spec)];

    let mut pvc = vec![containers::pvc(
        "ipfs-data",
        spec.ipfs.persistence.clone(),
        labels.clone(),
    )];

    if let Some(ipfs_cluster) = spec.ipfs_cluster {
        pvc.push(containers::pvc(
            "ipfs-cluster-data",
            ipfs_cluster.persistence.clone(),
            labels.clone(),
        ));

        volumes.extend(containers::ipfs_cluster::volumes().iter().cloned());
        containers.push(containers::ipfs_cluster::container(&name, &ipfs_cluster));
    }

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

    load_balancer::delete(client, name, namespace).await
}
