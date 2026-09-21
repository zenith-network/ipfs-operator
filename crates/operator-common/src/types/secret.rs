use k8s_openapi::ByteString;
use k8s_openapi::api::core::v1::Secret;
use kube::api::{DeleteParams, ObjectMeta, Patch, PatchParams};
use kube::{Api, Client, Error};
use std::collections::BTreeMap;
use tracing::{Level, event, instrument};

#[instrument(skip(client))]
pub async fn deploy(
    client: Client,
    name: &str,
    namespace: &str,
    data: BTreeMap<String, ByteString>,
    labels: BTreeMap<String, String>,
) -> Result<Secret, Error> {
    // Definition of the deployment. Alternatively, a YAML representation could be used as well.
    let object: Secret = Secret {
        data: Some(data),
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            namespace: Some(namespace.to_owned()),
            labels: Some(labels.clone()),
            ..ObjectMeta::default()
        },
        ..Secret::default()
    };

    event!(Level::INFO, name, namespace, "Creating Secret");

    // Create the pvc defined above
    let service_api: Api<Secret> = Api::namespaced(client, namespace);
    let params = PatchParams::apply(name);
    service_api
        .patch(name, &params, &Patch::Apply(&object))
        .await
}

#[instrument(skip(client))]
pub async fn delete(client: Client, name: String, namespace: String) -> Result<(), Error> {
    event!(Level::INFO, name, namespace, "Deleting Secret");

    let api: Api<Secret> = Api::namespaced(client, namespace.as_str());
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
pub async fn get_data(
    client: Client,
    name: &str,
    namespace: &str,
) -> Result<BTreeMap<String, ByteString>, crate::Error> {
    let service_api: Api<Secret> = Api::namespaced(client, namespace);

    let default_config = match service_api.get_opt(name).await? {
        Some(res) => res,
        None => {
            return Err(crate::Error::SecretMapError(format!(
                "Secret {name} not found"
            )));
        }
    };

    match default_config.data {
        Some(c) => Ok(c),
        None => Err(crate::Error::SecretMapError(
            "Secret missing data".to_string(),
        )),
    }
}
