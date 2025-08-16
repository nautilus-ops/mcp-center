use core::fmt;
use std::cmp::PartialEq;
use std::fmt::Display;
use std::str::FromStr;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum HttpScheme {
    Http,
    Https,
}

impl FromStr for HttpScheme {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "http" => Ok(HttpScheme::Http),
            "https" => Ok(HttpScheme::Https),
            _ => Err(format!("Unknown scheme: {}", s)),
        }
    }
}

impl HttpScheme {
    pub fn as_str(&self) -> &'static str {
        match self {
            HttpScheme::Http => "http",
            HttpScheme::Https => "https",
        }
    }

    pub fn is_http(&self) -> bool {
        self == &HttpScheme::Http
    }

    pub fn is_https(&self) -> bool {
        self == &HttpScheme::Https
    }
}
