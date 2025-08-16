use std::error::Error;
use tokio::runtime::Runtime;
use tokio_util::sync::CancellationToken;

pub trait Application: Send + Sync {
    fn prepare(&mut self, config: String) -> Result<(), Box<dyn Error>>;

    fn run(&mut self, shutdown: CancellationToken, rt: Runtime) -> Result<(), Box<dyn Error>>;
}
