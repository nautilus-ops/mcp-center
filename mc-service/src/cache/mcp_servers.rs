use mc_common::types::HttpScheme;
use mc_register::Registry;
use pingora_load_balancing::{Backend, LoadBalancer};
use regex::Regex;
use std::collections::HashMap;
use std::error::Error;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use tokio::time::interval;

#[derive(Clone)]
struct McpServerInfo {
    pub endpoint: String,
}

#[derive(Clone)]
pub struct Cache {
    handle: Arc<Box<dyn Registry>>,
    server_cache: Arc<RwLock<HashMap<String, HashMap<String, McpServerInfo>>>>,
    runtime: Arc<Runtime>,
}

impl Cache {
    pub fn new(handle: Arc<Box<dyn Registry>>, runtime: Arc<Runtime>, interval: u64) -> Self {
        let cache = Self {
            handle,
            server_cache: Arc::new(RwLock::new(HashMap::new())),
            runtime,
        };
        cache.async_cache(interval);
        cache
    }
    fn async_cache(&self, cache_interval: u64) {
        let cache = self.server_cache.clone();
        let handle = self.handle.clone();

        self.runtime.spawn(async move {
            let endpoint_regex = Regex::new(
                r"^(?P<scheme>https?)://(?P<host>[^/:]+)(?::(?P<port>\d+))?(?P<path>/.*)?$",
            )
            .unwrap();

            let mut ticker = interval(Duration::from_secs(cache_interval));
            loop {
                ticker.tick().await;

                let mcp_servers = match handle.list_mcp().await {
                    Ok(results) => results,
                    Err(err) => {
                        tracing::error!("Can't list mcp servers, error: {}", err);
                        continue;
                    }
                };

                let mut mcps = cache.write().await;

                mcp_servers.iter().for_each(|server| {
                    let mut tag = match &server.version {
                        None => "default".to_string(),
                        Some(version) => version.clone(),
                    };

                    if let Some(t) = &server.tag {
                        tag = t.clone();
                    }

                    let mut item = HashMap::<String, McpServerInfo>::new();
                    let result = match parse_endpoint(&endpoint_regex, server.endpoint.as_str()) {
                        Ok(result) => result,
                        Err(err) => {
                            tracing::error!(
                                "Can't parse endpoint {}, error: {}",
                                server.endpoint,
                                err
                            );
                            return;
                        }
                    };

                    let ep = format!("{}:{}", result.host, result.port);

                    item.insert(
                        tag.clone(),
                        McpServerInfo {
                            endpoint: server.endpoint.clone(),
                        },
                    );
                    mcps.insert(server.name.clone(), item);

                    tracing::info!(
                        "Load mcp server {}/{} success, endpoint: {}",
                        server.name,
                        tag,
                        ep
                    );
                });
            }
        });
    }

    pub async fn load_server_info(&self, mcp_name: &str, tag: &str) -> Option<String> {
        let cache = self.server_cache.read().await;
        if let Some(tags) = cache.get(mcp_name) {
            if let Some(info) = tags.get(tag) {
                return Some(info.endpoint.clone());
            }
        }
        None
    }
}

fn parse_endpoint(
    endpoint_regex: &Regex,
    endpoint: &str,
) -> Result<crate::proxy::ParsedEndpoint, Box<dyn Error>> {
    if let Some(caps) = endpoint_regex.captures(endpoint) {
        let scheme = caps.name("scheme").map(|m| m.as_str()).unwrap_or("");
        let host = caps.name("host").map(|m| m.as_str()).unwrap_or("");
        let port = match caps.name("port") {
            Some(p) => p.as_str(),
            None => match scheme {
                "http" => "80",
                "https" => "443",
                _ => {
                    return Err("Unsupported scheme".into());
                }
            },
        };
        let path = caps.name("path").map(|m| m.as_str()).unwrap_or("/");
        Ok(crate::proxy::ParsedEndpoint {
            endpoint: endpoint.to_string(),
            host: host.to_string(),
            port: port.to_string(),
            path: path.to_string(),
            scheme: HttpScheme::from_str(scheme)?,
        })
    } else {
        Err(format!("Failed to parse endpoint {endpoint}").into())
    }
}
