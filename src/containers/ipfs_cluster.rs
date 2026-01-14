use k8s_openapi::{
    api::core::v1::{
        Container, ContainerPort, EnvVar, PersistentVolumeClaimVolumeSource, Probe,
        TCPSocketAction, Volume, VolumeMount,
    },
    apimachinery::pkg::util::intstr::IntOrString,
};

use crate::IpfsClusterSpec;

pub fn container(name: &str, spec: &IpfsClusterSpec) -> Container {
    Container {
        name: format!("{name}-ipfs-cluster"),
        image: Some(format!(
            "{}:{}",
            spec.image.repository.clone().unwrap_or_default(),
            spec.image.tag.clone().unwrap_or_default()
        )),
        image_pull_policy: Some(spec.image.pull_policy.clone().unwrap_or_default()),
        ports: Some(vec![
            ContainerPort {
                name: Some("api-http".to_owned()),
                container_port: 9094,
                ..ContainerPort::default()
            },
            ContainerPort {
                name: Some("proxy-http".to_owned()),
                container_port: 9095,
                ..ContainerPort::default()
            },
            ContainerPort {
                name: Some("cluster-swarm".to_owned()),
                container_port: 9096,
                ..ContainerPort::default()
            },
        ]),
        volume_mounts: Some(vec![VolumeMount {
            mount_path: "/data/ipfs-cluster".to_string(),
            name: "ipfs-cluster-data".to_string(),
            ..VolumeMount::default()
        }]),
        env: Some(vec![
            EnvVar {
                name: "CLUSTER_SECRET".to_owned(),
                value: Some(
                    "20C359B473A9E2A9B00C3FE23222A566CF0517ADB2C3E1796322F5B0F7E390BB".to_owned(),
                ),
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
                name: "CLUSTER_MONITORPINGINTERVAL".to_owned(),
                value: Some("2s".to_owned()),
                ..EnvVar::default()
            },
            EnvVar {
                name: "CLUSTER_CRDT_TRUSTEDPEERS".to_owned(),
                value: Some("*".to_owned()),
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

pub fn volumes() -> Vec<Volume> {
    vec![Volume {
        name: "ipfs-cluster-data".to_string(),
        persistent_volume_claim: Some(PersistentVolumeClaimVolumeSource {
            claim_name: "ipfs-cluster-data".to_string(),
            ..PersistentVolumeClaimVolumeSource::default()
        }),
        ..Volume::default()
    }]
}
