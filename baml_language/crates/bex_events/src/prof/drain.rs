//! Cooperative profile draining for targets without a native consumer thread.
//!
//! Native persists profile data through `consumer.rs` and `file.rs`. WASM and
//! browser-only hosts drive this state machine explicitly at safe points,
//! receiving the same normalized profile events plus retained `.bamlprof`
//! bytes from a target-neutral sink.

#![allow(unsafe_code)]

use std::collections::{HashMap, HashSet};

use crate::{
    history::router::HistoryProfileRecord,
    ids::{EngineId, ProcessEuid},
    prof::{
        artifact::{
            ByteProfileArtifactSink, ByteProfileArtifactStats, ProfileArtifactRef,
            ProfileArtifactSink,
        },
        clock::{self, TickConverter},
        encode::{build_header, encode_disk_event, encode_length_delimited_message},
        metadata, record,
        registry::Registry,
        ring::Ring,
        transcode::to_disk_event,
    },
    run::{
        ProfileEventEnvelope, ProfileEventSource, RuntimeTarget,
        profile_event_envelope_from_disk_event,
    },
};

#[derive(Clone, Debug)]
pub struct CooperativeProfileDrainOptions {
    pub target: RuntimeTarget,
    pub source_id: String,
    pub max_bytes_per_engine: Option<usize>,
}

#[derive(Debug)]
pub struct CooperativeProfileDrain {
    options: CooperativeProfileDrainOptions,
    conv: TickConverter,
    process_id: [u8; 16],
    started_at_epoch_ns: u128,
    engines: HashMap<u64, EngineDrain>,
    closed_engines: HashSet<u64>,
    closed_reported: HashSet<u64>,
    corrupt_reported: bool,
}

#[derive(Debug)]
struct EngineDrain {
    sink: ByteProfileArtifactSink,
    header_written: bool,
    truncation_reported: bool,
}

#[derive(Debug, Default)]
pub struct CooperativeProfileDrainOutput {
    pub progress: bool,
    pub events: Vec<ProfileEventEnvelope>,
    pub history_records: Vec<HistoryProfileRecord>,
    pub chunks: Vec<ProfileArtifactChunk>,
    pub artifacts: Vec<ProfileArtifactSnapshot>,
    pub diagnostics: Vec<ProfileDrainDiagnostic>,
}

#[derive(Clone, Debug)]
pub struct ProfileArtifactChunk {
    pub engine_id: EngineId,
    pub process_euid: ProcessEuid,
    pub bytes: Vec<u8>,
    pub stats: ByteProfileArtifactStats,
}

#[derive(Clone, Debug)]
pub struct ProfileArtifactSnapshot {
    pub engine_id: EngineId,
    pub process_euid: ProcessEuid,
    pub bytes: Vec<u8>,
    pub stats: ByteProfileArtifactStats,
    pub artifact_ref: ProfileArtifactRef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileDrainDiagnostic {
    pub engine_id: Option<EngineId>,
    pub code: &'static str,
    pub message: String,
}

impl CooperativeProfileDrain {
    #[must_use]
    pub fn new(options: CooperativeProfileDrainOptions) -> Self {
        Self {
            options,
            conv: TickConverter::from_clock(),
            process_id: *uuid::Uuid::new_v4().as_bytes(),
            started_at_epoch_ns: clock::started_at_epoch_ns(),
            engines: HashMap::new(),
            closed_engines: HashSet::new(),
            closed_reported: HashSet::new(),
            corrupt_reported: false,
        }
    }

    #[cfg(test)]
    fn with_identity(
        options: CooperativeProfileDrainOptions,
        conv: TickConverter,
        process_id: [u8; 16],
        started_at_epoch_ns: u128,
    ) -> Self {
        Self {
            options,
            conv,
            process_id,
            started_at_epoch_ns,
            engines: HashMap::new(),
            closed_engines: HashSet::new(),
            closed_reported: HashSet::new(),
            corrupt_reported: false,
        }
    }

