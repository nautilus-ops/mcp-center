use crate::app::application::Application;
use clap::{Parser, Subcommand};
use regex::Regex;
use std::error::Error;
use std::path::PathBuf;
use std::process::exit;
use std::sync::Arc;
use std::{env, fs};
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
    pub fn run<T: Application>() -> Result<(), Box<dyn Error>> {
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
        tracing::info!("Preparing application with config: {}", filepath);

        tracing::info!("Starting Service");

        let rt = Builder::new_multi_thread()
            .worker_threads(num_cpus::get())
            .enable_all()
            .build()
            .unwrap();

        let cancellation_token = CancellationToken::new();
        let shutdown_token = cancellation_token.clone();

        // create application
        let mut application = T::new();

        let mut content = fs::read_to_string(filepath.clone()).map_err(|e| {
            tracing::error!("Failed to read config file {}: {}", filepath, e);
            e
        })?;

        content = replace_env_variables(content);

        // parse application config
        let config: T::Config = toml::from_str(&content).map_err(|e| {
            tracing::error!("Failed to parse TOML config: {}", e);
            e
        })?;

        let runtime = Arc::new(rt);

        // you can initialize some global resources, like db client.
        application.prepare(config, runtime.clone())?;

        runtime.clone().spawn(async move {
            let mut sigterm = signal(SignalKind::terminate()).expect("failed to bind SIGTERM");

            // listen the system signal to stop the application
            tokio::select! {
                _ = sigterm.recv() => {
                    tracing::info!("Received SIGTERM, shutting down...");
                },
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("Received Ctrl+C, shutting down directly...");
                    // When long connections exist, axum seems unable to gracefully shut down. So need exit(0)
                    exit(0)
                }
            }
            shutdown_token.cancel();
        });

        // start to run application
        if let Err(err) = application.run(cancellation_token, runtime.clone()) {
            tracing::error!("Error running application: {}", err);
        }

        Ok(())
    }
}

pub fn replace_env_variables(input: String) -> String {
    let re = Regex::new(r#""\$\{(\w+)(?::([^}]*))?\}""#).unwrap();

    re.replace_all(&input, |caps: &regex::Captures| {
        let var_name = &caps[1];
        let default = caps.get(2).map(|m| m.as_str()).unwrap_or("");

        let val = env::var(var_name).unwrap_or_else(|_| default.to_string());

        if val.parse::<f64>().is_ok() {
            val
        } else if val == "true" || val == "false" {
            val.to_string()
        } else {
            format!("\"{val}\"")
        }
    })
    .into_owned()
}

#[cfg(test)]
mod tests {
    use crate::booter::replace_env_variables;
    use std::env;

    #[test]
    fn test_replace_env_variables() {
        struct TestCase {
            input: &'static str,
            want: &'static str,
        }

        let tests = vec![
            TestCase {
                input: r#"self_addr = "${SELF_ADDR:http://127.0.0.1}""#,
                want: r#"self_addr = "http://127.0.0.1""#,
            },
            TestCase {
                input: r#"mcp_definition_path = "${SERVER_DEFINITION_PATH:mcp_servers.toml}""#,
                want: r#"mcp_definition_path = "mcp_servers.toml""#,
            },
            TestCase {
                input: r#"port = "${POSTGRES_PORT:5432}""#,
                want: r#"port = 5432"#,
            },
            TestCase {
                input: r#"host = "${POSTGRES_HOST}""#,
                want: r#"host = "127.0.0.1""#,
            },
        ];

        unsafe { env::set_var("POSTGRES_HOST", "127.0.0.1") }
        tests
            .into_iter()
            .for_each(|t| assert_eq!(replace_env_variables(t.input.to_string()), t.want));
    }
}
