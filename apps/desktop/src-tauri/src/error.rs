//! Single error type returned across the Tauri command boundary.
//!
//! Frontend code receives a `{ kind, message }` JSON object and can switch on
//! `kind` for typed handling. Internal `?` propagation works via `From` impls.

use serde::Serialize;
use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum QuillError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("crypto error: {0}")]
    Crypto(String),

    #[error("git error: {0}")]
    Git(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl QuillError {
    fn kind(&self) -> &'static str {
        match self {
            Self::Io(_) => "io",
            Self::Serde(_) => "serde",
            Self::Crypto(_) => "crypto",
            Self::Git(_) => "git",
            Self::NotFound(_) => "not_found",
            Self::InvalidArgument(_) => "invalid_argument",
            Self::Storage(_) => "storage",
            Self::Internal(_) => "internal",
        }
    }
}

#[derive(Serialize)]
struct WireError<'a> {
    kind: &'a str,
    message: String,
}

impl Serialize for QuillError {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        WireError {
            kind: self.kind(),
            message: self.to_string(),
        }
        .serialize(s)
    }
}

impl From<anyhow::Error> for QuillError {
    fn from(e: anyhow::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

impl From<argon2::Error> for QuillError {
    fn from(e: argon2::Error) -> Self {
        Self::Crypto(e.to_string())
    }
}

impl From<aes_gcm::Error> for QuillError {
    fn from(e: aes_gcm::Error) -> Self {
        Self::Crypto(e.to_string())
    }
}

impl From<std::string::FromUtf8Error> for QuillError {
    fn from(e: std::string::FromUtf8Error) -> Self {
        Self::Internal(e.to_string())
    }
}

/// Helper for constructing `Storage` errors with formatting.
#[allow(unused_macros)]
macro_rules! storage_err {
    ($($arg:tt)*) => {
        $crate::error::QuillError::Storage(format!($($arg)*))
    };
}

/// Display-as-Debug for tracing convenience.
pub struct DisplayErr<'a>(pub &'a QuillError);
impl fmt::Display for DisplayErr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub type Result<T, E = QuillError> = std::result::Result<T, E>;
