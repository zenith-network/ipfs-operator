use std::collections::BTreeMap;

use k8s_openapi::{api::core::v1::ContainerPort, apimachinery::pkg::util::intstr::IntOrString};
use kube::{Client, Error, core::ErrorResponse};
use operator_common::{
    external_address_name,
    types::{
        configmap, load_balancer,
        service::{self, Port},
    },
};
use tracing::{info, instrument};

use crate::{
    NodeSpec,
    identity::Identities,
    ipfs::{copy_default_startup_scripts, generate_config, get_bootstrap_list},
};

#[derive(Debug, Clone)]
pub struct Common {
    pub name: String,
    pub bootstrap_name: Option<String>,
    pub namespace: String,
    pub spec: NodeSpec,
    pub identities: Identities,
    pub bootstrap_list: Vec<String>,
    pub labels: (BTreeMap<String, String>, BTreeMap<String, String>),
    pub external_addrs: BTreeMap<String, String>,
}

impl Common {
    pub async fn new(
        client: Client,
        name: String,
        bootstrap_name: Option<String>,
        namespace: String,
        spec: NodeSpec,
        labels: (BTreeMap<String, String>, BTreeMap<String, String>),
    ) -> Result<Self, Error> {
        match copy_default_startup_scripts(client.clone(), &name, &namespace, labels.0.clone())
            .await
        {
            Ok(_) => (),
            Err(err) => {
                return Err(Error::Api(ErrorResponse {
                    status: "Failed".to_string(),
                    message: err.to_string(),
                    reason: "Failed to copy default startup scripts".to_string(),
                    code: 418,
                }));
            }
        }

        let identities = match Identities::new(
            client.clone(),
            &name,
            &namespace,
            spec.replicas,
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

        Ok(Self {
            name,
            bootstrap_name,
            namespace,
            spec,
            identities,
            labels,
            bootstrap_list: vec![],
            external_addrs: BTreeMap::default(),
        })
    }

    #[instrument(skip(client))]
    pub async fn create_lb(&self, client: Client, ports: Vec<Port>) -> Result<(), Error> {
        let mut service_selector_labels = self.labels.1.clone();
        service_selector_labels.append(&mut BTreeMap::from([(
            "statefulset.kubernetes.io/pod-name".to_string(),
            format!("{}-0", self.name),
        )]));

        match load_balancer::deploy(
            client.clone(),
            self.name.clone(),
            self.namespace.clone(),
            self.spec.kind.clone().to_string(),
            self.spec.replicas,
            ports.clone(),
            (self.labels.0.clone(), service_selector_labels.clone()),
        )
        .await
        {
            Ok(_) => {}
            Err(err) => {
                return Err(Error::Api(ErrorResponse {
                    status: "Failed".to_string(),
                    message: err.to_string(),
                    reason: "Failed to create load balancers for service port".to_string(),
                    code: 418,
                }));
            }
        };

        Ok(())
    }

    #[instrument(skip(client))]
    pub async fn create_external_ip_cm(&mut self, client: Client) -> Result<(), Error> {
        self.external_addrs = match load_balancer::get_external_ips(
            client.clone(),
            self.name.to_string(),
            self.namespace.to_string(),
            service::Port {
                name: "p2p".to_string(),
                port: self.spec.p2p_port.unwrap_or(4001),
                protocol: "TCP".to_string(),
                ..Default::default()
            },
            self.spec.replicas,
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
            external_address_name(&self.name).as_str(),
            &self.namespace,
            self.external_addrs.clone(),
            self.labels.0.clone(),
        )
        .await?;

        Ok(())
    }

    #[instrument(skip(client))]
    pub async fn generate_bootstrap_list(&mut self, client: Client) -> Result<(), Error> {
        if self.external_addrs.is_empty() {
            self.create_external_ip_cm(client.clone()).await?;
        }
        let (identities, external_addrs) = if let Some(bootstrap_name) = self.bootstrap_name.clone()
        {
            (
                match Identities::get(
                    client.clone(),
                    &format!("{bootstrap_name}-identities"),
                    &self.namespace,
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
                },
                match configmap::get_data(
                    client.clone(),
                    format!("{bootstrap_name}-external-addresses").as_str(),
                    &self.namespace,
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
                },
            )
        } else {
            (self.identities.clone(), self.external_addrs.clone())
        };

        self.bootstrap_list = match get_bootstrap_list(identities, external_addrs).await {
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
            self.bootstrap_list
        );

        Ok(())
    }

    #[instrument(skip(client))]
    pub async fn generate_configs(&mut self, client: Client) -> Result<(), Error> {
        if self.bootstrap_list.len() == 0 {
            self.generate_bootstrap_list(client.clone()).await?;
        }

        let configs = match generate_config(
            client.clone(),
            &self.name,
            self.identities.clone(),
            self.external_addrs.clone(),
            self.bootstrap_list.clone(),
            self.labels.0.clone(),
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
            &format!("{}-configs", self.name),
            &self.namespace,
            configs.clone(),
            self.labels.0.clone(),
        )
        .await?;

        Ok(())
    }
}

pub fn ipfs_ports() -> Vec<Port> {
    vec![
        Port {
            name: "swarm".to_string(),
            port: 4001,
            target_port: IntOrString::String("swarm".to_string()),
            protocol: "TCP".to_string(),
        },
        Port {
            name: "api".to_string(),
            port: 5001,
            target_port: IntOrString::String("api".to_string()),
            protocol: "TCP".to_string(),
        },
        Port {
            name: "ws".to_string(),
            port: 8081,
            target_port: IntOrString::String("ws".to_string()),
            protocol: "TCP".to_string(),
        },
        Port {
            name: "http".to_string(),
            port: 8080,
            target_port: IntOrString::String("http".to_string()),
            protocol: "TCP".to_string(),
        },
    ]
}

pub fn ipfs_cluster_ports() -> Vec<Port> {
    vec![
        Port {
            name: "api-http".to_string(),
            port: 9094,
            target_port: IntOrString::String("api-http".to_string()),
            protocol: "TCP".to_string(),
        },
        Port {
            name: "proxy-http".to_string(),
            port: 9095,
            target_port: IntOrString::String("proxy-http".to_string()),
            protocol: "TCP".to_string(),
        },
        Port {
            name: "cluster-swarm".to_string(),
            port: 9096,
            target_port: IntOrString::String("cluster-swarm".to_string()),
            protocol: "TCP".to_string(),
        },
    ]
}

pub fn generate_container_ports(ports: Vec<Port>) -> Vec<ContainerPort> {
    ports
        .into_iter()
        .map(|port| ContainerPort {
            name: Some(port.name),
            container_port: port.port,
            ..Default::default()
        })
        .collect()
}
