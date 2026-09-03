use std::io;

use actix_web::{middleware::Logger, web, App, HttpServer};
use fms_mq_gateway::http::{auth_token_data_from_env, configure_routes};

#[cfg(not(feature = "rocketmq-backend"))]
use fms_mq_gateway::memory::InMemoryTransport;
#[cfg(feature = "rocketmq-backend")]
use fms_mq_gateway::rocketmq::RocketMqTransport;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let host = std::env::var("MQ_GATEWAY_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("MQ_GATEWAY_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8097);

    #[cfg(feature = "rocketmq-backend")]
    let transport = RocketMqTransport::from_env()
        .await
        .map_err(|error| io::Error::other(error.to_string()))?;

    #[cfg(not(feature = "rocketmq-backend"))]
    let transport = {
        log::warn!("mq-gateway built without rocketmq-backend; using in-memory transport");
        InMemoryTransport::default()
    };

    let transport: web::Data<std::sync::Arc<dyn fms_mq_gateway::transport::MessageTransport>> =
        web::Data::new(std::sync::Arc::new(transport));
    let auth_token = auth_token_data_from_env();

    log::info!("starting mq-gateway on {host}:{port}");
    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .app_data(transport.clone())
            .app_data(auth_token.clone())
            .configure(configure_routes)
    })
    .bind((host, port))?
    .run()
    .await
}
