//! Cooperative profile draining for targets without a native consumer thread.
//!
//! Native drains through `consumer.rs`. WASM and browser-only hosts drive
//! this state machine explicitly at safe points. Live aggregation is the
//! §5.12 embedded CCT engine — the same plane the native consumer runs — and
//! [`CooperativeProfileDrain::cct_live_segment`] serves its always-sealed
//! live-segment bytes. The legacy per-event transcode outputs (normalized
//! profile events, `.bamlprof` chunk/artifact bytes, history records) left
//! with the run store's profile-event projection (§9.3 "one live plane").

#![allow(unsafe_code)]

use std::collections::{HashMap, HashSet};

use crate::{
    ids::EngineId,
    prof::{
        clock::{self, TickConverter},
        record,
        registry::Registry,
        ring::Ring,
    },
};

pub struct CooperativeProfileDrain {
    conv: TickConverter,
    process_id: [u8; 16],
    started_at_epoch_ns: u128,
    /// §5.12: the CCT engine embeds in the cooperative drain — wasm hosts
    /// get the same aggregation plane as the native consumer, and
    /// [`CooperativeProfileDrain::cct_live_segment`] serves the identical
    /// always-sealed live-segment bytes.
    cct: HashMap<u64, crate::prof::cct::CctEngine>,
    /// Latched from `ProfConfig` at construction; tests override.
    cct_enabled: bool,
    closed_engines: HashSet<u64>,
    closed_reported: HashSet<u64>,
    corrupt_reported: bool,
}

#[derive(Debug, Default)]
pub struct CooperativeProfileDrainOutput {
    pub progress: bool,
    pub diagnostics: Vec<ProfileDrainDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileDrainDiagnostic {
    pub engine_id: Option<EngineId>,
    pub code: &'static str,
    pub message: String,
}

impl Default for CooperativeProfileDrain {
    fn default() -> Self {
        Self::new()
    }
}

impl CooperativeProfileDrain {
    #[must_use]
    pub fn new() -> Self {
        Self {
            conv: TickConverter::from_clock(),
            process_id: *uuid::Uuid::new_v4().as_bytes(),
            started_at_epoch_ns: clock::started_at_epoch_ns(),
            cct: HashMap::new(),
            cct_enabled: crate::prof::ProfConfig::global().pipeline.runs_cct(),
            closed_engines: HashSet::new(),
            closed_reported: HashSet::new(),
            corrupt_reported: false,
        }
    }

    /// Force the §5.12 CCT embedding on/off (tests; hosts inherit the
    /// pipeline config).
    pub fn set_cct_enabled(&mut self, enabled: bool) {
        self.cct_enabled = enabled;
    }

    /// §5.12/§9.2: the whole-engine live fold encoded as an always-sealed
    /// BCCT segment — identical bytes to the native consumer's
    /// `cct_live_segment`, so the wasm `ObserveEngine` folds it with the
    /// same code. Sweeps deferrals first so cross-ring stragglers resolve.
    pub fn cct_live_segment(&mut self, engine_id: u64) -> Option<Vec<u8>> {
        let conv = self.conv.clone();
        let engine = self.cct.get_mut(&engine_id)?;
        engine.sweep_tick(&mut |ticks| conv.to_ns(ticks));
        let folded = crate::prof::cct::fold::fold_all(engine);
        let meta = crate::prof::metadata::get_engine_metadata(engine_id);
        let revision_string = meta
            .as_ref()
            .and_then(|m| m.revision_id.clone())
            .unwrap_or_default();
        let revision_bytes =
            bex_vm_types::RevisionId::decode(&revision_string).map_or([0u8; 32], |id| id.0);
        let (numer, denom) = self.conv.rate();
        let header = crate::prof::cct::segment::SegmentHeader {
            process_euid: self.process_id,
            engine_id,
            session_seg_seq: 0,
            started_epoch_ns: u64::try_from(self.started_at_epoch_ns).unwrap_or(u64::MAX),
            clock_kind: self.conv.kind() as u8,
            clock_quality: self.conv.quality() as u8,
            tick_ns_numer: numer,
            tick_ns_denom: denom,
            revision_id: revision_bytes,
        };
        Some(crate::prof::cct::fold::encode_live_snapshot(
            &folded, &header,
        ))
    }

