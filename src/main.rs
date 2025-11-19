use critical_one::{config::Config, create_app};
use std::env;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() {
    let env: String = env::var("RUN_ENV").unwrap_or_else(|_| "default".into());
    let config: Config = Config::load().expect("Failed to load config.");

    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| config.logging.level.clone().into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    tracing::info!(run_env = %env, "Starting Critical One server...");

    let app = create_app(config.clone());
    tracing::info!("Listening on {}", &config.server.addr);

    let listener = tokio::net::TcpListener::bind(&config.server.addr)
        .await
        .unwrap();

    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!("server error: {}", e);
    }
}
