use crate::app::application::Application;
use crate::app::config::McpCenter;
use clap::{Parser, Subcommand};
use std::error::Error;
use std::path::PathBuf;
use std::process::exit;
use tokio::runtime::Builder;
use tokio::signal::unix::{SignalKind, signal};
use tokio_util::sync::CancellationToken;

/// The default filepath used to load the application configuration
/// if no path is explicitly provided via CLI arguments.
const DEFAULT_BOOTSTRAP_FILEPATH: &str = "/etc/nautilus/bootstrap.toml";

/// CLI entry point for the application.
///
/// This struct defines the top-level command-line interface
/// including supported subcommands and arguments.
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
        // Parse command line arguments using clap
        let cli = Cli::parse();

        let mut filepath = String::new();

        // Determine the configuration file path based on CLI arguments
        match &cli.command {
            Some(Commands::Run { config }) => match config.clone() {
                None => {
                    // Use default configuration file path if none specified
                    filepath = String::from(DEFAULT_BOOTSTRAP_FILEPATH);
                }
                Some(fp) => {
                    // Convert PathBuf to string for the provided config path
                    filepath = format!("{}", fp.display()).to_string();
                }
            },
            None => {
                // Handle case where no valid command is provided
                eprintln!("Unknown or missing command. Use --help for usage information.");
            }
        }

        tracing::info!("Starting Service");

        // Create a multi-threaded Tokio runtime for async operations
        let rt = Builder::new_multi_thread()
            .worker_threads(num_cpus::get())
            .enable_all()
            .build()
            .unwrap();
        // Execute the main application logic within the async runtime

        let cancellation_token = CancellationToken::new();
        let shutdown_token = cancellation_token.clone();

        application.prepare(filepath)?;

        // Spawn a background base to listen for CTRL+C signal
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
            // Signal all components to shutdown gracefully
            shutdown_token.cancel();
        });

        // Run the main application with cancellation support
        if let Err(err) = application.run(cancellation_token, rt) {
            tracing::error!("Error running application: {}", err);
        }

        Ok(())
    }
}
