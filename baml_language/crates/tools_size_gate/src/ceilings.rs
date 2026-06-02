//! Rewrite the absolute size ceilings in `.cargo/size-gate.toml` in place,
//! preserving the file's comments and structure via `toml_edit`.
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
    measure::ArtifactMeasurement,
};

/// Update every per-platform ceiling in `.cargo/size-gate.toml` to sit
/// `margin_pct` above the supplied measurements.
///
/// For each (platform, artifact) pair we write the ceiling for the
/// artifact's *gated* metric only: `max_file_bytes` for file-gated
/// artifacts (binaries) and `max_gzip_bytes` for gzip-gated ones (WASM).
/// Existing tables and comments are preserved; only the numeric value on
/// the ceiling key is replaced.
pub(crate) fn sync_ceilings(
    workspace_root: &Path,
    config: &Config,
    measurements: &BTreeMap<String, BTreeMap<String, ArtifactMeasurement>>,
    margin_pct: f64,
) -> Result<()> {
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
            eprintln!(
                "  ceiling {name} [{platform}] {key} = {ceiling} ({gated_bytes} + {margin_pct}%)"
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
        return Ok(());
    }
    std::fs::write(&config_path, rendered)
        .with_context(|| format!("failed to write config: {}", config_path.display()))?;
    eprintln!("updated {updates} ceiling(s) in {}", config_path.display());
    Ok(())
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
        table[key] = Item::Value(underscored_int(value));
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

/// An integer value rendered with `_` digit grouping (e.g. `15_519_843`)
/// to match the hand-written style of the rest of the config.
///
/// `toml_edit`'s repr setters are private, so we mint the formatted value by
/// parsing a one-line fragment, then normalise its decor to a single
/// leading space so it renders as `key = 15_519_843`.
fn underscored_int(n: u64) -> Value {
    let fragment = format!("x = {}", group_digits(n));
    let doc: DocumentMut = fragment.parse().expect("grouped integer is valid TOML");
    let mut v = doc["x"]
        .as_value()
        .expect("parsed fragment has a value")
        .clone();
    v.decor_mut().set_prefix(" ");
    v.decor_mut().set_suffix("");
    v
}

/// Insert `_` separators every three digits from the right.
fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let len = s.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push('_');
        }
        out.push(c);
    }
    out
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
    fn group_digits_inserts_separators() {
        assert_eq!(group_digits(15_519_843), "15_519_843");
        assert_eq!(group_digits(100), "100");
        assert_eq!(group_digits(1_000), "1_000");
    }
}
