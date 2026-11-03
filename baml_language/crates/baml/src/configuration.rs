use std::{fs, path::Path};

use anyhow::{Context, Result};

/// Read lazily: syntax is always checked, shapes only when consumed.
pub(super) fn read_document(path: &Path) -> Result<toml::Table> {
    match fs::read_to_string(path) {
        Ok(text) => {
            toml::from_str(&text).with_context(|| format!("invalid TOML in {}", path.display()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(toml::Table::new()),
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

pub(super) fn optional_string(
    document: &toml::Table,
    keys: &[&str],
    path: &Path,
) -> Result<Option<String>> {
    let mut table = document;
    for key in &keys[..keys.len() - 1] {
        let Some(value) = table.get(*key) else {
            return Ok(None);
        };
        table = value
            .as_table()
            .with_context(|| format!("{}: {key} must be a table", path.display()))?;
    }
    table
        .get(keys[keys.len() - 1])
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .with_context(|| format!("{}: {} must be a string", path.display(), keys.join(".")))
        })
        .transpose()
}

/// Validate every wrapper-owned field before modifying a project. Unknown
/// fields stay in the document, including comments retained by `toml_edit`.
pub(super) fn validate_project(text: &str) -> Result<()> {
    let document: toml::Table = toml::from_str(text).context("invalid baml.toml")?;
    let config: super::ProjectConfig =
        toml::from_str(text).context("invalid toolchain settings in baml.toml")?;
    let _: ProjectShape = toml::from_str(text).context("invalid project settings in baml.toml")?;
    if let Some(toolchain) = config.toolchain {
        if let Some(value) = toolchain.version {
            super::validate_version(&value)?;
        }
        if let Some(value) = toolchain.channel {
            anyhow::ensure!(
                super::is_channel(&value),
                "invalid toolchain.channel {value:?}"
            );
        }
        if let Some(value) = toolchain.path {
            anyhow::ensure!(!value.trim().is_empty(), "toolchain.path must not be empty");
        }
    }
    for (section, strings, booleans) in [
        ("package", &["name"][..], &["prelude"][..]),
        ("pack", &["telemetry_environment"][..], &[][..]),
        ("bridge", &["telemetry_environment"][..], &[][..]),
        ("boundary", &["project", "api_url"][..], &[][..]),
    ] {
        if let Some(value) = document.get(section) {
            let table = value
                .as_table()
                .with_context(|| format!("{section} must be a table"))?;
            for key in strings {
                if let Some(value) = table.get(*key) {
                    anyhow::ensure!(value.is_str(), "{section}.{key} must be a string");
                }
            }
            for key in booleans {
                if let Some(value) = table.get(*key) {
                    anyhow::ensure!(value.is_bool(), "{section}.{key} must be a boolean");
                }
            }
        }
    }
    Ok(())
}

// Mirrors the compiler-owned manifest shapes without pulling the compiler into
// the tiny Rust bootstrapper. Reads remain selective; writes check every field.
#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct ProjectShape {
    package: Option<PackageShape>,
    dependencies: Option<std::collections::BTreeMap<String, DependencyShape>>,
    scripts: Option<std::collections::BTreeMap<String, ScriptShape>>,
    generator: Option<std::collections::BTreeMap<String, GeneratorShape>>,
    test: Option<TestShape>,
}
#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct PackageShape {
    name: Option<String>,
    prelude: Option<bool>,
}
#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct DependencyShape {
    path: String,
}
#[derive(serde::Deserialize)]
#[serde(untagged)]
#[allow(dead_code)]
enum ScriptShape {
    Line(String),
    Tokens(Vec<String>),
}
#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct GeneratorShape {
    output_type: Option<String>,
    naming_convention: Option<String>,
    output_dir: Option<String>,
    sdk_import_path: Option<String>,
    max_typed_union_arity: Option<i64>,
}
#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct TestShape {
    default: Option<String>,
    profiles: Option<std::collections::BTreeMap<String, TestProfileShape>>,
}
#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct TestProfileShape {
    args: Option<Vec<String>>,
}
