use crate::crd::NodeSpec;
use k8s_openapi::api::apps::v1::{StatefulSet, StatefulSetSpec};
use k8s_openapi::api::core::v1::{
    ConfigMapVolumeSource, Container, ContainerPort, EnvVar, PersistentVolumeClaim,
    PersistentVolumeClaimSpec, PersistentVolumeClaimVolumeSource, PodSpec, PodTemplateSpec, Volume,
    VolumeMount, VolumeResourceRequirements,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use kube::api::{DeleteParams, ObjectMeta, PostParams};
use kube::core::ErrorResponse;
use kube::{Api, Client, Error};
use operator_common::types::{
    configmap,
    load_balancer::{self, get_external_ips},
};
use operator_common::ActionType;
use std::collections::BTreeMap;
use std::string::ToString;
use tracing::{event, instrument, Level};

const DATA_DIR: &str = "/data";

#[instrument(skip(client))]
pub async fn deploy(
    client: Client,
    name: String,
    namespace: String,
    spec: NodeSpec,
    action: ActionType,
    labels: (BTreeMap<String, String>, BTreeMap<String, String>),
) -> Result<StatefulSet, Error> {
    match load_balancer::create(
        client.clone(),
        name.clone(),
        namespace.clone(),
        spec.kind.to_string(),
        spec.replicas,
        spec.p2p_port.unwrap_or(4001),
        action,
    )
    .await
    {
        Ok(_) => {}
        Err(err) => {
            return Err(Error::Api(ErrorResponse {
                status: "Failed".to_string(),
                message: err.to_string(),
                reason: "Failed to create load balancers".to_string(),
                code: 418,
            }))
        }
    }

    let external_addrs = match get_external_ips(
        client.clone(),
        name.clone(),
        namespace.clone(),
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
            }))
        }
    };

    configmap::deploy(
        client.clone(),
        format!("{name}-external-addresses").as_str(),
        namespace.clone(),
        external_addrs.clone(),
        labels.0.clone(),
    )
    .await?;

    //
    // DONE TO HERE
    //
    let mut requests: BTreeMap<String, Quantity> = BTreeMap::new();
    requests.insert(
        "storage".to_owned(),
        Quantity(spec.persistence.size.clone().unwrap_or_default()),
    );

    let mounts = Some(vec![
        VolumeMount {
            mount_path: DATA_DIR.to_string(),
            name: "node-data".to_string(),
            ..VolumeMount::default()
        },
        VolumeMount {
            mount_path: "/var/lib/ipfs/external-addresses".to_string(),
            name: "external-addresses".to_string(),
            ..VolumeMount::default()
        },
    ]);
    let volumes = Some(vec![
        Volume {
            name: "node-data".to_string(),
            persistent_volume_claim: Some(PersistentVolumeClaimVolumeSource {
                claim_name: "node-data".to_string(),
                ..PersistentVolumeClaimVolumeSource::default()
            }),
            ..Volume::default()
        },
        Volume {
            name: "external-addresses".to_string(),
            config_map: Some(ConfigMapVolumeSource {
                name: Some(format!("{name}-ipfs-external-addresses")),
                ..ConfigMapVolumeSource::default()
            }),
            ..Volume::default()
        },
    ]);

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
                    init_containers: Some(vec![
                            Container {
                                name: "generate-node-key".to_owned(),
                                image: Some(format!("{}:{}", spec.image.repository.clone().unwrap_or_default(), spec.image.tag.clone().unwrap_or_default())),
                                image_pull_policy: Some(spec.image.pull_policy.clone().unwrap_or_default()),
                                env: Some(vec![EnvVar {
                                    name: "RUST_LOG".to_owned(),
                                    value: Some(spec.rust_log.clone()),
                                    ..EnvVar::default()
                                }]),
                                volume_mounts: mounts.clone(),
                                command: Some(vec![
                                    "sh".to_owned(),
                                    "-c".to_owned(),
                                    format!("if [ ! -e {node_key_file} ]; then /gevulot generate key --key-file {node_key_file}; fi", node_key_file = format!("{DATA_DIR}/node.key")),
                                ]),
                                ..Container::default()
                            },
                            Container {
                                name: "database-migration".to_owned(),
                                image: Some(format!("{}:{}", spec.image.repository.clone().unwrap_or_default(), spec.image.tag.clone().unwrap_or_default())),
                                image_pull_policy: Some(spec.image.pull_policy.clone().unwrap_or_default()),
                                env: Some(vec![
                                    EnvVar {
                                        name: "RUST_LOG".to_owned(),
                                        value: Some(spec.rust_log.clone()),
                                        ..EnvVar::default()
                                    },
                                ]),
                                volume_mounts: mounts.clone(),
                                command: Some(vec![
                                    "/gevulot".to_owned(),
                                    "migrate".to_owned(),
                                ]),
                                ..Container::default()
                            },
                        ]),
                    containers: vec![Container {
                        name: name.to_owned(),
                        image: Some(format!(
                            "{}:{}",
                            spec.image.repository.clone().unwrap_or_default(),
                            spec.image.tag.clone().unwrap_or_default()
                        )),
                        image_pull_policy: Some(spec.image.pull_policy.clone().unwrap_or_default()),
                        ports: Some(vec![
                            ContainerPort {
                                name: Some("http".to_owned()),
                                container_port: 9944,
                                ..ContainerPort::default()
                            },
                            ContainerPort {
                                name: Some("p2p".to_owned()),
                                container_port: 9999,
                                ..ContainerPort::default()
                            },
                        ]),
                        command: Some(vec![
                                "sh".to_owned(),
                                "-c".to_owned(),
                                "/gevulot run --p2p-advertised-listen-addr $(cat /var/lib/ipfs/external-addresses/${HOSTNAME}):9999".to_owned(),
                            ]),
                        volume_mounts: mounts.clone(),
                        ..Container::default()
                    }],
                    volumes,
                    ..PodSpec::default()
                }),
                metadata: Some(ObjectMeta {
                    labels: Some(labels.0.clone()),
                    ..ObjectMeta::default()
                }),
            },

            // Only archive and json rpc nodes get PVCs
            volume_claim_templates: Some(vec![PersistentVolumeClaim {
                    metadata: ObjectMeta {
                        name: Some("node-data".to_string()),
                        labels: Some(labels.0.clone()),
                        ..ObjectMeta::default()
                    },
                    spec: Some(PersistentVolumeClaimSpec {
                        access_modes: Some(vec!["ReadWriteOnce".to_string()]),
                        resources: Some(VolumeResourceRequirements {
                            requests: Some(requests),
                            ..VolumeResourceRequirements::default()
                        }),
                        ..PersistentVolumeClaimSpec::default()
                    }),
                    ..PersistentVolumeClaim::default()
                }]),
            ..StatefulSetSpec::default()
        }),
        ..StatefulSet::default()
    };

    event!(Level::INFO, name, namespace, "Creating StatefulSet");

    // Create the deployment defined above
    let statefulset_api: Api<StatefulSet> = Api::namespaced(client, namespace.as_str());
    statefulset_api
        .create(&PostParams::default(), &object)
        .await
}

