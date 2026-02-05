use std::fmt;

#[derive(Debug)]
pub enum CapitolTradesError {
    Api(capitoltrades_api::Error),
    Cache(String),
    Serialization(serde_json::Error),
    InvalidInput(String),
}

impl fmt::Display for CapitolTradesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api(e) => write!(f, "API error: {}", e),
            Self::Cache(msg) => write!(f, "Cache error: {}", msg),
            Self::Serialization(e) => write!(f, "Serialization error: {}", e),
            Self::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
        }
    }
}

impl std::error::Error for CapitolTradesError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Api(e) => Some(e),
            Self::Serialization(e) => Some(e),
            _ => None,
        }
    }
}

impl From<capitoltrades_api::Error> for CapitolTradesError {
    fn from(e: capitoltrades_api::Error) -> Self {
        Self::Api(e)
    }
}

impl From<serde_json::Error> for CapitolTradesError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization(e)
    }
}