    pub(crate) fn drain_until_idle(
        &mut self,
        registry: &'static Registry,
        max_sweeps: usize,
    ) -> CooperativeProfileDrainOutput {
        let mut output = CooperativeProfileDrainOutput::default();
        let max_sweeps = max_sweeps.max(1);
        for _ in 0..max_sweeps {
            // SAFETY: cooperative targets have exactly one drain owner. The
            // native background consumer is not compiled into wasm, and tests
            // below use private registries.
            let progress = unsafe {
                registry.sweep(&mut |ring, bytes| {
                    self.consume(ring, bytes, &mut output);
                })
            };
            output.progress |= progress;
            if !progress {
                break;
            }
        }
        output
    }

    fn consume(
        &mut self,
        ring: &'static Ring,
        bytes: &[u8],
        output: &mut CooperativeProfileDrainOutput,
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

        // §5.12 CCT embedding: the aggregation plane consumes the raw bytes
        // directly (it degrades itself on a corrupt range).
        if self.cct_enabled {
            let conv = self.conv.clone();
            let engine = self.cct.entry(engine_id).or_insert_with(|| {
                let function_count = crate::prof::metadata::get_engine_metadata(engine_id)
                    .map_or(0, |meta| u32::try_from(meta.functions.len()).unwrap_or(0));
                crate::prof::cct::CctEngine::new(function_count)
            });
            engine.consume(bytes, &mut |ticks| conv.to_ns(ticks));
        }

        // Corruption in the committed range is a logic error worth surfacing
        // even though no per-event transcode consumes the records anymore.
        if !self.corrupt_reported
            && let Some(err) = record::iter(bytes).find_map(Result::err)
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
            record::{FunctionEndStatus, RawRecord, ThreadEndStatus},
            register_engine_metadata,
            registry::Registry,
            ring::RingCtx,
        },
    };

    const ENGINE: u64 = 0xD0A1_0001;

    fn leak<T>(value: T) -> &'static T {
        Box::leak(Box::new(value))
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
            dictionary: None,
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
    fn cooperative_drain_reports_progress_without_diagnostics() {
        let registry = leak(Registry::new());
        let ctx = leak(RingCtx::new(1 << 20));
        register_engine_metadata(ENGINE, metadata());
        push_records(registry, ctx, ENGINE);

        let mut drain = CooperativeProfileDrain::new();
        let output = drain.drain_until_idle(registry, 8);

        assert!(output.progress);
        assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
        let _ = crate::prof::metadata::remove_engine_metadata(ENGINE);
    }

    #[test]
    fn cooperative_drain_serves_a_sealed_cct_live_segment() {
        let registry = leak(Registry::new());
        let ctx = leak(RingCtx::new(1 << 20));
        register_engine_metadata(ENGINE + 7, metadata());
        push_records(registry, ctx, ENGINE + 7);

        let mut drain = CooperativeProfileDrain::new();
        drain.set_cct_enabled(true);
        let output = drain.drain_until_idle(registry, 8);
        assert!(output.progress);

        let bytes = drain
            .cct_live_segment(ENGINE + 7)
            .expect("live segment after drain");
        let contents = crate::prof::cct::segment::scan_segment(&bytes).expect("live segment scans");
        assert_eq!(contents.end, crate::prof::cct::segment::ScanEnd::Sealed);
        let totals = contents
            .blocks
            .iter()
            .find(|b| b.kind == crate::prof::cct::segment::BlockKind::NodeTotal as u8)
            .expect("node_total block");
        let rows =
            crate::prof::cct::blocks::decode_cct_delta(totals.payload, totals.row_count as usize)
                .expect("totals decode");
        let enters: u32 = rows.iter().map(|r| r.enters).sum();
        assert!(enters >= 1, "the pushed call is aggregated: {rows:?}");
        let _ = crate::prof::metadata::remove_engine_metadata(ENGINE + 7);
    }
}
