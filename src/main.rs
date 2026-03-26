use clap::Parser;
use fetchless::config::Config;
use fetchless::{db, mcp, server};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let database = db::init_db(&config.db_path)?;
    let client = reqwest::Client::new();

    if config.mcp {
        tracing::info!("Starting Fetchless MCP server");
        mcp::run_mcp(database, client).await?;
        return Ok(());
    }

    let addr = format!("{}:{}", config.bind, config.port);
    let state = server::AppState {
        db: database,
        client,
        config: config.clone(),
    };
    let app = server::build_router(state);

    tracing::info!("Fetchless v{} listening on {addr}", env!("CARGO_PKG_VERSION"));

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl+C handler");
    tracing::info!("Shutting down gracefully...");
}
