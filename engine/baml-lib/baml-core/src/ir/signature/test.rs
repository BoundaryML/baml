use super::*;
use crate::ir::IntermediateRepr;
use crate::{validate, validate_single_file};
use anyhow::Result;
use colored::*;
use diff::lines;
use internal_baml_diagnostics::SourceFile;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

fn compute_signature(src: &str) -> Result<(MerkleTree, HashMap<String, MerkleTree>)> {
    let contents = vec![SourceFile::new_allocated(
        PathBuf::from("test.baml"),
        Arc::from(src),
    )];
    let mut schema = validate(&PathBuf::from("."), contents);
    schema.diagnostics.to_result()?;
    let ir = IntermediateRepr::from_parser_database(&schema.db, schema.configuration)?;
    let (root_node_name, all_nodes) = ir.create_merkle_tree();
    Ok((root_node_name, all_nodes))
}

#[test]
fn test_signature() -> Result<()> {
    const BAML_SRC_BEFORE: &str = include_str!("test_data/test_before.baml");
    const BAML_SRC_AFTER: &str = include_str!("test_data/test_after.baml");

    // compute the signature for the before and after
    let before_signature = compute_signature(BAML_SRC_BEFORE)?;
    let after_signature = compute_signature(BAML_SRC_AFTER)?;

    // assert that the signatures are different
    let before_tree_output = before_signature.0.print_tree(&before_signature.1);
    let after_tree_output = after_signature.0.print_tree(&after_signature.1);

    // Debug output to see the actual tree structures
    println!("Before Tree Output:\n{}", before_tree_output);
    println!("After Tree Output:\n{}", after_tree_output);

    let diff = lines(&before_tree_output, &after_tree_output);
    for change in diff {
        match change {
            diff::Result::Left(line) => println!("{}", format!("- {}", line).red()), // Lines only in before
            diff::Result::Right(line) => println!("{}", format!("+ {}", line).green()), // Lines only in after
            diff::Result::Both(_, _) => {} // Lines that are the same
        }
    }

    assert_eq!(before_tree_output, after_tree_output);

    Ok(())
}
