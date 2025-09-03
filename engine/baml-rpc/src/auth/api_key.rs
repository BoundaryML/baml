use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ApiKeyEnvironment {
    pub value: String,
}

impl ApiKeyEnvironment {
    pub const DEVELOPMENT: &'static str = "development";
    pub const STAGING: &'static str = "staging";
    pub const PRODUCTION: &'static str = "production";

    pub fn new(value: String) -> Self {
        Self { value }
    }

    pub fn development() -> Self {
        Self {
            value: Self::DEVELOPMENT.to_string(),
        }
    }

    pub fn staging() -> Self {
        Self {
            value: Self::STAGING.to_string(),
        }
    }

    pub fn production() -> Self {
        Self {
            value: Self::PRODUCTION.to_string(),
        }
    }

    pub fn short_code(&self) -> &str {
        match self.value.as_str() {
            Self::DEVELOPMENT => "dev",
            Self::STAGING => "stg",
            Self::PRODUCTION => "prod",
            _ => &self.value[..self.value.len().min(4)],
        }
    }
}
