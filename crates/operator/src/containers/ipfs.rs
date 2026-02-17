use k8s_openapi::{
    api::core::v1::{
        ConfigMapVolumeSource, Container, EnvVar, ExecAction, PersistentVolumeClaimVolumeSource,
        Probe, TCPSocketAction, Volume, VolumeMount,
    },
    apimachinery::pkg::util::intstr::IntOrString,
};

use crate::{
    NodeSpec,
    types::common::{generate_container_ports, ipfs_ports},
};

pub fn container(name: &str, spec: &NodeSpec) -> Container {
    Container {
        name: name.to_owned(),
        image: Some(format!(
            "{}:{}",
            spec.ipfs.image.repository.clone().unwrap_or_default(),
            spec.ipfs.image.tag.clone().unwrap_or_default()
        )),
        image_pull_policy: Some(spec.ipfs.image.pull_policy.clone().unwrap_or_default()),
        ports: Some(generate_container_ports(ipfs_ports())),
        command: Some(vec![
            "sh".to_owned(),
            "-c".to_owned(),
            "/sbin/tini -- /usr/local/bin/start_ipfs daemon".to_owned(),
        ]),
        volume_mounts: Some(vec![
            VolumeMount {
                mount_path: "/data".to_string(),
                name: "ipfs-data".to_string(),
                ..VolumeMount::default()
            },
            VolumeMount {
                mount_path: "/var/lib/ipfs/configs".to_string(),
                name: "ipfs-configs".to_string(),
                ..VolumeMount::default()
            },
            VolumeMount {
                mount_path: "/usr/local/bin/start_ipfs".to_string(),
                name: "ipfs-startup-scripts".to_string(),
                sub_path: Some("start_ipfs".to_string()),
                ..VolumeMount::default()
            },
            VolumeMount {
                mount_path: "/data/ipfs/swarm.key".to_string(),
                name: "swarm-key".to_string(),
                sub_path: Some("swarm.key".to_string()),
                ..VolumeMount::default()
            },
        ]),
        env: Some(vec![EnvVar {
            name: "IPFS_PROFILE".to_owned(),
            value: Some("server".to_owned()),
            ..EnvVar::default()
        }]),
        // This is dumb, but ok
        liveness_probe: Some(Probe {
            tcp_socket: Some(TCPSocketAction {
                port: IntOrString::String("swarm".to_string()),
                ..TCPSocketAction::default()
            }),
            initial_delay_seconds: Some(30),
            timeout_seconds: Some(5),
            period_seconds: Some(15),
            ..Probe::default()
        }),
        // Check if the empty directory is available
        readiness_probe: Some(Probe {
            exec: Some(ExecAction {
                command: Some(vec![
                    "ipfs".to_owned(),
                    "ls".to_owned(),
                    "QmUNLLsPACCz1vLxQVkXqqLX5R1X345qqfHbsf67hvA3Nn".to_owned(),
                ]),
            }),
            ..Probe::default()
        }),
        ..Container::default()
    }
}

pub fn volumes(name: &str, bootstrap_name: &str) -> Vec<Volume> {
    vec![
        Volume {
            name: "ipfs-data".to_string(),
            persistent_volume_claim: Some(PersistentVolumeClaimVolumeSource {
                claim_name: "ipfs-data".to_string(),
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
            name: "ipfs-startup-scripts".to_string(),
            config_map: Some(ConfigMapVolumeSource {
                name: Some(format!("{name}-startup-scripts")),
                default_mode: Some(0o755),
                ..ConfigMapVolumeSource::default()
            }),
            ..Volume::default()
        },
        Volume {
            name: "swarm-key".to_string(),
            config_map: Some(ConfigMapVolumeSource {
                name: Some(format!("{bootstrap_name}-swarm-key")),
                ..ConfigMapVolumeSource::default()
            }),
            ..Volume::default()
        },
    ]
}
