use k8s_openapi::api::core::v1::{Service, ServicePort, ServiceSpec};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::api::{DeleteParams, ListParams, ObjectMeta, Patch, PatchParams};
use kube::{Api, Client, Error, ResourceExt};
use std::collections::BTreeMap;
use std::fmt::Display;
use tracing::{Level, event, instrument};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ServiceType {
    ClusterIP,
    NodePort,
    LoadBalancer,
}

impl Display for ServiceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceType::ClusterIP => write!(f, "ClusterIP"),
            ServiceType::NodePort => write!(f, "NodePort"),
            ServiceType::LoadBalancer => write!(f, "LoadBalancer"),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Port {
    pub name: String,
    pub port: i32,
    pub target_port: IntOrString,
    pub protocol: String,
}

#[instrument(skip(client))]
pub async fn deploy(
    client: Client,
    name: String,
    namespace: String,
    service_type: ServiceType,
    service_port: Vec<Port>,
    labels: (BTreeMap<String, String>, BTreeMap<String, String>),
) -> Result<Service, Error> {
    let mut service_ports: Vec<ServicePort> = Vec::new();

    for port in service_port {
        service_ports.push(ServicePort {
            name: Some(port.name),
            port: port.port,
            protocol: Some(port.protocol.to_string()),
            target_port: Some(port.target_port),
            ..ServicePort::default()
        });
    }

    let object: Service = Service {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            namespace: Some(namespace.to_owned()),
            labels: Some(labels.0.clone()),
            ..ObjectMeta::default()
        },
        spec: Some(ServiceSpec {
            type_: Some(service_type.to_string()),
            ports: Some(service_ports),
            selector: Some(labels.1),
            ..ServiceSpec::default()
        }),
        ..Service::default()
    };

    event!(Level::INFO, name, namespace, "Creating Service");

    let service_api: Api<Service> = Api::namespaced(client, namespace.as_str());
    let params = PatchParams::apply(&name);
    service_api
        .patch(&name, &params, &Patch::Apply(&object))
        .await
}

#[instrument(skip(client))]
pub async fn delete(client: Client, name: String, namespace: String) -> Result<(), Error> {
    event!(Level::INFO, name, namespace, "Deleting Service");

    let api: Api<Service> = Api::namespaced(client, namespace.as_str());
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

#[instrument(skip(client))]
pub async fn delete_cluster_ips(
    client: Client,
    name: String,
    namespace: String,
) -> Result<(), Error> {
    let service_api: Api<Service> = Api::namespaced(client.clone(), namespace.as_str());
    let lp = ListParams::default()
        .match_any()
        .timeout(300)
        .labels(format!("app.kubernetes.io/instance={name}").as_str())
        .fields("spec.type=ClusterIP");
    let existing_services = service_api.list(&lp).await?;

    for svc in existing_services {
        delete(client.clone(), svc.name_any(), namespace.clone()).await?;
    }

    Ok(())
}
