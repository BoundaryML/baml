//! Rewrite the absolute size ceilings in `.cargo/size-gate.toml` in place,
//! preserving the file's structure via `toml_edit`.
//!
//! This is the write-side counterpart to the per-platform `max_file_bytes`
//! / `max_gzip_bytes` ceilings that `compare.rs` reads. The daily baseline
//! refresh (`bake`) calls this so that the absolute ceiling tracks the
//! freshly recorded baseline rather than drifting away from it.

use std::{collections::BTreeMap, path::Path};

use anyhow::{Context, Result};
use toml_edit::{DocumentMut, Item, Table, Value};

use crate::{
    config::{Config, GateMetric},
    human_size,
    measure::ArtifactMeasurement,
};

/// Update every per-platform ceiling in `.cargo/size-gate.toml` to sit
/// `margin_pct` above the supplied measurements.
///
/// For each (platform, artifact) pair we write the ceiling for the
/// artifact's *gated* metric only: `max_file_bytes` for file-gated
/// artifacts (binaries) and `max_gzip_bytes` for gzip-gated ones (WASM).
/// Existing tables are preserved; only the value on
/// the ceiling key is replaced.
///
/// Returns `true` if the config file was changed on disk.
pub(crate) fn sync_ceilings(
    workspace_root: &Path,
    config: &Config,
    measurements: &BTreeMap<String, BTreeMap<String, ArtifactMeasurement>>,
    margin_pct: f64,
) -> Result<bool> {
    let config_path = workspace_root.join(".cargo/size-gate.toml");
    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read config: {}", config_path.display()))?;
    let mut doc: DocumentMut = content
        .parse()
        .with_context(|| format!("failed to parse {} for editing", config_path.display()))?;

    let mut updates = 0u32;
    for (platform, artifacts) in measurements {
        for (name, measurement) in artifacts {
            // Only touch artifacts the config actually knows about; the
            // gate metric decides which ceiling key we write.
            let Some(artifact_config) = config.artifacts.get(name) else {
                eprintln!("  skipping ceiling for unknown artifact `{name}`");
                continue;
            };
            let (key, gated_bytes) = match artifact_config.gate {
                GateMetric::File => ("max_file_bytes", measurement.file_bytes),
                GateMetric::Gzip => ("max_gzip_bytes", measurement.gzip_bytes),
            };
            let ceiling = ceiling_for(gated_bytes, margin_pct);
            set_ceiling(&mut doc, name, platform, key, ceiling);
            updates += 1;
            let formatted_ceiling = human_size::format(ceiling);
            eprintln!(
                "  ceiling {name} [{platform}] {key} = {formatted_ceiling} ({gated_bytes} + {margin_pct}%)"
            );
        }
    }

    // Only write when the rendered config actually differs. Rewriting the
    // identical bytes would be harmless for git, but skipping keeps the log
    // honest ("nothing changed" → no baseline-refresh PR) and avoids a
    // needless mtime bump.
    let rendered = doc.to_string();
    if rendered == content {
        eprintln!("ceilings unchanged in {}", config_path.display());
        return Ok(false);
    }
    std::fs::write(&config_path, rendered)
        .with_context(|| format!("failed to write config: {}", config_path.display()))?;
    eprintln!("updated {updates} ceiling(s) in {}", config_path.display());
    Ok(true)
}

/// `bytes` grown by `margin_pct`, rounded up to the next whole byte.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a byte ceiling fits u64; margin_pct is small and non-negative in practice"
)]
fn ceiling_for(bytes: u64, margin_pct: f64) -> u64 {
    let scaled = (bytes as f64) * (1.0 + margin_pct / 100.0);
    scaled.ceil() as u64
}

/// Set `artifacts.<name>.platform.<platform>.<key> = value`, creating any
/// intermediate tables that don't already exist. Existing tables (and the
/// comments attached to them) are left untouched.
fn set_ceiling(doc: &mut DocumentMut, name: &str, platform: &str, key: &str, value: u64) {
    let platform_table = doc
        .as_table_mut()
        .entry("artifacts")
        .or_insert_with(implicit_table)
        .as_table_mut()
        .and_then(|t| t.entry(name).or_insert_with(implicit_table).as_table_mut())
        .and_then(|t| {
            t.entry("platform")
                .or_insert_with(implicit_table)
                .as_table_mut()
        })
        .and_then(|t| {
            t.entry(platform)
                .or_insert_with(implicit_table)
                .as_table_mut()
        });

    if let Some(table) = platform_table {
        table[key] = Item::Value(human_readable_size(value));
    }
}

/// A fresh empty table marked implicit so we never emit a redundant
/// `[artifacts]` / `[artifacts.x.platform]` header for a table that only
/// exists to hold a deeper one.
fn implicit_table() -> Item {
    let mut t = Table::new();
    t.set_implicit(true);
    Item::Table(t)
}

fn human_readable_size(bytes: u64) -> Value {
    let mut v = Value::from(human_size::format(bytes));
    v.decor_mut().set_prefix(" ");
    v.decor_mut().set_suffix("");
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ceiling_rounds_up() {
        assert_eq!(ceiling_for(1_000_000, 3.0), 1_030_000);
        // Round up rather than truncate.
        assert_eq!(ceiling_for(1_000_001, 3.0), 1_030_002);
        assert_eq!(ceiling_for(0, 3.0), 0);
    }

    #[test]
    fn set_ceiling_writes_a_human_readable_toml_string() {
        let mut doc = DocumentMut::new();
        set_ceiling(
            &mut doc,
            "baml-cli",
            "aarch64-apple-darwin",
            "max_file_bytes",
            21_956_782,
        );
        assert_eq!(
            doc.to_string(),
            "[artifacts.baml-cli.platform.aarch64-apple-darwin]\nmax_file_bytes = \"20.9 MiB\"\n"
        );
    }
}
