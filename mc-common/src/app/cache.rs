
use mc_db::{DBClient, McpDBHandler};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use std::error::Error;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use tokio::sync::broadcast::Receiver;
use tokio::time::interval;
use crate::app::event::Event;
use crate::types::HttpScheme;

static REGEX_ENDPOINT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?P<scheme>https?)://(?P<host>[^/:]+)(?::(?P<port>\d+))?(?P<path>/.*)?$").unwrap()
});

#[derive(Debug, Clone)]
pub struct McpServerInfo {
    pub endpoint: String,
    pub host: String,
    pub port: String,
    pub path: String,
    pub scheme: HttpScheme,
}

impl PartialEq for McpServerInfo {
    fn eq(&self, other: &Self) -> bool {
        self.host == other.host && self.port == other.port && self.path == other.path
    }
}

#[derive(Clone)]
pub struct Cache {
    db_client: Arc<DBClient>,
    server_cache: Arc<RwLock<HashMap<String, HashMap<String, McpServerInfo>>>>,
    runtime: Arc<Runtime>,
}

impl Cache {
    pub fn new(
        db_client: Arc<DBClient>,
        receiver: Receiver<Event>,
        runtime: Arc<Runtime>,
        interval: u64,
    ) -> Self {
        let cache = Self {
            db_client,
            server_cache: Arc::new(RwLock::new(HashMap::new())),
            runtime,
        };
        cache.async_cache(interval);
        cache.handle_event(receiver);
        cache
    }
    fn async_cache(&self, cache_interval: u64) {
        let cache = self.server_cache.clone();
        let db_client = self.db_client.clone();

        self.runtime.spawn(async move {
            let mut ticker = interval(Duration::from_secs(cache_interval));
            loop {
                ticker.tick().await;

                let handler = McpDBHandler::new(db_client.clone());
                let mcp_servers = match handler.list_all().await {
                    Ok(results) => results,
                    Err(err) => {
                        tracing::error!("Can't list mcp servers, error: {}", err);
                        continue;
                    }
                };

                let mut updated_count: usize = 0;
                let all_count: usize = mcp_servers.len();

                for server in mcp_servers {
                    let r_cache = cache.read().await;

                    let tag = &server.tag;

                    let mcp_server = match parse_endpoint(server.endpoint.as_str()) {
                        Ok(p) => p,
                        Err(err) => {
                            tracing::error!("Failed to parse endpoint, error: {}", err);
                            return;
                        }
                    };

                    if let Some(tags) = r_cache.get(&server.name)
                        && let Some(item) = tags.get(&server.tag)
                        && item == &mcp_server
                    {
                        return;
                    }

                    // unlock the read lock
                    drop(r_cache);

                    updated_count += 1;

                    let mut w_cache = cache.write().await;

                    w_cache
                        .entry(server.name.clone())
                        .or_insert_with(HashMap::new)
                        .insert(tag.clone(), mcp_server);

                    tracing::info!(
                        "Load mcp server {}/{} success, endpoint: {}",
                        server.name,
                        tag,
                        server.endpoint
                    );
                }

                tracing::info!(
                    updated_count = updated_count,
                    no_need_update = all_count - updated_count,
                    "sync mcp servers done"
                );
            }
        });
    }

    pub async fn load_server_info(&self, mcp_name: &str, tag: &str) -> Option<McpServerInfo> {
        let cache = self.server_cache.read().await;
        if let Some(tags) = cache.get(mcp_name)
            && let Some(info) = tags.get(tag)
        {
            return Some(info.clone());
        }
        None
    }

    #[allow(dead_code)]
    pub async fn update_or_create_server_info(
        &self,
        mcp_name: &str,
        tag: &str,
        server: McpServerInfo,
    ) -> Result<(), Box<dyn Error>> {
        let mut cache = self.server_cache.write().await;

        match cache.get_mut(mcp_name) {
            None => {
                let mut tags = HashMap::new();
                tags.insert(tag.to_string(), server);
                cache.insert(mcp_name.to_string(), tags);
            }
            Some(tags) => {
                tags.insert(tag.to_string(), server);
            }
        };

        Ok(())
    }

    fn handle_event(&self, mut receiver: Receiver<Event>) {
        let cache = self.server_cache.clone();
        self.runtime.spawn(async move {
            while let Ok(event) = receiver.recv().await {
                match event {
                    Event::Delete { mcp_name, tag } => {
                        let mut cache = cache.write().await;
                        if let Some(tags) = cache.get_mut(&mcp_name) {
                            tags.remove(tag.as_str());
                        }
                        tracing::info!("Remove mcp server {}/{} from cache", mcp_name, tag);
                    }
                    Event::CreateOrUpdate {
                        mcp_name,
                        tag,
                        endpoint,
                    } => {
                        let server = parse_endpoint(endpoint.as_str())
                            .map_err(|err| {
                                tracing::error!("Failed to parse endpoint, error: {}", err);
                            })
                            .unwrap();

                        let mut cache = cache.write().await;

                        match cache.get_mut(&mcp_name) {
                            None => {
                                let mut tags = HashMap::new();
                                tags.insert(tag.to_string(), server);
                                cache.insert(mcp_name.to_string(), tags);
                            }
                            Some(tags) => {
                                tags.insert(tag.to_string(), server);
                            }
                        };
                        tracing::info!("update or create mcp server {}/{} success", mcp_name, tag);
                    }
                }
            }
        });
    }
}
fn parse_endpoint(endpoint: &str) -> Result<McpServerInfo, Box<dyn Error>> {
    if let Some(caps) = REGEX_ENDPOINT.captures(endpoint) {
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
        Ok(McpServerInfo {
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
