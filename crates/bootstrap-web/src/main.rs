#![allow(unused_imports, unused_variables)]
use actix_web::{
    App, HttpRequest, HttpResponse, HttpServer, Responder, get,
    middleware::{self, Compat},
    web::{self, Data},
};
use bootstrap_web::{bootstrap::get_list, swarm, telemetry};
use k8s_openapi::api::core::v1::ConfigMap;
use kube::{Api, Client, api::ListParams};
use prometheus::{Encoder, TextEncoder};
use serde::Deserialize;
use tracing::{info, instrument};
use tracing_actix_web::TracingLogger;

#[derive(Clone)]
pub struct State {
    client: Client,
    /// Metrics registry
    registry: prometheus::Registry,
}

impl State {
    pub async fn new() -> Self {
        Self {
            client: Client::try_default()
                .await
                .expect("failed to create kube Client"),
            registry: prometheus::Registry::new(),
        }
    }

    /// Metrics getter
    pub fn metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.registry.gather()
    }
}

#[get("/metrics")]
async fn metrics(c: Data<State>, _req: HttpRequest) -> impl Responder {
    let metrics = c.metrics();
    let encoder = TextEncoder::new();
    let mut buffer = vec![];
    encoder.encode(&metrics, &mut buffer).unwrap();
    HttpResponse::Ok().body(buffer)
}

#[get("/health")]
async fn health(_: HttpRequest) -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

#[derive(Deserialize)]
struct Bootstrap {
    name: String,
}

#[get("/seeds")]
async fn default_seeds(c: Data<State>, _req: HttpRequest) -> impl Responder {
    let bs_list = match get_list(c.client.clone(), "bootstrap", "ipfs").await {
        Ok(list) => list,
        Err(err) => {
            tracing::error!("Failed to fetch bootstrap list: {}", err);
            return HttpResponse::InternalServerError().json("Failed to fetch bootstrap list");
        }
    };
    HttpResponse::Ok().json(bs_list)
}

#[get("/seeds/{name}")]
async fn seeds(
    bootstrap: web::Path<Bootstrap>,
    c: Data<State>,
    _req: HttpRequest,
) -> impl Responder {
    let bs_list = match get_list(c.client.clone(), &bootstrap.name, "ipfs").await {
        Ok(list) => list,
        Err(err) => {
            tracing::error!("Failed to fetch bootstrap list: {}", err);
            return HttpResponse::InternalServerError().json("Failed to fetch bootstrap list");
        }
    };
    HttpResponse::Ok().json(bs_list)
}

#[get("/swarm_key")]
async fn default_swarm_key(c: Data<State>, _req: HttpRequest) -> impl Responder {
    swarm::get_key(c.client.clone(), "bootstrap", "ipfs").await
}

#[get("/swarm_key/{name}")]
async fn swarm_key(
    bootstrap: web::Path<Bootstrap>,
    c: Data<State>,
    _req: HttpRequest,
) -> impl Responder {
    swarm::get_key(c.client.clone(), &bootstrap.name, "ipfs").await
}

#[get("/")]
async fn index(c: Data<State>, _req: HttpRequest) -> impl Responder {
    HttpResponse::Ok().json("You are here")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    telemetry::init().await;

    let state = State::new().await;
    info!("namespace: {}", state.client.default_namespace());

    let server = HttpServer::new(move || {
        App::new()
            .app_data(Data::new(state.clone()))
            .wrap(TracingLogger::default())
            .service(index)
            .service(health)
            .service(metrics)
            .service(default_seeds)
            .service(default_swarm_key)
            .service(seeds)
            .service(swarm_key)
            .service(web::scope("/").wrap(Compat::new(TracingLogger::default())))
    })
    .bind("0.0.0.0:8080")?
    .shutdown_timeout(5);

    Ok(server.run().await?)
}
