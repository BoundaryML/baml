use anyhow::Result;
use std::{collections::HashMap, path::PathBuf};

use internal_baml_core::{
    internal_baml_diagnostics::SourceFile, ir::repr::IntermediateRepr, validate,
};

use crate::ir_helpers::{IRHelper, PromptRenderer, RuntimeContext};

pub struct BamlRuntime {
    ir: IntermediateRepr,
}

impl BamlRuntime {
    pub fn from_directory(directory: &PathBuf) -> Result<Self> {
        let glob_pattern = directory.join("**/*").to_string_lossy().to_string();
        let glob_pattern = if glob_pattern.starts_with(r"\\?\") {
            &glob_pattern[4..]
        } else {
            &glob_pattern
        };
        let entries = glob::glob(glob_pattern)?;

        let valid_extensions = ["baml", "json"];
        let src_files = entries
            .filter_map(|path| path.ok())
            .filter(|path| {
                path.is_file()
                    && path.extension().map_or(false, |ext| {
                        valid_extensions.contains(&ext.to_str().unwrap())
                    })
            })
            .collect::<Vec<_>>();

        Self::from_files(directory, src_files)
    }

    fn from_files(directory: &PathBuf, files: Vec<PathBuf>) -> Result<Self> {
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

        Ok(Self {
            ir: IntermediateRepr::from_parser_database(&schema.db)?,
        })
    }

    pub fn call_function(
        &self,
        function_name: &str,
        params: &HashMap<&str, serde_json::Value>,
        ctx: RuntimeContext,
    ) -> Result<()> {
        let function = self.ir.find_function(function_name)?;
        self.ir.check_function_params(&function, params)?;

        // Generate the prompt.
        let renderer = PromptRenderer::from_function(&function)?;

        // Call the LLM.
        // self.ir.find_client()

        // Parse the output.

        todo!()
    }
}
