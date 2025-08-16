use crate::app::application::Application;
use clap::{Parser, Subcommand};
use std::error::Error;
use std::path::PathBuf;
use tokio::runtime::Builder;
use tokio::signal::unix::{SignalKind, signal};
use tokio_util::sync::CancellationToken;

const DEFAULT_BOOTSTRAP_FILEPATH: &str = "/etc/nautilus/bootstrap.toml";

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// The subcommand to execute.
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Runs the application with an optional configuration file path.
    Run {
        #[arg(short, long, value_name = "FILE")]
        config: Option<PathBuf>,
    },
}

pub struct Booter;

impl Booter {
    pub fn run<T: Application>(mut application: T) -> Result<(), Box<dyn Error>> {
        let cli = Cli::parse();

        let mut filepath = String::new();
        
        match &cli.command {
            Some(Commands::Run { config }) => match config.clone() {
                None => {
                    filepath = String::from(DEFAULT_BOOTSTRAP_FILEPATH);
                }
                Some(fp) => {
                    filepath = format!("{}", fp.display()).to_string();
                }
            },
            None => {
                eprintln!("Unknown or missing command. Use --help for usage information.");
            }
        }

        tracing::info!("Starting Service");
        
        let rt = Builder::new_multi_thread()
            .worker_threads(num_cpus::get())
            .enable_all()
            .build()
            .unwrap();

        let cancellation_token = CancellationToken::new();
        let shutdown_token = cancellation_token.clone();

        application.prepare(filepath)?;
        
        rt.spawn(async move {
            let mut sigterm = signal(SignalKind::terminate()).expect("failed to bind SIGTERM");

            tokio::select! {
                _ = sigterm.recv() => {
                    tracing::info!("Received SIGTERM, shutting down...");
                },
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("Received Ctrl+C, shutting down...");
                }
            }
            shutdown_token.cancel();
        });
        
        if let Err(err) = application.run(cancellation_token, rt) {
            tracing::error!("Error running application: {}", err);
        }

        Ok(())
    }
}
