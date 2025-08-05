use async_trait::async_trait;
use std::error::Error;

pub mod manager;

pub struct SessionInfo {
    pub name: String,
    pub tag: String,
}

/// The `Manager` trait defines an interface for loading and saving session information.
///
/// In Pingora's `ProxyHttp` trait, both `upstream_response_body_filter` and
/// `response_body_filter` are synchronous functions. Therefore, implementations of this
/// `Manager` trait should also avoid using async methods.
pub trait Manager: Send + Sync {
    /// Load session information by session ID.
    fn load(&self, session_id: &str) -> Result<SessionInfo, Box<dyn Error>>;

    /// Save session information by session ID.
    fn save(&self, session_id: &str, info: SessionInfo) -> Result<(), Box<dyn Error>>;
}