    pub(crate) fn drain_until_idle(
        &mut self,
        registry: &'static Registry,
        max_sweeps: usize,
    ) -> CooperativeProfileDrainOutput {
        let mut output = CooperativeProfileDrainOutput::default();
        let mut touched_engines = HashSet::new();
        let max_sweeps = max_sweeps.max(1);
        for _ in 0..max_sweeps {
            // SAFETY: cooperative targets have exactly one drain owner. The
            // native background consumer is not compiled into wasm, and tests
            // below use private registries.
            let progress = unsafe {
                registry.sweep(&mut |ring, bytes| {
                    self.transcode(ring, bytes, &mut output, &mut touched_engines);
                })
            };
            output.progress |= progress;
            if !progress {
                break;
            }
        }
        output.artifacts = self.artifact_snapshots(&touched_engines);
        output
    }

    fn transcode(
        &mut self,
        ring: &'static Ring,
        bytes: &[u8],
        output: &mut CooperativeProfileDrainOutput,
        touched_engines: &mut HashSet<u64>,
    ) {
        // SAFETY: `Registry::sweep` handed us this ring only after a drain
        // reported progress in this sweep.
        let engine_id = unsafe { ring.engine_id() };
        if self.closed_engines.contains(&engine_id) {
            if self.closed_reported.insert(engine_id) {
                output.diagnostics.push(ProfileDrainDiagnostic {
                    engine_id: Some(EngineId(engine_id)),
                    code: "ProfileRecordsAfterEngineClose",
                    message: format!("dropping records for closed engine {engine_id}"),
                });
            }
            return;
        }

        self.ensure_header(engine_id, output, touched_engines);

        let mut corrupt = None;
        let mut chunk = Vec::new();
        for rec in record::iter(bytes) {
            match rec {
                Ok(raw) => {
                    let event = to_disk_event(&raw, &self.conv);
                    if let Some(envelope) = profile_event_envelope_from_disk_event(
                        ProfileEventSource::Live {
                            target: self.options.target.clone(),
                            source_id: self.options.source_id.clone(),
                        },
                        ProcessEuid(self.process_id),
                        EngineId(engine_id),
                        &event,
                    ) {
                        crate::history::publish_history_profile_event(&envelope, &event);
                        output.history_records.push(HistoryProfileRecord {
                            envelope: envelope.clone(),
                            disk_event: event.clone(),
                        });
                        output.events.push(envelope);
                    }
                    encode_disk_event(&mut chunk, &event);
                }
                Err(err) => {
                    corrupt = Some(err);
                    break;
                }
            }
        }
        if !chunk.is_empty() {
            self.write_chunk(engine_id, &chunk, output, touched_engines);
        }
        if let Some(err) = corrupt
            && !self.corrupt_reported
        {
            self.corrupt_reported = true;
            output.diagnostics.push(ProfileDrainDiagnostic {
                engine_id: Some(EngineId(engine_id)),
                code: "CorruptProfileRecord",
                message: format!(
                    "corrupt profiling record in committed range for engine {engine_id}: {err:?}"
                ),
            });
        }
    }

    fn ensure_header(
        &mut self,
        engine_id: u64,
        output: &mut CooperativeProfileDrainOutput,
        touched_engines: &mut HashSet<u64>,
    ) {
        let engine = self
            .engines
            .entry(engine_id)
            .or_insert_with(|| EngineDrain {
                sink: self.options.max_bytes_per_engine.map_or_else(
                    ByteProfileArtifactSink::new,
                    ByteProfileArtifactSink::with_max_bytes,
                ),
                header_written: false,
                truncation_reported: false,
            });
        if engine.header_written {
            return;
        }

        let meta = metadata::get_engine_metadata(engine_id);
        let header = build_header(
            self.process_id,
            engine_id,
            self.started_at_epoch_ns,
            meta.as_ref(),
            &self.conv,
        );
        let mut chunk = Vec::new();
        if let Err(err) = encode_length_delimited_message(&mut chunk, &header) {
            output.diagnostics.push(ProfileDrainDiagnostic {
                engine_id: Some(EngineId(engine_id)),
                code: "ProfileHeaderEncodeFailed",
                message: format!("failed to encode .bamlprof header for engine {engine_id}: {err}"),
            });
            return;
        }
        if self.write_chunk(engine_id, &chunk, output, touched_engines)
            && let Some(engine) = self.engines.get_mut(&engine_id)
        {
            engine.header_written = true;
        }
    }

