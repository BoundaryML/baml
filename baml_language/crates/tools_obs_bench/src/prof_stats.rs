use std::{collections::BTreeMap, path::PathBuf};

use anyhow::{Context, Result};
use bex_events::prof::{pb::disk_event_v1::Event, read::read_bamlprof_from_bytes};
use serde::Serialize;

use crate::row::ArtifactIdentity;

#[derive(Debug, Serialize)]
pub(crate) struct ProfStats {
    schema_version: u32,
    artifact: ArtifactIdentity,
    engine_id: u64,
    revision_id: String,
    source_snapshot_id: String,
    functions: usize,
    events: usize,
    event_counts: BTreeMap<&'static str, u64>,
    min_timestamp_ns: Option<u64>,
    max_timestamp_ns: Option<u64>,
    truncated: bool,
}

pub(crate) fn inspect_all(paths: &[PathBuf]) -> Result<Vec<ProfStats>> {
    paths
        .iter()
        .map(|path| {
            let (artifact, bytes) =
                ArtifactIdentity::read(path).with_context(|| format!("read {}", path.display()))?;
            let contents = read_bamlprof_from_bytes(&bytes)
                .with_context(|| format!("decode {}", path.display()))?;
            let mut event_counts = BTreeMap::new();
            let mut min_timestamp_ns = None;
            let mut max_timestamp_ns = None;
            for event in &contents.events {
                let (kind, timestamp) = event_kind_and_timestamp(event.event.as_ref());
                *event_counts.entry(kind).or_default() += 1;
                if let Some(timestamp) = timestamp {
                    min_timestamp_ns =
                        Some(min_timestamp_ns.map_or(timestamp, |old: u64| old.min(timestamp)));
                    max_timestamp_ns =
                        Some(max_timestamp_ns.map_or(timestamp, |old: u64| old.max(timestamp)));
                }
            }
            Ok(ProfStats {
                schema_version: 1,
                artifact,
                engine_id: contents.header.engine_id,
                revision_id: contents.header.revision_id,
                source_snapshot_id: contents.header.source_snapshot_id,
                functions: contents
                    .header
                    .function_table
                    .as_ref()
                    .map_or(0, |table| table.functions.len()),
                events: contents.events.len(),
                event_counts,
                min_timestamp_ns,
                max_timestamp_ns,
                truncated: contents.truncated,
            })
        })
        .collect()
}

pub(crate) fn event_kind_and_timestamp(event: Option<&Event>) -> (&'static str, Option<u64>) {
    match event {
        Some(Event::StartThread(value)) => ("start_thread", Some(value.timestamp_ns)),
        Some(Event::EndThread(value)) => ("end_thread", Some(value.timestamp_ns)),
        Some(Event::CallFunction(value)) => ("call_function", Some(value.timestamp_ns)),
        Some(Event::SetFunctionId(value)) => ("set_function_id", Some(value.timestamp_ns)),
        Some(Event::EndFunction(value)) => ("end_function", Some(value.timestamp_ns)),
        Some(Event::Heartbeat(value)) => ("heartbeat", Some(value.timestamp_ns)),
        Some(Event::SuspendThread(value)) => ("suspend_thread", Some(value.timestamp_ns)),
        Some(Event::ResumeThread(value)) => ("resume_thread", Some(value.timestamp_ns)),
        Some(Event::LlmCallMeta(value)) => ("llm_call_meta", Some(value.timestamp_ns)),
        None => ("missing", None),
    }
}
