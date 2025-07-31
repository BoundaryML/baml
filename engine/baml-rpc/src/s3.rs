use baml_ids::ProjectId;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct S3UploadMetadata {
    pub project_id: ProjectId,
    pub api_key_name: String,
    pub env_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baml_runtime: Option<String>,
}

impl S3UploadMetadata {
    pub fn to_map(&self) -> IndexMap<String, String> {
        let mut map = IndexMap::new();
        map.insert("project_id".to_string(), self.project_id.to_string());
        map.insert("api_key_name".to_string(), self.api_key_name.clone());
        map.insert("env_name".to_string(), self.env_name.clone());
        if let Some(baml_runtime) = &self.baml_runtime {
            map.insert("baml_runtime".to_string(), baml_runtime.clone());
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use serde_json::{self, json};

    use super::*;

    #[test]
    fn test_s3_upload_metadata_deserialization() -> Result<()> {
        let project_id = ProjectId::new();
        let example = json!({
            "project_id": project_id.to_string(),
            "api_key_name": "test-api-key",
            "env_name": "test-env"
        });

        let metadata: S3UploadMetadata = serde_json::from_value(example)?;

        assert_eq!(metadata.project_id, project_id);
        assert_eq!(metadata.api_key_name, "test-api-key");
        assert_eq!(metadata.env_name, "test-env");

        Ok(())
    }

    #[test]
    fn test_s3_upload_metadata_to_map() -> Result<()> {
        let metadata = S3UploadMetadata {
            project_id: ProjectId::new(),
            api_key_name: "test-api-key".to_string(),
            env_name: "test-env".to_string(),
            baml_runtime: None,
        };

        assert_eq!(
            serde_json::to_value(metadata.to_map())?,
            serde_json::to_value(metadata)?
        );

        Ok(())
    }
}