    fn write_chunk(
        &mut self,
        engine_id: u64,
        chunk: &[u8],
        output: &mut CooperativeProfileDrainOutput,
        touched_engines: &mut HashSet<u64>,
    ) -> bool {
        let Some(engine) = self.engines.get_mut(&engine_id) else {
            return false;
        };
        let before = engine.sink.stats();
        let retained_chunk_len = before.max_bytes.map_or(chunk.len(), |max_bytes| {
            max_bytes
                .saturating_sub(before.retained_bytes)
                .min(chunk.len())
        });
        if let Err(err) = engine.sink.write_chunk(chunk) {
            output.diagnostics.push(ProfileDrainDiagnostic {
                engine_id: Some(EngineId(engine_id)),
                code: "ProfileArtifactWriteFailed",
                message: format!("failed to write .bamlprof bytes for engine {engine_id}: {err}"),
            });
            return false;
        }
        touched_engines.insert(engine_id);
        let stats = engine.sink.stats();
        if retained_chunk_len > 0 {
            output.chunks.push(ProfileArtifactChunk {
                engine_id: EngineId(engine_id),
                process_euid: ProcessEuid(self.process_id),
                bytes: chunk[..retained_chunk_len].to_vec(),
                stats,
            });
        }
        if !engine.truncation_reported && (stats.dropped_bytes > 0 || stats.dropped_chunks > 0) {
            engine.truncation_reported = true;
            output.diagnostics.push(ProfileDrainDiagnostic {
                engine_id: Some(EngineId(engine_id)),
                code: "ProfileArtifactTruncated",
                message: format!(
                    ".bamlprof artifact for engine {engine_id} exceeded the byte cap; dropped {} bytes across {} chunks",
                    stats.dropped_bytes, stats.dropped_chunks
                ),
            });
        }
        true
    }

    fn artifact_snapshots(&mut self, engine_ids: &HashSet<u64>) -> Vec<ProfileArtifactSnapshot> {
        let mut snapshots = engine_ids
            .iter()
            .filter_map(|engine_id| self.artifact_snapshot(*engine_id))
            .collect::<Vec<_>>();
        snapshots.sort_by_key(|snapshot| snapshot.engine_id.0);
        snapshots
    }

