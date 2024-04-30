#[cfg(test)]
mod tests;

mod ir_features;
mod runtime_interface;
use anyhow::Result;
use std::{collections::HashMap, path::PathBuf};

use internal_baml_core::{
    internal_baml_diagnostics::SourceFile, ir::repr::IntermediateRepr, validate,
};

use crate::internal::llm_client::{llm_provider::LLMProvider, retry_policy::CallablePolicy};

pub struct BamlRuntime {
    ir: IntermediateRepr,
    clients: HashMap<String, (LLMProvider, Option<CallablePolicy>)>,
}

impl BamlRuntime {
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
            clients: HashMap::new(),
        })
    }
}
