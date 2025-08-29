use crate::server::McpCenterServer;
use mc_booter::booter::Booter;
use std::error::Error;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry;
use tracing_subscriber::util::SubscriberInitExt;

mod cache;
mod config;
mod reverse_proxy;
mod server;

fn main() -> Result<(), Box<dyn Error>> {
    registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("info,{}=debug", env!("CARGO_CRATE_NAME")).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    if let Err(err) = Booter::run::<McpCenterServer>() {
        tracing::error!("Failed to start application: {}", err);
    }
    Ok(())
}
