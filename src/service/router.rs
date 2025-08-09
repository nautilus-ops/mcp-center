use pingora_core::{HTTPStatus, InternalError};
use pingora_proxy::Session;
use regex::{Captures, Regex};

pub enum Router {
    ConnectRouter(String, String),
    MessageRouter(String),
}

pub struct Matcher {
    connect_regex: Regex,
    message_regex: Regex,
}

impl Matcher {
    pub fn new() -> Self {
        Self {
            connect_regex: Regex::new(r"^/connect/([^/]+)/([^/]+)(/.*)?$").unwrap(),
            message_regex: Regex::new(r"^/message\?sessionId=([0-9a-fA-F\-]+)$").unwrap(),
        }
    }

    pub fn matching(&self, session: &mut Session) -> pingora_core::Result<Router> {
        let uri = session.req_header().uri.to_string();
        if uri.len() == 1 {
            // TODO router : http://<your_host>:<your_port>/
            return Ok(Router::ConnectRouter(
                "TODO".to_string(),
                "TODO".to_string(),
            ));
        }

        if uri.len() >= 2 {
            if let Some(ch) = uri.chars().nth(1) {
                match ch {
                    // /message
                    'm' => {
                        return {
                            let session_id = self.parse_message_router(uri.as_str())?;
                            Ok(Router::MessageRouter(session_id))
                        };
                    }
                    // /connect/{mcp_name}/{tag}
                    'c' => {
                        return {
                            let (mcp_name, tag) = self.parse_connection_router(uri.as_str())?;
                            Ok(Router::ConnectRouter(mcp_name, tag))
                        };
                    }
                    _ => {}
                }
            };
        }
        Err(pingora_core::Error::explain(
            HTTPStatus(404),
            "404 not found this route",
        ))
    }

    pub fn parse_connection_router(&self, uri: &str) -> pingora_core::Result<(String, String)> {
        match self.connect_regex.captures(uri) {
            None => {
                tracing::error!("Can't parse [connection] uri {}", uri);
                Err(pingora_core::Error::new(InternalError))
            }
            Some(caps) => Ok((caps[1].to_string(), caps[2].to_string())),
        }
    }

    pub fn parse_message_router(&self, uri: &str) -> pingora_core::Result<(String)> {
        if let Some(id) = self
            .message_regex
            .captures(uri)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_string())
        {
            return Ok(id.clone());
        };

        tracing::error!("Can't parse [message] uri {}", uri);
        Err(pingora_core::Error::new(InternalError))
    }
}
