use serde::de::DeserializeOwned;
use std::error::Error;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio_util::sync::CancellationToken;

pub trait Application: Send + Sync {
    type Config: DeserializeOwned;

    fn new() -> Self;
    fn prepare(&mut self, config: Self::Config, rt: Arc<Runtime>) -> Result<(), Box<dyn Error>>;

    fn run(&mut self, shutdown: CancellationToken, rt: Arc<Runtime>) -> Result<(), Box<dyn Error>>;
}
