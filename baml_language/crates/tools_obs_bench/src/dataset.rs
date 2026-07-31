use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::{Map, Value};

pub(crate) const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;
pub(crate) const MAX_ROWS: usize = 100_000;

#[derive(Clone, Debug)]
pub(crate) struct BenchRow {
    pub(crate) source: PathBuf,
    pub(crate) line: usize,
    pub(crate) value: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct ValidationSummary {
    pub(crate) schema_version: u32,
    pub(crate) files: usize,
    pub(crate) benchmark_rows: usize,
    pub(crate) manifest_rows: usize,
}

impl BenchRow {
    pub(crate) fn object(&self) -> Result<&Map<String, Value>> {
        self.value.as_object().ok_or_else(|| {
            anyhow::anyhow!(
                "{}:{}: benchmark row must be a JSON object",
                self.source.display(),
                self.line
            )
        })
    }

    pub(crate) fn bench_id(&self) -> Option<&str> {
        self.value.get("bench_id").and_then(Value::as_str)
    }

    pub(crate) fn metric(&self, name: &str) -> Option<f64> {
        self.value.get(name).and_then(Value::as_f64)
    }

    pub(crate) fn evidence(&self) -> Option<&str> {
        self.value.get("evidence").and_then(Value::as_str)
    }

    pub(crate) fn is_manifest(&self) -> bool {
        self.value.get("type").and_then(Value::as_str) == Some("machine")
            || (self.value.get("os").is_some()
                && self.value.get("arch").is_some()
                && self.bench_id().is_none())
    }
}

pub(crate) fn load(paths: &[PathBuf]) -> Result<Vec<BenchRow>> {
    if paths.is_empty() {
        bail!("at least one benchmark row file is required");
    }
    let mut rows = Vec::new();
    for path in paths {
        let metadata = fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
        if metadata.len() > MAX_INPUT_BYTES {
            bail!(
                "{} is {} bytes; the per-file limit is {MAX_INPUT_BYTES}",
                path.display(),
                metadata.len()
            );
        }
        let source = fs::read_to_string(path)
            .with_context(|| format!("read benchmark rows from {}", path.display()))?;
        let trimmed = source.trim();
        if trimmed.is_empty() {
            bail!("{} contains no benchmark rows", path.display());
        }
        if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
            match value {
                Value::Array(values) => {
                    for (index, value) in values.into_iter().enumerate() {
                        rows.push(BenchRow {
                            source: path.clone(),
                            line: index + 1,
                            value,
                        });
                    }
                }
                value => rows.push(BenchRow {
                    source: path.clone(),
                    line: 1,
                    value,
                }),
            }
        } else {
            for (index, line) in source.lines().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }
                let value = serde_json::from_str::<Value>(line)
                    .with_context(|| format!("parse NDJSON {}:{}", path.display(), index + 1))?;
                rows.push(BenchRow {
                    source: path.clone(),
                    line: index + 1,
                    value,
                });
            }
        }
        if rows.len() > MAX_ROWS {
            bail!("benchmark row count exceeds bounded limit {MAX_ROWS}");
        }
    }
    Ok(rows)
}

pub(crate) fn validate(paths: &[PathBuf]) -> Result<ValidationSummary> {
    let rows = load(paths)?;
    let mut benchmark_rows = 0;
    let mut manifest_rows = 0;
    for row in &rows {
        let object = row.object()?;
        let schema = object.get("schema_version").and_then(Value::as_u64);
        if schema != Some(1) {
            bail!(
                "{}:{}: unsupported or missing schema_version",
                row.source.display(),
                row.line
            );
        }
        if row.is_manifest() {
            manifest_rows += 1;
            continue;
        }
        let Some(bench_id) = row.bench_id() else {
            bail!(
                "{}:{}: row has neither bench_id nor machine-manifest shape",
                row.source.display(),
                row.line
            );
        };
        if bench_id.is_empty() || bench_id.len() > 160 {
            bail!(
                "{}:{}: bench_id must contain 1..=160 bytes",
                row.source.display(),
                row.line
            );
        }
        benchmark_rows += 1;
    }
    Ok(ValidationSummary {
        schema_version: 1,
        files: paths.len(),
        benchmark_rows,
        manifest_rows,
    })
}

pub(crate) fn platform_label() -> String {
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        other => other,
    };
    format!("{}-{arch}", std::env::consts::OS)
}

pub(crate) fn default_baseline_path(manifest_dir: &Path, platform: &str) -> PathBuf {
    manifest_dir
        .join("baselines")
        .join(format!("{platform}.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str, body: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("obs-bench-dataset-{name}-{}", std::process::id()));
        fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn loads_ndjson_and_json_arrays() {
        let ndjson = temp_file(
            "rows.ndjson",
            "{\"schema_version\":1,\"bench_id\":\"a\",\"elapsed_ns\":1}\n",
        );
        let array = temp_file(
            "rows.json",
            "[{\"schema_version\":1,\"bench_id\":\"b\",\"elapsed_ns\":2}]",
        );
        let rows = load(&[ndjson.clone(), array.clone()]).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].bench_id(), Some("a"));
        assert_eq!(rows[1].metric("elapsed_ns"), Some(2.0));
        fs::remove_file(ndjson).unwrap();
        fs::remove_file(array).unwrap();
    }
}
