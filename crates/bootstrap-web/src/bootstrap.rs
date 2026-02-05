use controller::{identity::Identities, ipfs::get_bootstrap_list};
use kube::{Client, Error, core::ErrorResponse};
use operator_common::types::configmap;
use tracing::{info, instrument};

#[instrument(skip(client))]
pub async fn get_list(client: Client, name: &str, namespace: &str) -> Result<Vec<String>, Error> {
    info!("Getting bootstrap list: {}", name);
    let (identities, external_addrs) = (
        match Identities::get(client.clone(), &format!("{name}-identities"), namespace).await {
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
            format!("{name}-external-addresses").as_str(),
            namespace,
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
    );

    let bootstrap_list = match get_bootstrap_list(identities, external_addrs).await {
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

    Ok(bootstrap_list)
}
