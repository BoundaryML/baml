use std::path::{Path, PathBuf};

use tokio::fs;

use crate::api::errors::ApiError;

#[derive(Debug, Clone)]
pub struct WorkspaceFileAccess {
    workspace_roots: Vec<PathBuf>,
}

impl WorkspaceFileAccess {
    pub fn new(workspace_roots: Vec<PathBuf>) -> Self {
        Self { workspace_roots }
    }

    pub async fn read_file(&self, path: &str) -> Result<Vec<u8>, ApiError> {
        let resolved_path = self.resolve_path(path)?;
        self.validate_access(&resolved_path)?;

        fs::read(&resolved_path)
            .await
            .map_err(|e| ApiError::NotFound(format!("File not found: {}", e)))
    }

    pub fn resolve_path(&self, path: &str) -> Result<PathBuf, ApiError> {
        if path.starts_with("baml_src://") {
            // Handle baml_src:// URI scheme
            let relative_path = path.strip_prefix("baml_src://").unwrap();
            // Find baml_src directory in workspace
            for root in &self.workspace_roots {
                let baml_src = root.join("baml_src");
                if baml_src.exists() {
                    return Ok(baml_src.join(relative_path));
                }
            }
            Err(ApiError::NotFound(
                "baml_src directory not found".to_string(),
            ))
        } else if path.starts_with("/") || path.contains(":\\") {
            // Absolute path - validate it's within workspace
            Ok(PathBuf::from(path))
        } else {
            // Relative path - resolve against first workspace root
            match self.workspace_roots.first() {
                Some(root) => Ok(root.join(path)),
                None => Err(ApiError::BadRequest(
                    "No workspace root available".to_string(),
                )),
            }
        }
    }

    fn validate_access(&self, path: &Path) -> Result<(), ApiError> {
        let canonical = path
            .canonicalize()
            .map_err(|_| ApiError::NotFound("Path not found".to_string()))?;

        // Ensure path is within one of the workspace roots
        for root in &self.workspace_roots {
            if let Ok(canonical_root) = root.canonicalize() {
                if canonical.starts_with(&canonical_root) {
                    return Ok(());
                }
            }
        }

        Err(ApiError::Unauthorized(
            "Access denied: path outside workspace".to_string(),
        ))
    }
}
