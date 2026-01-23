use crate::crd::NodeSpec;
use k8s_openapi::api::networking::v1::{
    HTTPIngressPath, HTTPIngressRuleValue, Ingress, IngressBackend, IngressRule,
    IngressServiceBackend, IngressSpec, ServiceBackendPort,
};
use kube::api::{DeleteParams, ObjectMeta, PostParams};
use kube::{Api, Client, Error};
use std::collections::BTreeMap;
use tracing::{event, instrument, Level};

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
    labels: BTreeMap<String, String>,
) -> Result<Ingress, Error> {
    // Definition of the deployment. Alternatively, a YAML representation could be used as well.
    let object: Ingress = Ingress {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            namespace: Some(namespace.to_owned()),
            labels: Some(labels.clone()),
            ..ObjectMeta::default()
        },
        spec: Some(IngressSpec {
            rules: Some(vec![IngressRule {
                host: Some(spec.ingress.host.to_owned()),
                http: Some(HTTPIngressRuleValue {
                    paths: vec![HTTPIngressPath {
                        path: Some(spec.ingress.path.to_string()),
                        path_type: "ImplementationSpecific".to_string(),
                        backend: IngressBackend {
                            service: Some(IngressServiceBackend {
                                name: name.to_string(),
                                port: Some(ServiceBackendPort {
                                    number: Some(spec.ingress.service_port),
                                    ..ServiceBackendPort::default()
                                }),
                            }),
                            ..IngressBackend::default()
                        },
                    }],
                }),
            }]),
            ..IngressSpec::default()
        }),
        ..Ingress::default()
    };

    event!(Level::INFO, name, namespace, "Creating Ingress");

    // Create the pvc defined above
    let ingress_api: Api<Ingress> = Api::namespaced(client, namespace.as_str());
    ingress_api.create(&PostParams::default(), &object).await
}

/// Deletes an existing deployment.
///
/// # Arguments:
/// - `client` - A Kubernetes client to delete the Deployment with
/// - `name` - Name of the deployment to delete
/// - `namespace` - Namespace the existing deployment resides in
///
/// Note: It is assumed the deployment exists for simplicity. Otherwise returns an Error.
#[instrument(skip(client))]
pub async fn delete(client: Client, name: String, namespace: String) -> Result<(), Error> {
    event!(Level::INFO, name, namespace, "Deleting Ingress");

    let api: Api<Ingress> = Api::namespaced(client, namespace.as_str());
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
