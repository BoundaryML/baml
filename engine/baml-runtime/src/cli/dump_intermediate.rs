use std::path::PathBuf;

use anyhow::Result;
use baml_compiler::{compile, hir::Hir};
use baml_vm::{EvalStack, Object, Value};
use clap::Parser;
use internal_baml_core::{
    internal_baml_diagnostics::SourceFile, ir::repr::IntermediateRepr, validate, ValidatedSchema,
};

use crate::baml_src_files;

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
    pub fn run(
        &self,
        dump_type: DumpType,
        feature_flags: internal_baml_core::feature_flags::FeatureFlags,
    ) -> Result<()> {
        // Parse and validate BAML files
        let validated_schema = self.load_and_validate_baml_files(feature_flags)?;

        match dump_type {
            DumpType::HIR => {
                eprintln!("=== HIGH-LEVEL INTERMEDIATE REPRESENTATION (HIR) ===");
                eprintln!("Source directory: {:?}", self.from);
                eprintln!();

                self.dump_hir(&validated_schema)?;
            }
            DumpType::Bytecode => {
                eprintln!("=== BYTECODE ===");
                eprintln!("Source directory: {:?}", self.from);
                eprintln!();

                self.dump_bytecode(&validated_schema)?;
            }
        }

        Ok(())
    }

    fn load_and_validate_baml_files(
        &self,
        feature_flags: internal_baml_core::feature_flags::FeatureFlags,
    ) -> Result<ValidatedSchema> {
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
        let validated_schema = validate(&self.from, source_files, feature_flags.clone());

        // Check for validation errors
        if validated_schema.diagnostics.has_errors() {
            eprintln!("Validation errors found:");
            for error in validated_schema.diagnostics.errors() {
                eprintln!("  {error:?}");
            }
            anyhow::bail!("Cannot generate HIR/bytecode due to validation errors");
        }

        // Display warnings if feature flag is set
        if feature_flags.should_display_warnings() && validated_schema.diagnostics.has_warnings() {
            eprintln!(
                "{}",
                validated_schema.diagnostics.warnings_to_pretty_string()
            );
        }

        Ok(validated_schema)
    }

    fn dump_hir(&self, validated_schema: &ValidatedSchema) -> Result<()> {
        // Convert to HIR (filtering is done in to_doc())
        let hir = Hir::from_ast(&validated_schema.db.ast);

        let mut w = Vec::new();
        hir.to_doc()
            .render(78, &mut w)
            .expect("Rendering should succeed");
        eprintln!(
            "{}",
            String::from_utf8(w).expect("UTF-8 conversion should succeed")
        );

        Ok(())
    }

    fn dump_bytecode(&self, validated_schema: &ValidatedSchema) -> Result<()> {
        let program = compile(&validated_schema.db)?;

        // Filter and display objects, excluding builtins
        for obj in &program.objects {
            match obj {
                Object::Function(f) => {
                    // Skip builtin functions (though we don't have any compiled as bytecode currently)
                    if baml_compiler::builtin::is_builtin_identifier(&f.name) {
                        continue;
                    }
                    eprintln!("{}", f.name);
                    eprintln!(
                        "{}",
                        baml_vm::debug::display_bytecode(
                            f,
                            &EvalStack::new(),
                            &program.objects,
                            &program.globals,
                            true
                        )
                    );
                }
                Object::Class(c) => {
                    if !baml_compiler::builtin::is_builtin_class(&c.name) {
                        eprintln!("Class: {} with {} fields", c.name, c.field_names.len());
                    }
                }
                Object::Enum(e) => {
                    if !baml_compiler::builtin::is_builtin_enum(&e.name) {
                        eprintln!("Enum {}", e.name);
                    }
                }
                _ => {
                    // Skip other object types (Instance, etc.)
                }
            }
        }

        Ok(())
    }
}
