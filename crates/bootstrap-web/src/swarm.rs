use actix_web::HttpResponse;
use kube::Client;
use operator_common::types::configmap;

pub async fn get_key(client: Client, name: &str, namespace: &str) -> HttpResponse {
    match configmap::get_data(client, &format!("{name}-swarm-key"), namespace).await {
        Ok(key) => HttpResponse::Ok().body(
            key.get("swarm.key")
                .unwrap_or(&"not set".to_string())
                .to_string(),
        ),
        Err(err) => HttpResponse::InternalServerError().body(err.to_string()),
    }
}
