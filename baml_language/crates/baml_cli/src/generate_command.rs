#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use baml_db::baml_compiler_diagnostics::{render, Severity};
use baml_compiler2_hir::{self, file_package, item_tree::GeneratorConfigItem};
use baml_project::ProjectDatabase;
use baml_workspace::discover_baml_files;
use clap::Args;

#[derive(Args, Clone, Debug)]
pub struct GenerateArgs {
    /// Path to the baml_src directory
    #[arg(long, default_value = ".")]
    pub from: PathBuf,

    /// Output directory override (takes precedence over generator config)
    #[arg(long, short = 'o')]
    pub output: Option<PathBuf>,
}

/// A parsed generator definition from a BAML source file.
struct GeneratorDef {
    name: String,
    output_type: String,
    /// Resolved output directory (absolute).
    output_dir: PathBuf,
}

impl GenerateArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let from = std::fs::canonicalize(&self.from)
            .with_context(|| format!("Could not resolve baml_src path: {}", self.from.display()))?;

        // Set up the compiler database and load all .baml files.
        let mut db = ProjectDatabase::new();
        let project = db.set_project_root(&from);
        let baml_files = discover_baml_files(&from);
        if baml_files.is_empty() {
            eprintln!("No .baml files found in {}", from.display());
            return Ok(crate::ExitCode::Other);
        }

        for file_path in &baml_files {
            let content = std::fs::read_to_string(file_path)
                .with_context(|| format!("Failed to read {}", file_path.display()))?;
            db.add_or_update_file(file_path, &content);
        }

        // Check for diagnostic errors.
        let source_files = db.get_source_files();
        let diagnostics = baml_project::collect_diagnostics(&db, project, &source_files);
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        if !errors.is_empty() {
            // Build source/path maps for ariadne rendering.
            let mut sources = HashMap::new();
            let mut file_paths = HashMap::new();
            for sf in &source_files {
                let file_id = sf.file_id(&db);
                sources.insert(file_id, sf.text(&db).to_string());
                file_paths.insert(file_id, sf.path(&db));
            }
            let rendered = render::render_diagnostics(
                &errors.iter().copied().cloned().collect::<Vec<_>>(),
                &sources,
                &file_paths,
                &render::RenderConfig::cli(),
            );
            eprintln!("{rendered}");
            return Ok(crate::ExitCode::Other);
        }

        // Discover generator definitions from the BAML source.
        let generators = discover_generators(&db, &from);

        if generators.is_empty() {
            eprintln!("No generator blocks found in BAML sources.");
            eprintln!("Add a generator block to your .baml files, e.g.:");
            eprintln!();
            eprintln!("  generator my_client {{");
            eprintln!("    output_type python/pydantic");
            eprintln!("    output_dir \"../baml_client\"");
            eprintln!("  }}");
            return Ok(crate::ExitCode::Other);
        }

        // Build the codegen ObjectPool from the compiler database.
        let pool = crate::codegen_bridge::build_object_pool(&db)
            .context("Failed to build codegen object pool")?;

        let mut total_files = 0;

        for generator in &generators {
            let output_dir = self.output.clone().unwrap_or_else(|| generator.output_dir.clone());

            let generated = match generator.output_type.as_str() {
                "python/pydantic" | "python/pydantic/v1" => {
                    baml_codegen_python::to_source_code(&pool, &from)
                }
                other => {
                    eprintln!(
                        "Skipping generator '{}': unsupported output_type '{other}'",
                        generator.name
                    );
                    continue;
                }
            };

            std::fs::create_dir_all(&output_dir).with_context(|| {
                format!(
                    "Failed to create output directory: {}",
                    output_dir.display()
                )
            })?;

            let mut count = 0;
            for (rel_path, content) in &generated {
                let dest = output_dir.join(rel_path);
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&dest, content)
                    .with_context(|| format!("Failed to write {}", dest.display()))?;
                count += 1;
            }

            // Write __init__.py if it doesn't exist (Python only).
            if generator.output_type.starts_with("python") {
                let init_path = output_dir.join("__init__.py");
                if !init_path.exists() {
                    std::fs::write(&init_path, "# Generated by baml-cli\n")
                        .with_context(|| format!("Failed to write {}", init_path.display()))?;
                    count += 1;
                }
            }

            println!(
                "Generator '{}': wrote {count} files to {}",
                generator.name,
                output_dir.display()
            );
            total_files += count;
        }

        if total_files == 0 {
            eprintln!("No files generated (no supported generators found).");
            return Ok(crate::ExitCode::Other);
        }

        Ok(crate::ExitCode::Success)
    }
}

/// Walk the HIR item trees to discover all `generator` blocks and their config.
fn discover_generators(db: &ProjectDatabase, baml_src: &std::path::Path) -> Vec<GeneratorDef> {
    let mut generators = Vec::new();

    for source_file in db.get_source_files() {
        let pkg_info = file_package::file_package(db, source_file);
        if pkg_info.package.as_str() != "user" {
            continue;
        }

        let item_tree = baml_compiler2_hir::file_item_tree(db, source_file);

        for (_id, generator_item) in &item_tree.generators {
            let config = &generator_item.config_items;
            let output_type = get_config(config, "output_type").unwrap_or_default();
            if output_type.is_empty() {
                eprintln!(
                    "Warning: generator '{}' missing 'output_type', skipping",
                    generator_item.name
                );
                continue;
            }

            // output_dir is relative to baml_src, defaults to "../"
            let raw_output_dir = get_config(config, "output_dir")
                .unwrap_or_else(|| "..".to_string());
            // Strip surrounding quotes if present (config values may be quoted strings)
            let raw_output_dir = raw_output_dir
                .trim_matches('"')
                .trim_matches('\'');

            let output_dir = baml_src.join(raw_output_dir).join("baml_client");

            generators.push(GeneratorDef {
                name: generator_item.name.to_string(),
                output_type,
                output_dir,
            });
        }
    }

    generators
}

/// Look up a config key in a generator's config items.
fn get_config(items: &[GeneratorConfigItem], key: &str) -> Option<String> {
    items
        .iter()
        .find(|item| item.key.as_str() == key)
        .map(|item| item.value.clone())
}
