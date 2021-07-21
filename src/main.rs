#![allow(unused_imports, unused_variables)]

use actix_web::{
    App, get,
    HttpRequest,
    HttpResponse, HttpServer, middleware, Responder, web::{self, Data},
};
use prometheus::{Encoder, TextEncoder};
use serde_json::json;
use tracing::{debug, error, info, trace, warn};
use tracing_subscriber::{EnvFilter, prelude::*, Registry};

pub use mycelium::*;

use crate::objects::Manager;
use crate::objects::minecraft_set::reconcile;

#[get("/metrics")]
async fn metrics(c: Data<Manager>, _req: HttpRequest) -> impl Responder {
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

#[get("/state")]
async fn state(c: Data<Manager>, _req: HttpRequest) -> impl Responder {
    let state = c.state().await;
    HttpResponse::Ok().json(&state)
}

#[get("/servers/{ns}/{env}/{tag}")]
async fn servers(c: Data<Manager>, path: web::Path<(String, String, String)>) -> impl Responder {
    let inner = path.into_inner();
    let vec = c.velocity(inner.1, inner.2, inner.0).await;
    HttpResponse::Ok().json(json!(vec))
}

#[actix_rt::main]
async fn main() -> Result<(), Error> {
    #[cfg(feature = "telemetry")]
        let otlp_endpoint =
        std::env::var("OPENTELEMETRY_ENDPOINT_URL").expect("otel tracing collector not configured");

    #[cfg(feature = "telemetry")]
        let tracer = opentelemetry_otlp::new_pipeline()
        .with_endpoint(&otlp_endpoint)
        .with_trace_config(opentelemetry::sdk::trace::config().with_resource(
            opentelemetry::sdk::Resource::new(vec![opentelemetry::KeyValue::new(
                "service.name",
                "mycelium-operator",
            )]),
        ))
        .with_tonic()
        .install_batch(opentelemetry::runtime::Tokio)
        .unwrap();

    // Finish layers
    #[cfg(feature = "telemetry")]
        let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
    let logger = tracing_subscriber::fmt::layer().json();

    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();

    // Register all subscribers
    #[cfg(feature = "telemetry")]
        let collector = Registry::default().with(telemetry).with(logger).with(env_filter);
    #[cfg(not(feature = "telemetry"))]
        let collector = Registry::default().with(logger).with(env_filter);

    tracing::subscriber::set_global_default(collector).unwrap();

    // Start kubernetes controller
    let (manager, set_drainer, proxy_drainer) = Manager::new().await;

    // Start web server
    let server = HttpServer::new(move || {
        App::new()
            .data(manager.clone())
            .wrap(middleware::Logger::default().exclude("/health"))
            .service(state)
            .service(servers)
            .service(health)
            .service(metrics)
    })
        .bind("0.0.0.0:8080")
        .expect("can't bind to 0.0.0.0:8080")
        .shutdown_timeout(0);

    tokio::select! {
        _ = set_drainer => warn!("set_controller exited"),
        _ = proxy_drainer => warn!("proxy_controller exited"),
        _ = server.run() => info!("actix exited"),
    }
    Ok(())
}
