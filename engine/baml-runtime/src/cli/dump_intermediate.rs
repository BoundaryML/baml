use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

use crate::baml_src_files;
use internal_baml_core::{ir::IntermediateRepr, validate, SourceFile, ValidatedSchema};

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
                eprintln!("  {}", error);
            }
            anyhow::bail!("Cannot generate HIR/bytecode due to validation errors");
        }

        Ok(validated_schema)
    }

    fn dump_hir(&self, validated_schema: &ValidatedSchema) -> Result<()> {
        // Convert to HIR
        let ir = IntermediateRepr::from_parser_database(
            &validated_schema.db,
            validated_schema.configuration.clone(),
        )?;

        // Pretty-print the HIR
        println!("Enums ({}):", ir.enums.len());
        for enum_node in &ir.enums {
            println!("  enum {} {{", enum_node.elem.name);
            for variant in &enum_node.elem.values {
                println!("    {}", variant.elem.0);
            }
            println!("  }}");
        }
        println!();

        println!("Classes ({}):", ir.classes.len());
        for class_node in &ir.classes {
            println!("  class {} {{", class_node.elem.name);
            for field in &class_node.elem.fields {
                println!("    {}: {}", field.elem.name, field.elem.r#type);
            }
            println!("  }}");
        }
        println!();

        println!("Functions ({}):", ir.functions.len());
        for func_node in &ir.functions {
            print!("  function {}(", func_node.elem.name);
            for (i, param) in func_node.elem.params.iter().enumerate() {
                if i > 0 {
                    print!(", ");
                }
                print!("{}: {}", param.elem.name, param.elem.r#type);
            }
            println!(") -> {} {{", func_node.elem.output_type);
            println!(
                "    // client: {}",
                func_node.elem.client_spec.elem.client_name
            );
            println!("  }}");
        }
        println!();

        println!("Expression Functions ({}):", ir.expr_fns.len());
        for expr_fn_node in &ir.expr_fns {
            print!("  expr function {}(", expr_fn_node.elem.name);
            for (i, param) in expr_fn_node.elem.params.iter().enumerate() {
                if i > 0 {
                    print!(", ");
                }
                print!("{}: {}", param.elem.name, param.elem.r#type);
            }
            println!(") -> {} {{", expr_fn_node.elem.output_type);
            println!("    {:?}", expr_fn_node.elem.expr);
            println!("  }}");
        }
        println!();

        println!("Clients ({}):", ir.clients.len());
        for client_node in &ir.clients {
            println!(
                "  client<{}> {}",
                client_node.elem.provider, client_node.elem.name
            );
        }

        Ok(())
    }

    fn dump_bytecode(&self, validated_schema: &ValidatedSchema) -> Result<()> {
        println!("Bytecode compilation is not yet fully implemented.");
        println!("For now, showing the parsed structure:");
        println!();

        self.dump_hir(validated_schema)?;

        Ok(())
    }
}