    fn artifact_snapshot(&mut self, engine_id: u64) -> Option<ProfileArtifactSnapshot> {
        let engine = self.engines.get_mut(&engine_id)?;
        let artifact_ref = engine
            .sink
            .flush()
            .unwrap_or_else(|err| ProfileArtifactRef::Bytes {
                len: 0,
                truncated: true,
                dropped_bytes: err.to_string().len(),
                dropped_chunks: 1,
            });
        Some(ProfileArtifactSnapshot {
            engine_id: EngineId(engine_id),
            process_euid: ProcessEuid(self.process_id),
            bytes: engine.sink.bytes().to_vec(),
            stats: engine.sink.stats(),
            artifact_ref,
        })
    }
}

pub fn drain_global_until_idle(
    drain: &mut CooperativeProfileDrain,
    max_sweeps: usize,
) -> CooperativeProfileDrainOutput {
    drain.drain_until_idle(crate::prof::registry::global_registry(), max_sweeps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ids::{BexCallId, BexThreadId, FunctionId},
        prof::{
            EngineProfileMetadata, FunctionMetaEntry,
            clock::TickConverter,
            file::ProfileWriter,
            read::read_bamlprof_from_bytes,
            record::{FunctionEndStatus, RawRecord, ThreadEndStatus},
            register_engine_metadata,
            registry::Registry,
            ring::RingCtx,
        },
        run::{self, RuntimeTarget},
    };

    const ENGINE: u64 = 0xD0A1_0001;

    fn leak<T>(value: T) -> &'static T {
        Box::leak(Box::new(value))
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "bamlprof-cooperative-drain-{}-{tag}",
            uuid::Uuid::new_v4()
        ))
    }

    fn options(max_bytes_per_engine: Option<usize>) -> CooperativeProfileDrainOptions {
        CooperativeProfileDrainOptions {
            target: RuntimeTarget::Wasm,
            source_id: "test-wasm-drain".to_string(),
            max_bytes_per_engine,
        }
    }

    fn metadata() -> EngineProfileMetadata {
        EngineProfileMetadata {
            program_id: "program".to_string(),
            source_snapshot_id: Some("snapshot".to_string()),
            revision_id: Some("revision".to_string()),
            functions: vec![FunctionMetaEntry {
                function_id: 1,
                fqn: "pkg.Main".to_string(),
                source_file: "main.baml".to_string(),
                span_start: 1,
                span_end: 20,
                kind: "bytecode".to_string(),
                definition_key: Some("function:pkg.Main".to_string()),
                owner_type: None,
                parent_function: None,
                lambda_path: None,
                package_name: Some("pkg".to_string()),
                namespace: vec!["pkg".to_string()],
            }],
        }
    }

    fn push_records(registry: &'static Registry, ctx: &'static RingCtx, engine_id: u64) {
        let handle = registry.acquire(ctx, 64 * 1024, 1, engine_id);
        let records = [
            RawRecord::StartThread {
                flags: 0,
                thread_id: BexThreadId(1),
                parent_thread_id: BexThreadId(0),
                parent_call_id: BexCallId(0),
                ts_ticks: 1,
                name: b"root",
            },
            RawRecord::CallFunction {
                flags: 0,
                thread_id: BexThreadId(1),
                call_id: BexCallId(2),
                parent_call_id: BexCallId(0),
                function_id: FunctionId(1),
                ts_ticks: 2,
                call_site: None,
            },
            RawRecord::EndFunction {
                status: FunctionEndStatus::Ok,
                thread_id: BexThreadId(1),
                call_id: BexCallId(2),
                ts_ticks: 3,
            },
            RawRecord::EndThread {
                status: ThreadEndStatus::Completed,
                thread_id: BexThreadId(1),
                ts_ticks: 4,
            },
        ];
        for record in records {
            let mut buf = [0u8; record::MAX_RECORD_LEN];
            let len = record.encode(&mut buf);
            // SAFETY: this test owns the producer handle on this thread.
            unsafe { handle.push(&buf[..len]) };
        }
    }

    #[test]
    fn cooperative_drain_emits_events_and_valid_bamlprof_bytes() {
        let registry = leak(Registry::new());
        let ctx = leak(RingCtx::new(1 << 20));
        register_engine_metadata(ENGINE, metadata());
        push_records(registry, ctx, ENGINE);

        let mut drain = CooperativeProfileDrain::with_identity(
            options(None),
            TickConverter::identity(),
            [7; 16],
            123,
        );
        let output = drain.drain_until_idle(registry, 8);

        assert!(output.progress);
        assert_eq!(output.events.len(), 4);
        assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
        assert_eq!(output.artifacts.len(), 1);

        let parsed = read_bamlprof_from_bytes(&output.artifacts[0].bytes).unwrap();
        assert_eq!(parsed.header.engine_id, ENGINE);
        assert_eq!(parsed.header.program_id, "program");
        assert_eq!(parsed.events.len(), 4);
        assert!(!parsed.truncated);

        let dir = temp_dir("byte-contract");
        let mut writer = ProfileWriter::create(&dir, [7; 16], 123, ENGINE, &parsed.header).unwrap();
        for event in &parsed.events {
            writer.encode_event(event);
        }
        writer.sync().unwrap();
        let native_path = writer.path().to_path_buf();
        drop(writer);
        let native_bytes = std::fs::read(&native_path).unwrap();
        assert_eq!(native_bytes, output.artifacts[0].bytes);
        let _ = std::fs::remove_file(native_path);
        let _ = std::fs::remove_dir(dir);

        let reconstructed = run::bamlprof::reconstruct_bamlprof(&parsed).unwrap();
        assert_eq!(reconstructed.calls.len(), 1);
        assert_eq!(
            reconstructed.calls[0].function_name,
            Some("pkg.Main".to_string())
        );
    }

    #[test]
    fn cooperative_drain_reports_bounded_artifact_truncation() {
        let registry = leak(Registry::new());
        let ctx = leak(RingCtx::new(1 << 20));
        let engine = ENGINE + 1;
        register_engine_metadata(engine, metadata());
        push_records(registry, ctx, engine);

        let mut drain = CooperativeProfileDrain::with_identity(
            options(Some(16)),
            TickConverter::identity(),
            [8; 16],
            123,
        );
        let output = drain.drain_until_idle(registry, 8);

        assert_eq!(output.artifacts.len(), 1);
        assert_eq!(output.artifacts[0].stats.retained_bytes, 16);
        assert!(output.artifacts[0].stats.dropped_bytes > 0);
        assert!(output.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "ProfileArtifactTruncated"
                && diagnostic.engine_id == Some(EngineId(engine))
        }));
    }
}
