use serde::{Deserialize, Serialize};
use ts_rs::TS;

mod api_key;
mod permissions;

pub use api_key::*;
pub use permissions::*;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ApiKeyMetadata {
    pub api_key_id: String,
    pub key_prefix: String,
    pub name: String,
    pub description: Option<String>,
    pub org_id: String,
    pub project_id: String,
    pub environment: String,
    pub permissions: Vec<Permission>,
    pub created_by_user_id: String,
    pub last_used_at: Option<String>, // ISO8601 timestamp
    pub expires_at: Option<String>,   // ISO8601 timestamp
    pub is_active: bool,
    pub created_at: String, // ISO8601 timestamp
    pub updated_at: String, // ISO8601 timestamp
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CreateApiKeyRequest {
    pub name: String,
    pub description: Option<String>,
    pub project_id: String,
    pub environment: String,
    pub permissions: Vec<Permission>,
    pub expires_at: Option<String>, // ISO8601 timestamp
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CreateApiKeyResponse {
    pub api_key: String, // The actual key, only shown once
    pub metadata: ApiKeyMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UpdateApiKeyRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub permissions: Option<Vec<Permission>>,
    pub is_active: Option<bool>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ListApiKeysRequest {
    pub project_id: Option<String>,
    pub environment: Option<String>,
    pub is_active: Option<bool>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ListApiKeysResponse {
    pub api_keys: Vec<ApiKeyMetadata>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RotateApiKeyResponse {
    pub new_api_key: String, // The actual key, only shown once
    pub metadata: ApiKeyMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ApiKeyValidationResult {
    pub is_valid: bool,
    pub metadata: Option<ApiKeyMetadata>,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_typescript_bindings() {
        // This test forces the TypeScript generation to run
        let _ = ApiKeyMetadata::export();
        let _ = CreateApiKeyRequest::export();
        let _ = CreateApiKeyResponse::export();
        let _ = UpdateApiKeyRequest::export();
        let _ = ListApiKeysRequest::export();
        let _ = ListApiKeysResponse::export();
        let _ = RotateApiKeyResponse::export();
        let _ = ApiKeyValidationResult::export();
        let _ = Permission::export();
        let _ = PermissionTemplate::export();
    }
}
