use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use bex_events::prof::storage::{
    BcctHeader, BcctWriter, BlockRows, CctDeltaRow, ClockDescriptor, NodeBirthRow, SegmentState,
    scan_bcct_bytes,
};
use serde::Serialize;

use crate::artifact::{self, ArtifactKind};

const MAX_LOGICAL_BYTES_PER_MODE: u64 = 10 * 1024 * 1024 * 1024;
const MAX_TEMPLATE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_ENCODED_TEMPLATE_BYTES: u64 = 9 * 1024 * 1024;
const MAX_SCAN_BYTES: u64 = 64 * 1024 * 1024;
const MAX_SYNTH_NODES: u32 = 4096;
const MAX_SYNTH_ARTIFACTS: usize = 10_000;

#[derive(Debug, Serialize)]
pub(crate) struct CorpusScan {
    schema_version: u32,
    root_count: usize,
    files: usize,
    bytes: u64,
    by_kind: BTreeMap<String, usize>,
    sealed_bcct: usize,
    recovered_bcct: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct CorpusSynth {
    schema_version: u32,
    output: String,
    seed: u64,
    target_bytes: u64,
    logical_bytes: u64,
    generated_bytes: u64,
    physical_template_bytes: u64,
    nodes: u32,
    artifacts: Vec<SynthArtifact>,
}

#[derive(Debug, Serialize)]
struct SynthArtifact {
    path: String,
    mode: &'static str,
    bytes: u64,
    blocks: usize,
    template: bool,
}

pub(crate) fn scan(paths: &[PathBuf]) -> Result<CorpusScan> {
    let files = artifact::expand_paths(paths)?;
    let mut bytes = 0_u64;
    let mut by_kind = BTreeMap::new();
    let mut sealed_bcct = 0;
    let mut recovered_bcct = 0;
    for path in &files {
        bytes = bytes.saturating_add(fs::metadata(path)?.len());
        let kind = artifact::kind_for(path)?;
        *by_kind
            .entry(format!("{kind:?}").to_lowercase())
            .or_default() += 1;
        if matches!(kind, ArtifactKind::Bcct) && fs::metadata(path)?.len() <= MAX_SCAN_BYTES {
            let scan = scan_bcct_bytes(&fs::read(path)?)?;
            if matches!(scan.state, SegmentState::Sealed(_)) {
                sealed_bcct += 1;
            } else {
                recovered_bcct += 1;
            }
        }
    }
    Ok(CorpusScan {
        schema_version: 1,
        root_count: paths.len(),
        files: files.len(),
        bytes,
        by_kind,
        sealed_bcct,
        recovered_bcct,
    })
}

pub(crate) fn synth(
    output: &Path,
    target_bytes: u64,
    nodes: u32,
    seed: u64,
) -> Result<CorpusSynth> {
    if !(4096..=MAX_LOGICAL_BYTES_PER_MODE).contains(&target_bytes) {
        bail!("target_bytes must be in 4096..={MAX_LOGICAL_BYTES_PER_MODE} (10 GiB per mode)");
    }
    if nodes == 0 || nodes > MAX_SYNTH_NODES {
        bail!("nodes must be in 1..={MAX_SYNTH_NODES}");
    }
    fs::create_dir_all(output).with_context(|| format!("create {}", output.display()))?;
    if fs::read_dir(output)?.next().is_some() {
        bail!(
            "{} must be empty so a corpus cannot mix shard generations",
            output.display()
        );
    }
    let mut artifacts = Vec::new();
    let mut generated_bytes = 0_u64;
    let mut physical_template_bytes = 0_u64;
    for full_trace in [false, true] {
        let template_target = target_bytes.min(MAX_TEMPLATE_BYTES);
        let (bytes, blocks) = synth_bcct(template_target, nodes, seed, full_trace)?;
        let template_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if template_bytes > MAX_ENCODED_TEMPLATE_BYTES {
            bail!(
                "encoded corpus template exceeds bounded limit {MAX_ENCODED_TEMPLATE_BYTES} bytes"
            );
        }
        let shard_count_u64 = target_bytes.div_ceil(template_bytes);
        let shard_count = usize::try_from(shard_count_u64)
            .context("synthetic corpus shard count overflows usize")?;
        if artifacts.len().saturating_add(shard_count) > MAX_SYNTH_ARTIFACTS {
            bail!("synthetic corpus exceeds {MAX_SYNTH_ARTIFACTS} artifact paths");
        }
        let (name, stem, extension, mode) = if full_trace {
            (
                "full-trace.bamlbcct",
                "full-trace",
                "bamlbcct",
                "full_trace",
            )
        } else {
            ("cct-only.bamlcct", "cct-only", "bamlcct", "cct_only")
        };
        let template_path = output.join(name);
        fs::write(&template_path, &bytes)
            .with_context(|| format!("write {}", template_path.display()))?;
        for shard in 0..shard_count {
            let path = if shard == 0 {
                template_path.clone()
            } else {
                output.join(format!("{stem}-{shard:05}.{extension}"))
            };
            if shard != 0 {
                fs::hard_link(&template_path, &path).with_context(|| {
                    format!(
                        "hard-link corpus shard {} from {}; the output filesystem must support hard links",
                        path.display(),
                        template_path.display()
                    )
                })?;
            }
            artifacts.push(SynthArtifact {
                path: path.display().to_string(),
                mode,
                bytes: template_bytes,
                blocks,
                template: shard == 0,
            });
        }
        generated_bytes =
            generated_bytes.saturating_add(template_bytes.saturating_mul(shard_count_u64));
        physical_template_bytes = physical_template_bytes.saturating_add(template_bytes);
    }
    let manifest = CorpusSynth {
        schema_version: 1,
        output: output.display().to_string(),
        seed,
        target_bytes,
        logical_bytes: target_bytes.saturating_mul(2),
        generated_bytes,
        physical_template_bytes,
        nodes,
        artifacts,
    };
    fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(manifest)
}

fn synth_bcct(
    target_bytes: u64,
    nodes: u32,
    seed: u64,
    full_trace: bool,
) -> Result<(Vec<u8>, usize)> {
    let mut process_euid = [0_u8; 16];
    process_euid[..8].copy_from_slice(&seed.to_le_bytes());
    process_euid[8..].copy_from_slice(&seed.to_le_bytes());
    let mut writer = BcctWriter::create(
        Vec::with_capacity(usize::try_from(target_bytes).unwrap_or(0)),
        &BcctHeader {
            process_euid,
            engine_id: seed,
            session_seg_seq: 1,
            started_epoch_ns: 1,
            clock: ClockDescriptor {
                kind: 1,
                quality: 1,
                tick_ns_numer: 1,
                tick_ns_denom: 1,
            },
            revision_id: [u8::try_from(seed & 0xff).unwrap_or(0); 32],
        },
    )?;
    let births = (1..=nodes)
        .map(|node_id| NodeBirthRow {
            node_id,
            parent_node_id: node_id.saturating_sub(1),
            function_id: 16 + node_id,
            logical_thread_id: 1 + u64::from(node_id % 86),
            partition_id: 1,
        })
        .collect();
    writer.append(&BlockRows::NodeBirth(births), 0, 0)?;
    let row_count = nodes.min(256);
    let rows = (1..=row_count)
        .map(|node_id| CctDeltaRow {
            node_id,
            enters: 1,
            ends_ok: 1,
            total_ns: u64::from(node_id) * 10,
            self_ns: u64::from(node_id) * 7,
            await_ns: u64::from(node_id) * 3,
            ..CctDeltaRow::default()
        })
        .collect::<Vec<_>>();
    let mut blocks = 1;
    while writer.bytes_written() < target_bytes.saturating_sub(4096) && blocks < 16_384 {
        let timestamp = u64::try_from(blocks).unwrap_or(u64::MAX) * 250_000_000;
        writer.append(&BlockRows::CctDelta(rows.clone()), timestamp, timestamp)?;
        blocks += 1;
        if full_trace {
            let mut raw = vec![0_u8; 2048];
            fill_deterministic(&mut raw, seed ^ timestamp);
            writer.append(&BlockRows::Reserved7(raw), timestamp, timestamp)?;
            blocks += 1;
        }
    }
    writer.seal()?;
    let bytes = writer.into_inner();
    scan_bcct_bytes(&bytes)?;
    Ok((bytes, blocks))
}

fn fill_deterministic(bytes: &mut [u8], mut state: u64) {
    for byte in bytes {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *byte = state as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_corpus_uses_shards_for_both_modes() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output =
            std::env::temp_dir().join(format!("obs-bench-corpus-{}-{nonce}", std::process::id()));
        let manifest = synth(&output, 8192, 8, 7).unwrap();
        assert_eq!(manifest.logical_bytes, 16_384);
        assert!(manifest.artifacts.iter().any(|row| row.mode == "cct_only"));
        assert!(
            manifest
                .artifacts
                .iter()
                .any(|row| row.mode == "full_trace")
        );
        assert!(manifest.physical_template_bytes < manifest.generated_bytes);
        assert!(
            manifest
                .artifacts
                .iter()
                .all(|row| Path::new(&row.path).is_file())
        );
        fs::remove_dir_all(output).unwrap();
    }
}
