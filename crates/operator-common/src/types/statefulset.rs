use k8s_openapi::api::apps::v1::StatefulSet;
use kube::api::DeleteParams;
use kube::{Api, Client, Error};
use tracing::{Level, event, instrument};

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
