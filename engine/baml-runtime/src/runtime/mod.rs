mod ir_features;
mod publisher;
pub(crate) mod runtime_interface;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::Result;
pub(super) use publisher::AstSignatureWrapper;

cfg_if::cfg_if!(
    if #[cfg(target_arch = "wasm32")] {
        type DashMap<K, V> = std::sync::Arc<std::sync::Mutex<std::collections::HashMap<K, V>>>;
    } else {
        use dashmap::DashMap;
    }
);

use std::sync::Arc;

use internal_baml_core::{
    internal_baml_diagnostics::{Diagnostics, SourceFile},
    internal_baml_parser_database::ParserDatabase,
    ir::repr::IntermediateRepr,
    validate,
};

use crate::internal::llm_client::{llm_provider::LLMProvider, retry_policy::CallablePolicy};

// A cached client contains provider and other related stuff(env vars, etc)
// This exists because we want to avoid creating a new provider for every request
// Add more fields here which are cache-specific to avoid percolating them inside the provider
#[derive(Clone)]
pub struct CachedClient {
    pub provider: Arc<LLMProvider>,
    pub env_vars: HashMap<String, String>,
}

impl CachedClient {
    pub fn new(provider: Arc<LLMProvider>, env_vars: HashMap<String, String>) -> Self {
        Self { provider, env_vars }
    }

    pub fn has_env_vars_changed(&self, new_env_vars: &HashMap<String, String>) -> bool {
        self.env_vars
            .iter()
            .any(|(k, v)| new_env_vars.get(k).map_or(false, |v2| v2 != v))
    }
}

#[derive(Clone)]
pub struct InternalBamlRuntime {
    pub ir: Arc<IntermediateRepr>,
    pub db: ParserDatabase,
    pub diagnostics: Diagnostics,
    clients: DashMap<String, CachedClient>,
    retry_policies: DashMap<String, CallablePolicy>,
    source_files: Vec<SourceFile>,
}

impl InternalBamlRuntime {
    pub(super) fn from_file_content<T: AsRef<str>>(
        directory: &str,
        files: &HashMap<T, T>,
    ) -> Result<Self> {
        let contents = files
            .iter()
            .map(|(path, contents)| {
                Ok(SourceFile::from((
                    PathBuf::from(path.as_ref()),
                    contents.as_ref().to_string(),
                )))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut schema = validate(&PathBuf::from(directory), contents.clone());
        schema.diagnostics.to_result()?;

        let ir = IntermediateRepr::from_parser_database(&schema.db, schema.configuration)?;
        Ok(InternalBamlRuntime {
            ir: Arc::new(ir),
            db: schema.db,
            diagnostics: schema.diagnostics,
            clients: Default::default(),
            retry_policies: Default::default(),
            source_files: contents,
        })
    }

    pub(super) fn from_files(directory: &Path, files: Vec<PathBuf>) -> Result<Self> {
        let contents: Vec<SourceFile> = files
            .iter()
            .map(|path| match std::fs::read_to_string(path) {
                Ok(contents) => Ok(SourceFile::from((path.clone(), contents))),
                Err(e) => Err(e),
            })
            .filter_map(|res| res.ok())
            .collect();
        let mut schema = validate(directory, contents.clone());
        schema.diagnostics.to_result()?;

        let ir = IntermediateRepr::from_parser_database(&schema.db, schema.configuration)?;

        Ok(Self {
            ir: Arc::new(ir),
            db: schema.db,
            diagnostics: schema.diagnostics,
            clients: Default::default(),
            retry_policies: Default::default(),
            source_files: contents,
        })
    }
}
