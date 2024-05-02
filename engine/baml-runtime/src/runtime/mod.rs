#[cfg(test)]
mod tests;

mod ir_features;
mod runtime_interface;

use anyhow::Result;
use std::{collections::HashMap, path::PathBuf};

use dashmap::DashMap;
use internal_baml_core::{
    internal_baml_diagnostics::{Diagnostics, SourceFile},
    ir::repr::IntermediateRepr,
    validate,
};
use std::sync::Arc;

use crate::internal::llm_client::{llm_provider::LLMProvider, retry_policy::CallablePolicy};

pub struct InternalBamlRuntime {
    ir: IntermediateRepr,
    diagnostics: Diagnostics,
    clients: DashMap<String, (Arc<LLMProvider>, Option<CallablePolicy>)>,
}

impl InternalBamlRuntime {
    pub(super) fn from_file_content(
        directory: &str,
        files: &HashMap<String, String>,
    ) -> Result<Self> {
        let contents = files
            .iter()
            .map(|(path, contents)| {
                Ok(SourceFile::from((
                    PathBuf::from(path),
                    contents.to_string(),
                )))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut schema = validate(&PathBuf::from(directory), contents);
        schema.diagnostics.to_result()?;

        let ir = IntermediateRepr::from_parser_database(&schema.db)?;

        Ok(Self {
            ir,
            diagnostics: schema.diagnostics,
            clients: DashMap::new(),
        })
    }

    pub(super) fn from_files(directory: &PathBuf, files: Vec<PathBuf>) -> Result<Self> {
        let contents = files
            .iter()
            .map(|path| match std::fs::read_to_string(path) {
                Ok(contents) => Ok(SourceFile::from((path.clone(), contents))),
                Err(e) => Err(e),
            })
            .filter_map(|res| res.ok())
            .collect();
        let mut schema = validate(directory, contents);
        schema.diagnostics.to_result()?;

        let ir = IntermediateRepr::from_parser_database(&schema.db)?;

        Ok(Self {
            ir,
            diagnostics: schema.diagnostics,
            clients: DashMap::new(),
        })
    }
}
