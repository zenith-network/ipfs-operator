use crate::crd::NodeSpec;
use crate::identity::Identities;
use crate::ipfs::{generate_config, get_bootstrap_list};
use k8s_openapi::api::apps::v1::{StatefulSet, StatefulSetSpec};
use k8s_openapi::api::core::v1::{
    ConfigMapVolumeSource, Container, ContainerPort, EnvVar, PersistentVolumeClaim,
    PersistentVolumeClaimSpec, PersistentVolumeClaimVolumeSource, PodSpec, PodTemplateSpec, Volume,
    VolumeMount, VolumeResourceRequirements,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use kube::api::{ObjectMeta, PostParams};
use kube::core::ErrorResponse;
use kube::{Api, Client, Error};
use operator_common::types::statefulset;
use operator_common::{
    ActionType, external_address_name,
    types::{
        configmap,
        load_balancer::{self, get_external_ips},
    },
};
use std::collections::BTreeMap;
use std::string::ToString;
use tracing::{Level, event, info, instrument};

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
            }));
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

    let bootstrap_list =
        match get_bootstrap_list(&name, identities.clone(), external_addrs.clone()).await {
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
            mount_path: "/var/lib/ipfs/configs".to_string(),
            name: "ipfs-configs".to_string(),
            ..VolumeMount::default()
        },
        VolumeMount {
            mount_path: "/data/ipfs/swarm.key".to_string(),
            name: "swarm-key".to_string(),
            sub_path: Some("swarm.key".to_string()),
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
            name: "ipfs-configs".to_string(),
            config_map: Some(ConfigMapVolumeSource {
                name: Some(format!("{name}-configs")),
                ..ConfigMapVolumeSource::default()
            }),
            ..Volume::default()
        },
        Volume {
            name: "swarm-key".to_string(),
            config_map: Some(ConfigMapVolumeSource {
                name: Some(format!("{name}-swarm-key")),
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
                    containers: vec![Container {
                        name: name.to_owned(),
                        image: Some(format!(
                            "{}:{}",
                            spec.image.repository.clone().unwrap_or_default(),
                            spec.image.tag.clone().unwrap_or_default()
                        )),
                        image_pull_policy: Some(spec.image.pull_policy.clone().unwrap_or_default()),
                        ports: Some(vec![ContainerPort {
                            name: Some("p2p".to_owned()),
                            container_port: 4001,
                            ..ContainerPort::default()
                        }]),
                        command: Some(vec![
                            "sh".to_owned(),
                            "-c".to_owned(),
                            "/sbin/tini -- /usr/local/bin/start_ipfs --config-file /var/lib/ipfs/configs/${HOSTNAME} daemon".to_owned(),
                        ]),
                        volume_mounts: mounts.clone(),
                        env: Some(vec![EnvVar {
                            name: "IPFS_PROFILE".to_owned(),
                            value: Some("server".to_owned()),
                            ..EnvVar::default()
                        }]),
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

    let statefulset_api: Api<StatefulSet> = Api::namespaced(client, namespace.as_str());
    statefulset_api
        .create(&PostParams::default(), &object)
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