#[instrument(skip(client))]
pub async fn delete(client: Client, name: String, namespace: String) -> Result<(), Error> {
    event!(Level::INFO, name, namespace, "Deleting StatefulSet");

    let api: Api<StatefulSet> = Api::namespaced(client, namespace.as_str());
    match api.delete(name.as_str(), &DeleteParams::default()).await {
        Ok(_) => Ok(()),
        Err(e) => {
            match e {
                // If the resource doesn't exist, we can ignore the error
                Error::Api(er) => {
                    if er.reason == "NotFound" {
                        return Ok(());
                    };
                    Err(Error::Api(er))
                }
                _ => Err(e),
            }
        }
    }
}

// #[instrument]
// fn set_env_vars(
//     spec: NodeSpec,
//     announce_addr: AnnounceAddr,
//     discovery_addrs: Option<String>,
// ) -> Vec<EnvVar> {
//     let mut variables: Vec<EnvVar> = Vec::new();

//     // All modes get these
//     variables.push(EnvVar {
//         name: "RUST_LOG".to_owned(),
//         value: Some(spec.rust_log.to_owned()),
//         ..EnvVar::default()
//     });

//     variables.push(EnvVar {
//         name: "GEVULOT_DATA_DIRECTORY".to_owned(),
//         value: Some(DATA_DIR.to_string()),
//         ..EnvVar::default()
//     });

//     variables.push(EnvVar {
//         name: "GEVULOT_HTTP_PORT".to_owned(),
//         value: Some("9995".to_string()),
//         ..EnvVar::default()
//     });

//     variables.push(EnvVar {
//         name: "GEVULOT_P2P_LISTEN_ADDR".to_owned(),
//         value: Some("0.0.0.0:9999".to_string()),
//         ..EnvVar::default()
//     });

//     variables.push(EnvVar {
//         name: "GEVULOT_HEALTHCHECK_LISTEN_ADDR".to_owned(),
//         value: Some("0.0.0.0:8888".to_string()),
//         ..EnvVar::default()
//     });

//     // Archive nodes need to set these differently. The only way to reasonably do it is to use CLI
//     // flags so this env variable is un-set for archive nodes.
//     if let AnnounceAddr::StandardCluster(addr) = announce_addr {
//         variables.push(EnvVar {
//             name: "GEVULOT_P2P_ADVERTISED_LISTEN_ADDR".to_owned(),
//             value: Some(format!("{}:9999", addr)),
//             ..EnvVar::default()
//         });
//     };

//     event!(Level::INFO, "Setting environment variables");

//     variables
// }
