use std::fmt;

#[derive(Debug)]
pub enum FlareSyncError {
    Config(String),
    Io(std::io::Error),
    Network(reqwest::Error),
    Json(serde_json::Error),
    Cloudflare(String),
}

impl fmt::Display for FlareSyncError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FlareSyncError::Config(s) => write!(f, "Configuration error: {}", s),
            FlareSyncError::Io(e) => write!(f, "IO error: {}", e),
            FlareSyncError::Network(e) => write!(f, "Network error: {}", e),
            FlareSyncError::Json(e) => write!(f, "JSON error: {}", e),
            FlareSyncError::Cloudflare(s) => write!(f, "Cloudflare API error: {}", s),
        }
    }
}

impl std::error::Error for FlareSyncError {}

impl From<reqwest::Error> for FlareSyncError {
    fn from(err: reqwest::Error) -> FlareSyncError {
        FlareSyncError::Network(err)
    }
}

impl From<std::io::Error> for FlareSyncError {
    fn from(err: std::io::Error) -> FlareSyncError {
        FlareSyncError::Io(err)
    }
}

impl From<serde_json::Error> for FlareSyncError {
    fn from(err: serde_json::Error) -> FlareSyncError {
        FlareSyncError::Json(err)
    }
}
