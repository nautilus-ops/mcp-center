use std::env;
use std::error::Error;
use crate::booter::Booter;
use crate::service::proxy::MainServer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry;
use tracing_subscriber::util::SubscriberInitExt;

mod app;
mod booter;
mod service;
mod envs;

fn main() -> Result<(), Box<dyn Error>> {
    registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("info,{}=debug", env!("CARGO_CRATE_NAME")).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    // match &self.bootstrap.registry {
    //     Registry::SelfManagement => SelfManagerHandler::new(),
    //     Registry::Nacos(_) => SelfManagerHandler::new(),
    //     Registry::Redis(_) => SelfManagerHandler::new(),
    //     Registry::ExternalAPI(config) => {
    //         
    //     }
    // };
    

    if let Err(err) = Booter::run(MainServer::new()) {
        tracing::error!("Failed to start application: {}", err);
    }
    Ok(())
}
