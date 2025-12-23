use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct GeneratorInfo {
    pub name: String,
    pub output_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BamlSrcVersionPayload {
    pub version: String,
    pub root_path: String,
    pub generators: Vec<GeneratorInfo>,
}
