use std::path::Path;

use anyhow::{Context, Result};
use bex_events::prof::{pb::disk_event_v1::Event, read::read_bamlprof_from_bytes};

use crate::prof_stats::event_kind_and_timestamp;

pub(crate) fn replay(path: &Path) -> Result<()> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let contents =
        read_bamlprof_from_bytes(&bytes).with_context(|| format!("decode {}", path.display()))?;
    println!(
        "{}",
        serde_json::json!({
            "type": "header",
            "schema_version": 1,
            "engine_id": contents.header.engine_id,
            "revision_id": contents.header.revision_id,
            "source_snapshot_id": contents.header.source_snapshot_id,
            "truncated": contents.truncated,
        })
    );
    for (sequence, envelope) in contents.events.iter().enumerate() {
        let event = envelope.event.as_ref();
        let (kind, timestamp_ns) = event_kind_and_timestamp(event);
        let detail = event_detail(event);
        println!(
            "{}",
            serde_json::json!({
                "type": "event",
                "sequence": sequence,
                "kind": kind,
                "timestamp_ns": timestamp_ns,
                "detail": detail,
            })
        );
    }
    Ok(())
}

fn event_detail(event: Option<&Event>) -> serde_json::Value {
    match event {
        Some(Event::StartThread(value)) => serde_json::json!({
            "thread_id": value.thread_id,
            "parent_thread_id": value.parent_thread_id,
            "parent_call_id": value.parent_call_id,
            "name": value.name,
        }),
        Some(Event::EndThread(value)) => serde_json::json!({
            "thread_id": value.thread_id,
            "status": value.status,
        }),
        Some(Event::CallFunction(value)) => serde_json::json!({
            "thread_id": value.thread_id,
            "call_id": value.call_id,
            "parent_call_id": value.parent_call_id,
            "function_id": value.function_id,
        }),
        Some(Event::SetFunctionId(value)) => serde_json::json!({
            "thread_id": value.thread_id,
            "call_id": value.call_id,
            "id": value.id,
        }),
        Some(Event::EndFunction(value)) => serde_json::json!({
            "thread_id": value.thread_id,
            "call_id": value.call_id,
            "status": value.status,
        }),
        Some(Event::SuspendThread(value)) => serde_json::json!({
            "thread_id": value.thread_id,
            "suspend_seq": value.suspend_seq,
            "reason": value.reason,
        }),
        Some(Event::ResumeThread(value)) => serde_json::json!({
            "thread_id": value.thread_id,
            "suspend_seq": value.suspend_seq,
            "suspend_timestamp_ns": value.suspend_timestamp_ns,
        }),
        Some(Event::LlmCallMeta(value)) => serde_json::json!({
            "thread_id": value.thread_id,
            "call_id": value.call_id,
            "model_id": value.model_id,
            "tokens_in": value.tokens_in,
            "tokens_out": value.tokens_out,
            "flags": value.flags,
        }),
        Some(Event::Heartbeat(_)) | None => serde_json::Value::Null,
    }
}
