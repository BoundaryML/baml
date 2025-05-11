use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct BamlSrcVersionPayload {
    pub version: String,
    pub root_path: String,
}
