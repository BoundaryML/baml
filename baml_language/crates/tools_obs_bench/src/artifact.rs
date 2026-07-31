use std::{
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use bex_events::{
    prof::{
        read::read_bamlprof_from_bytes,
        storage::{SegmentState, scan_bcct_bytes},
    },
    value::read_bamlvalue_from_bytes,
};
use serde::Serialize;

use crate::dataset;

const MAX_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ARTIFACTS: usize = 10_000;
const MAX_FUZZ_ITERATIONS: u32 = 10_000;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ArtifactKind {
    Prof,
    Value,
    Bcct,
    BenchmarkRows,
    Manifest,
}

#[derive(Debug, Serialize)]
pub(crate) struct ValidationRow {
    schema_version: u32,
    path: String,
    kind: ArtifactKind,
    bytes: u64,
    valid: bool,
    truncated: Option<bool>,
    records: Option<usize>,
    detail: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct CrashFuzzRow {
    schema_version: u32,
    path: String,
    kind: ArtifactKind,
    seed: u64,
    iterations: u32,
    accepted_prefixes: u32,
    rejected_prefixes: u32,
    panics: u32,
}

pub(crate) fn validate(paths: &[PathBuf]) -> Result<Vec<ValidationRow>> {
    let files = expand_paths(paths)?;
    files.iter().map(|path| validate_one(path)).collect()
}

pub(crate) fn crashfuzz(
    paths: &[PathBuf],
    iterations: u32,
    seed: u64,
) -> Result<Vec<CrashFuzzRow>> {
    if iterations == 0 || iterations > MAX_FUZZ_ITERATIONS {
        bail!("iterations must be in 1..={MAX_FUZZ_ITERATIONS}");
    }
    let files = expand_paths(paths)?;
    let mut rows = Vec::new();
    for path in files {
        let kind = kind_for(&path)?;
        if matches!(kind, ArtifactKind::BenchmarkRows | ArtifactKind::Manifest) {
            bail!("crashfuzz expects binary observability artifacts, not JSON files");
        }
        let bytes = bounded_read(&path)?;
        decode_binary(kind, &bytes)
            .with_context(|| format!("validate original artifact {}", path.display()))?;
        let mut rng = seed ^ stable_hash(path.as_os_str().as_encoded_bytes());
        let mut accepted_prefixes = 0;
        let mut rejected_prefixes = 0;
        let mut panics = 0;
        for iteration in 0..iterations {
            rng = splitmix64(rng.wrapping_add(u64::from(iteration)));
            let len = if bytes.is_empty() {
                0
            } else {
                usize::try_from(rng % (bytes.len() as u64 + 1)).unwrap_or(0)
            };
            match catch_unwind(AssertUnwindSafe(|| decode_binary(kind, &bytes[..len]))) {
                Ok(Ok(())) => accepted_prefixes += 1,
                Ok(Err(_)) => rejected_prefixes += 1,
                Err(_) => panics += 1,
            }
        }
        if panics != 0 {
            bail!(
                "{} panicked while opening {panics} crash prefixes",
                path.display()
            );
        }
        rows.push(CrashFuzzRow {
            schema_version: 1,
            path: path.display().to_string(),
            kind,
            seed,
            iterations,
            accepted_prefixes,
            rejected_prefixes,
            panics,
        });
    }
    Ok(rows)
}

fn validate_one(path: &Path) -> Result<ValidationRow> {
    let kind = kind_for(path)?;
    let bytes = fs::metadata(path)?.len();
    match kind {
        ArtifactKind::Prof => {
            let contents = read_bamlprof_from_bytes(&bounded_read(path)?)
                .with_context(|| format!("decode {}", path.display()))?;
            Ok(ValidationRow {
                schema_version: 1,
                path: path.display().to_string(),
                kind,
                bytes,
                valid: true,
                truncated: Some(contents.truncated),
                records: Some(contents.events.len()),
                detail: "legacy profile decoded".to_owned(),
            })
        }
        ArtifactKind::Value => {
            let contents = read_bamlvalue_from_bytes(&bounded_read(path)?)
                .with_context(|| format!("decode {}", path.display()))?;
            Ok(ValidationRow {
                schema_version: 1,
                path: path.display().to_string(),
                kind,
                bytes,
                valid: true,
                truncated: Some(contents.truncated),
                records: Some(contents.records.len()),
                detail: "value artifact decoded".to_owned(),
            })
        }
        ArtifactKind::Bcct => {
            let contents = scan_bcct_bytes(&bounded_read(path)?)
                .with_context(|| format!("scan {}", path.display()))?;
            Ok(ValidationRow {
                schema_version: 1,
                path: path.display().to_string(),
                kind,
                bytes,
                valid: true,
                truncated: Some(matches!(contents.state, SegmentState::Torn)),
                records: Some(contents.blocks.len()),
                detail: if matches!(contents.state, SegmentState::Sealed(_)) {
                    "sealed BCCT container".to_owned()
                } else {
                    "open or recovered BCCT container".to_owned()
                },
            })
        }
        ArtifactKind::BenchmarkRows => {
            let summary = dataset::validate(&[path.to_path_buf()])?;
            Ok(ValidationRow {
                schema_version: 1,
                path: path.display().to_string(),
                kind,
                bytes,
                valid: true,
                truncated: None,
                records: Some(summary.benchmark_rows + summary.manifest_rows),
                detail: "benchmark row schema valid".to_owned(),
            })
        }
        ArtifactKind::Manifest => {
            let value = serde_json::from_slice::<serde_json::Value>(&bounded_read(path)?)
                .with_context(|| format!("parse {}", path.display()))?;
            if value
                .get("schema_version")
                .and_then(serde_json::Value::as_u64)
                != Some(1)
                || !value
                    .get("artifacts")
                    .is_some_and(serde_json::Value::is_array)
            {
                bail!(
                    "{} is not a schema-version-1 corpus manifest",
                    path.display()
                );
            }
            Ok(ValidationRow {
                schema_version: 1,
                path: path.display().to_string(),
                kind,
                bytes,
                valid: true,
                truncated: None,
                records: value
                    .get("artifacts")
                    .and_then(serde_json::Value::as_array)
                    .map(Vec::len),
                detail: "corpus manifest schema valid".to_owned(),
            })
        }
    }
}

fn decode_binary(kind: ArtifactKind, bytes: &[u8]) -> Result<()> {
    match kind {
        ArtifactKind::Prof => {
            read_bamlprof_from_bytes(bytes)?;
        }
        ArtifactKind::Value => {
            read_bamlvalue_from_bytes(bytes)?;
        }
        ArtifactKind::Bcct => {
            scan_bcct_bytes(bytes)?;
        }
        ArtifactKind::BenchmarkRows | ArtifactKind::Manifest => {
            bail!("JSON files are not binary artifacts")
        }
    }
    Ok(())
}

pub(crate) fn expand_paths(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    if paths.is_empty() {
        bail!("at least one artifact path is required");
    }
    let mut output = Vec::new();
    for path in paths {
        visit(path, 0, &mut output)?;
    }
    output.sort();
    output.dedup();
    if output.len() > MAX_ARTIFACTS {
        bail!("artifact count exceeds bounded limit {MAX_ARTIFACTS}");
    }
    if output.is_empty() {
        bail!("no supported observability artifacts found");
    }
    Ok(output)
}

fn visit(path: &Path, depth: usize, output: &mut Vec<PathBuf>) -> Result<()> {
    if depth > 8 {
        bail!("directory traversal depth exceeds 8 at {}", path.display());
    }
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("stat {}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_file() {
        if kind_for(path).is_ok() {
            output.push(path.to_path_buf());
        }
        return Ok(());
    }
    if !metadata.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(path).with_context(|| format!("read {}", path.display()))? {
        visit(&entry?.path(), depth + 1, output)?;
        if output.len() > MAX_ARTIFACTS {
            bail!("artifact count exceeds bounded limit {MAX_ARTIFACTS}");
        }
    }
    Ok(())
}

pub(crate) fn kind_for(path: &Path) -> Result<ArtifactKind> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if name == "manifest.json" {
        Ok(ArtifactKind::Manifest)
    } else if name.ends_with(".bamlprof") {
        Ok(ArtifactKind::Prof)
    } else if name.ends_with(".bamlvalue") {
        Ok(ArtifactKind::Value)
    } else if name.ends_with(".bamlcct") || name.ends_with(".bamlbcct") {
        Ok(ArtifactKind::Bcct)
    } else if name.ends_with(".ndjson") || name.ends_with(".json") {
        Ok(ArtifactKind::BenchmarkRows)
    } else {
        bail!("unsupported artifact type: {}", path.display())
    }
}

fn bounded_read(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_ARTIFACT_BYTES {
        bail!(
            "{} is {} bytes; validation limit is {MAX_ARTIFACT_BYTES}",
            path.display(),
            metadata.len()
        );
    }
    fs::read(path).with_context(|| format!("read {}", path.display()))
}

fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
