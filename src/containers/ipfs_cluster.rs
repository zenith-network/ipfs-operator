use k8s_openapi::{
    api::core::v1::{
        ConfigMapVolumeSource, Container, EnvVar, EnvVarSource, PersistentVolumeClaimVolumeSource,
        Probe, SecretKeySelector, TCPSocketAction, Volume, VolumeMount,
    },
    apimachinery::pkg::util::intstr::IntOrString,
};

use crate::{
    IpfsClusterSpec,
    types::common::{generate_container_ports, ipfs_cluster_ports},
};

pub fn container(name: &str, spec: &IpfsClusterSpec) -> Container {
    Container {
        name: format!("{name}-ipfs-cluster"),
        image: Some(format!(
            "{}:{}",
            spec.image.repository.clone().unwrap_or_default(),
            spec.image.tag.clone().unwrap_or_default()
        )),
        image_pull_policy: Some(spec.image.pull_policy.clone().unwrap_or_default()),
        ports: Some(generate_container_ports(ipfs_cluster_ports())),
        command: Some(vec!["sh".to_owned(), "/custom/entrypoint.sh".to_owned()]),
        volume_mounts: Some(vec![
            VolumeMount {
                mount_path: "/data/ipfs-cluster".to_string(),
                name: "ipfs-cluster-data".to_string(),
                ..VolumeMount::default()
            },
            VolumeMount {
                mount_path: "/custom".to_string(),
                name: "ipfs-cluster-startup-scripts".to_string(),
                ..VolumeMount::default()
            },
        ]),
        env: Some(vec![
            EnvVar {
                name: "CLUSTER_SECRET".to_owned(),
                value_from: Some(EnvVarSource {
                    secret_key_ref: Some(SecretKeySelector {
                        name: Some(format!("ipfs-cluster-{name}-cluster-secret")),
                        key: "cluster-secret".to_string(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..EnvVar::default()
            },
            EnvVar {
                name: "BOOTSTRAP_PEER_ID".to_owned(),
                value_from: Some(EnvVarSource {
                    secret_key_ref: Some(SecretKeySelector {
                        name: Some(format!("ipfs-cluster-{name}-peer-info")),
                        key: "bootstrap-peer-id".to_string(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
            EnvVar {
                name: "BOOTSTRAP_PEER_PRIV_KEY".to_owned(),
                value_from: Some(EnvVarSource {
                    secret_key_ref: Some(SecretKeySelector {
                        name: Some(format!("ipfs-cluster-{name}-peer-info")),
                        key: "bootstrap-peer-priv-key".to_string(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..EnvVar::default()
            },
            EnvVar {
                name: "CLUSTER_IPFSHTTP_NODEMULTIADDRESS".to_owned(),
                value: Some("/dns4/localhost/tcp/5001".to_owned()),
                ..EnvVar::default()
            },
            EnvVar {
                name: "CLUSTER_RESTAPI_HTTPLISTENMULTIADDRESS".to_owned(),
                value: Some("/ip4/0.0.0.0/tcp/9094".to_owned()),
                ..EnvVar::default()
            },
            EnvVar {
                name: "CLUSTER_MONITOR_PING_INTERVAL".to_owned(),
                value: Some("3m".to_owned()),
                ..EnvVar::default()
            },
            EnvVar {
                name: "CLUSTER_CRDT_TRUSTEDPEERS".to_owned(),
                value: Some("*".to_owned()),
                ..EnvVar::default()
            },
            EnvVar {
                name: "SVC_NAME".to_owned(),
                value: Some(format!("{name}")),
                ..EnvVar::default()
            },
        ]),
        liveness_probe: Some(Probe {
            tcp_socket: Some(TCPSocketAction {
                port: IntOrString::String("cluster-swarm".to_string()),
                ..TCPSocketAction::default()
            }),
            initial_delay_seconds: Some(5),
            timeout_seconds: Some(5),
            period_seconds: Some(10),
            ..Probe::default()
        }),
        ..Container::default()
    }
}

pub fn volumes(name: &str) -> Vec<Volume> {
    vec![
        Volume {
            name: "ipfs-cluster-data".to_string(),
            persistent_volume_claim: Some(PersistentVolumeClaimVolumeSource {
                claim_name: "ipfs-cluster-data".to_string(),
                ..PersistentVolumeClaimVolumeSource::default()
            }),
            ..Volume::default()
        },
        Volume {
            name: "ipfs-cluster-startup-scripts".to_string(),
            config_map: Some(ConfigMapVolumeSource {
                name: Some(format!("{name}-startup-scripts")),
                default_mode: Some(0o755),
                ..ConfigMapVolumeSource::default()
            }),
            ..Volume::default()
        },
    ]
}
