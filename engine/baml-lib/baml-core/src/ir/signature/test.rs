use anyhow::Result;
use internal_baml_diagnostics::SourceFile;
use std::path::PathBuf;
use std::sync::Arc;
use crate::{validate, validate_single_file};
use crate::ir::IntermediateRepr;
use super::*;

#[test]
fn test_signature() -> Result<()> {
    const BAML_SRC: &str = include_str!("test_data/test.baml");
    let contents = vec![SourceFile::new_allocated(PathBuf::from("test.baml"), Arc::from(BAML_SRC))];
    let mut schema = validate(&PathBuf::from("."), contents);
    schema.diagnostics.to_result()?;

    let ir = IntermediateRepr::from_parser_database(&schema.db, schema.configuration)?;
    let signature = ir.create_baml_hash();
    Ok(())
}
