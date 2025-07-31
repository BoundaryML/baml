use anyhow::Result;
use baml_compiler::hir::Program;
use clap::Parser;
use std::path::PathBuf;

use crate::baml_src_files;
use internal_baml_core::{
    internal_baml_diagnostics::SourceFile, ir::repr::IntermediateRepr, validate, ValidatedSchema,
};

#[derive(Parser, Debug)]
pub struct DumpIntermediateArgs {
    /// Path to BAML source directory
    #[arg(long = "from")]
    pub from: PathBuf,
}

pub enum DumpType {
    HIR,
    Bytecode,
}

impl DumpIntermediateArgs {
    pub fn run(&self, dump_type: DumpType) -> Result<()> {
        // Parse and validate BAML files
        let validated_schema = self.load_and_validate_baml_files()?;

        match dump_type {
            DumpType::HIR => {
                println!("=== HIGH-LEVEL INTERMEDIATE REPRESENTATION (HIR) ===");
                println!("Source directory: {:?}", self.from);
                println!();

                self.dump_hir(&validated_schema)?;
            }
            DumpType::Bytecode => {
                println!("=== BYTECODE ===");
                println!("Source directory: {:?}", self.from);
                println!();

                self.dump_bytecode(&validated_schema)?;
            }
        }

        Ok(())
    }

    fn load_and_validate_baml_files(&self) -> Result<ValidatedSchema> {
        // Get all BAML files from the directory
        let files = baml_src_files(&self.from)?;

        // Read file contents
        let source_files: Vec<SourceFile> = files
            .into_iter()
            .map(|path| {
                let contents = std::fs::read_to_string(&path)?;
                Ok(SourceFile::from((path, contents)))
            })
            .collect::<Result<Vec<_>>>()?;

        // Validate the files
        let validated_schema = validate(&self.from, source_files);

        // Check for validation errors
        if validated_schema.diagnostics.has_errors() {
            eprintln!("Validation errors found:");
            for error in validated_schema.diagnostics.errors() {
                eprintln!("  {:?}", error);
            }
            anyhow::bail!("Cannot generate HIR/bytecode due to validation errors");
        }

        Ok(validated_schema)
    }

    fn dump_hir(&self, validated_schema: &ValidatedSchema) -> Result<()> {
        // Convert to HIR
        let hir = Program::from_ast(&validated_schema.db.ast);
        let mut w = Vec::new();
        hir.to_doc()
            .render(78, &mut w)
            .expect("Rendering should succeed");
        println!(
            "{}",
            String::from_utf8(w).expect("UTF-8 conversion should succeed")
        );

        Ok(())
    }

    fn dump_bytecode(&self, validated_schema: &ValidatedSchema) -> Result<()> {
        println!("Bytecode compilation is not yet fully implemented.");
        println!("For now, showing the parsed structure:");
        println!();

        self.dump_hir(validated_schema)
    }
}
