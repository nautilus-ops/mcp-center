use std::error::Error;
use std::fmt::{Debug, Display, Formatter};

pub mod manager;

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub name: String,
    pub tag: String,
    pub scheme: String,
    pub host: String,
}

pub struct ManagerError {
    details: String,
}

impl ManagerError {
    pub fn new(details: &str) -> ManagerError {
        ManagerError {
            details: details.to_string(),
        }
    }
}

impl Debug for ManagerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.details)
    }
}

impl Display for ManagerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.details)
    }
}

impl Error for ManagerError {}

/// The `Manager` trait defines an interface for loading and saving session information.
///
/// In Pingora's `ProxyHttp` trait, both `upstream_response_body_filter` and
/// `response_body_filter` are synchronous functions. Therefore, implementations of this
/// `Manager` trait should also avoid using async methods.
pub trait Manager: Send + Sync {
    /// Load session information by session ID.
    fn load(&self, session_id: &str) -> Result<SessionInfo, ManagerError>;

    /// Save session information by session ID.
    fn save(&self, session_id: &str, info: SessionInfo) -> Result<(), ManagerError>;
}
