use pingora_core::{HTTPStatus, InternalError};
use regex::Regex;

#[cfg(test)]
use assert2::assert;

pub enum Router {
    ConnectRouter {
        name: String,
        tag: String,
    },
    MessageRouter {
        name: String,
        tag: String,
        message_path: String,
        session_id: String,
    },
}

pub struct Matcher {
    connect_regex: Regex,
    message_regex: Regex,
}

impl Matcher {
    pub fn new() -> Self {
        Self {
            connect_regex: Regex::new(r"^/connect/([^/]+)/([^/]+)(/.*)?$").unwrap(),
            message_regex: Regex::new(
                r"^/message/([^/]+)/([^/]+)(/.*)?\?sessionId=([0-9a-fA-F\-]+)$",
            )
            .unwrap(),
        }
    }

    pub fn matching(&self, uri: String) -> pingora_core::Result<Router> {
        if uri.len() == 1 {
            // TODO router : http://<your_host>:<your_port>/
            return Ok(Router::ConnectRouter {
                name: "".to_string(),
                tag: "".to_string(),
            });
        }

        if uri.len() >= 2 {
            if let Some(ch) = uri.chars().nth(1) {
                match ch {
                    // /message
                    'm' => {
                        return {
                            let (name, tag, message_path, session_id) =
                                self.parse_message_router(uri.as_str())?;
                            Ok(Router::MessageRouter {
                                name,
                                tag,
                                message_path,
                                session_id,
                            })
                        };
                    }
                    // /connect/{mcp_name}/{tag}
                    'c' => {
                        return {
                            let (name, tag) = self.parse_connection_router(uri.as_str())?;
                            Ok(Router::ConnectRouter { name, tag })
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

    pub fn parse_message_router(
        &self,
        uri: &str,
    ) -> pingora_core::Result<(String, String, String, String)> {
        if let Some(caps) = self.message_regex.captures(uri) {
            let name = caps.get(1).unwrap().as_str().to_string();
            let tag = caps.get(2).unwrap().as_str().to_string();
            let sub_path = caps
                .get(3)
                .map_or("".to_string(), |m| m.as_str().to_string());
            let session_id = caps.get(4).unwrap().as_str().to_string();
            return Ok((name, tag, sub_path, session_id));
        }

        tracing::error!("Can't parse [message] uri {}", uri);
        Err(pingora_core::Error::new(InternalError))
    }
}

#[test]
fn test_connect_router() {
    let matcher = Matcher::new();

    let cases = vec![
        ("/connect/foo/1.0.0", "foo", "1.0.0"),
        ("/connect/bar/2.0.0", "bar", "2.0.0"),
        ("/connect/test-service/v1.0.0", "test-service", "v1.0.0"),
    ];

    for (case, want_name, want_tag) in cases {
        let router = matcher.matching(case.to_string()).unwrap();
        
        match router {
            Router::ConnectRouter { name, tag } => {
                assert_eq!(name, want_name, "name mismatch: got {}, want {}", name, want_name);
                assert_eq!(tag, want_tag, "tag mismatch: got {}, want {}", tag, want_tag);
            }
            _ => panic!("Expected ConnectRouter, got different router type"),
        }
    }
}

#[test]
fn test_root_path() {
    let matcher = Matcher::new();
    let router = matcher.matching("/".to_string()).unwrap();
    
    match router {
        Router::ConnectRouter { name, tag } => {
            assert_eq!(name, "", "name should be empty for root path, got {}", name);
            assert_eq!(tag, "", "tag should be empty for root path, got {}", tag);
        }
        _ => panic!("Expected ConnectRouter for root path"),
    }
}

#[test]
fn test_invalid_paths() {
    let matcher = Matcher::new();

    let invalid_cases = vec![
        "/invalid",
        "/message/invalid",
        "/connect/invalid",
        "/message/foo/1.0.0", // missing sessionId
        "/message/foo/1.0.0/api?sessionId=invalid-uuid",
    ];

    for case in invalid_cases {
        let result = matcher.matching(case.to_string());
        assert!(result.is_err(), "Expected error for invalid path: {}", case);
    }
}

#[test]
fn test_parse_connection_router() {
    let matcher = Matcher::new();
    
    let (name, tag) = matcher.parse_connection_router("/connect/foo/1.0.0").unwrap();
    assert_eq!(name, "foo", "name mismatch: got {}, want foo", name);
    assert_eq!(tag, "1.0.0", "tag mismatch: got {}, want 1.0.0", tag);
    
    let result = matcher.parse_connection_router("/invalid/path");
    assert!(result.is_err(), "Expected error for invalid connection path");
}

#[test]
fn test_parse_message_router() {
    let matcher = Matcher::new();
    
    let (name, tag, message_path, session_id) = matcher
        .parse_message_router("/message/foo/1.0.0/api/message?sessionId=49b420bb-adc1-4231-917a-08822da1e8f3")
        .unwrap();
    
    assert_eq!(name, "foo", "name mismatch: got {}, want foo", name);
    assert_eq!(tag, "1.0.0", "tag mismatch: got {}, want 1.0.0", tag);
    assert_eq!(message_path, "/api/message", "message_path mismatch: got {}, want /api/message", message_path);
    assert_eq!(session_id, "49b420bb-adc1-4231-917a-08822da1e8f3", "session_id mismatch: got {}, want 49b420bb-adc1-4231-917a-08822da1e8f3", session_id);
    
    let result = matcher.parse_message_router("/invalid/path");
    assert!(result.is_err(), "Expected error for invalid message path");
}

#[test]
fn test_message_path_regex() {
    let matcher = Matcher::new();

    let cases = vec![
        ("/message/foo/1.0.0/api/message?sessionId=49b420bb-adc1-4231-917a-08822da1e8f3", "foo", "1.0.0", "/api/message", "49b420bb-adc1-4231-917a-08822da1e8f3"),
        ("/message/foo/1.0.0/message?sessionId=49b420bb-adc1-4231-917a-08822da1e8f3", "foo", "1.0.0", "/message", "49b420bb-adc1-4231-917a-08822da1e8f3"),
        ("/message/foo/1.0.0/api/sse/message?sessionId=49b420bb-adc1-4231-917a-08822da1e8f3", "foo", "1.0.0", "/api/sse/message", "49b420bb-adc1-4231-917a-08822da1e8f3"),
        ("/message/foo/1.0.0/anything?sessionId=49b420bb-adc1-4231-917a-08822da1e8f3", "foo", "1.0.0", "/anything", "49b420bb-adc1-4231-917a-08822da1e8f3"),
    ];

    for (case, want_name, want_tag, want_message_path, want_session_id) in cases {
        let router = matcher.matching(case.to_string()).unwrap();
        
        match router {
            Router::MessageRouter { name, tag, message_path, session_id } => {
                assert_eq!(name, want_name, "name mismatch: got {}, want {}", name, want_name);
                assert_eq!(tag, want_tag, "tag mismatch: got {}, want {}", tag, want_tag);
                assert_eq!(message_path, want_message_path, "message_path mismatch: got {}, want {}", message_path, want_message_path);
                assert_eq!(session_id, want_session_id, "session_id mismatch: got {}, want {}", session_id, want_session_id);
            }
            _ => panic!("Expected MessageRouter, got different router type"),
        }
    }
}
