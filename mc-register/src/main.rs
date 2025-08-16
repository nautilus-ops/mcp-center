use crate::service::server::RegisterService;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry;
use tracing_subscriber::util::SubscriberInitExt;
use mc_booter::booter::Booter;

mod service;

fn main() {
    registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("info,{}=debug", env!("CARGO_CRATE_NAME")).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    if let Err(err) = Booter::run::<RegisterService>() {
        tracing::error!("Failed to start application: {}", err);
    }
}
