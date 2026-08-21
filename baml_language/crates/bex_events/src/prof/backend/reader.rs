//! Streaming durable reader for the segmented profiling store.

use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
};

use super::{
    BoundaryHealthSnapshot, BoundaryRunMeta, CctCounters, ContextDelta, ContextKey, ContextRef,
    ContextTuple, CounterHealth, DecodedCasObject, DecodedCctSegment, DecodedEvidenceSegment,
    DecodedRunEnd, EdgeKind, ErrorCapture, ErrorCaptureId, EvidenceFact, OverflowDelta,
    OverflowReason, RunEndSegmentFence, SegmentHighWater, SegmentKind, SegmentReadError, SpanEnd,
    SpanRuntimeId, SpanStart, TerminalErrorRef, TerminalErrorTarget, ValueCid, ValueOccurrence,
    ValueRole, decode_cas_object, decode_cct_segment, decode_evidence_segment, decode_run_end,
    decode_run_meta,
};
use crate::ids::{BoundaryId, CallRef};

#[derive(Debug)]
pub enum RunReadError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Decode {
        path: PathBuf,
        source: SegmentReadError,
    },
    InvalidFence {
        kind: SegmentKind,
        fence: SegmentHighWater,
    },
    MissingSegment {
        kind: SegmentKind,
        sequence: u64,
    },
    SegmentBeyondFence {
        kind: SegmentKind,
        sequence: u64,
    },
    MetadataMismatch {
        kind: SegmentKind,
        sequence: u64,
    },
    SequenceMismatch {
        kind: SegmentKind,
        expected: u64,
        actual: u64,
    },
    ConflictingContextDefinition(ContextKey),
    /// A context's parent chain revisits a key: the segments verified their
    /// checksums, so this is a forged or corrupt CCT, never a reorder.
    CyclicContextChain(ContextKey),
    DuplicateSpanStart(CallRef),
    DuplicateSpanEnd(CallRef),
    DuplicateValueOccurrence {
        call_ref: CallRef,
        role: ValueRole,
    },
    DuplicateErrorCapture(ErrorCaptureId),
    MissingSpanStart(CallRef),
    MissingContextDefinition(ContextKey),
    MissingErrorCapture(ErrorCaptureId),
    CasIdentityMismatch(ValueCid),
    InvalidTerminalHealth,
    SequenceExhausted,
}

impl std::fmt::Display for RunReadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "profiling run read failed: {self:?}")
    }
}

impl std::error::Error for RunReadError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunReaderCursor {
    pub next_cct_sequence: u64,
    pub next_evidence_sequence: u64,
}

impl RunReaderCursor {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next_cct_sequence: 1,
            next_evidence_sequence: 1,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DurableRunReader {
    store_root: PathBuf,
    run_directory: PathBuf,
    pub meta: BoundaryRunMeta,
    pub end: Option<DecodedRunEnd>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergedContext {
    pub tuple: Option<ContextTuple>,
    pub counters: CctCounters,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpanEvidence {
    pub start: Option<SpanStart>,
    pub end: Option<SpanEnd>,
    pub runtime_ids: Vec<SpanRuntimeId>,
    pub input: Option<ValueOccurrence>,
    pub output: Option<ValueOccurrence>,
    pub terminal_error: Option<TerminalErrorRef>,
}

#[derive(Clone, Debug)]
pub struct ProfileRun {
    pub meta: BoundaryRunMeta,
    pub end: Option<DecodedRunEnd>,
    pub contexts: HashMap<ContextKey, MergedContext>,
    pub overflow: HashMap<(OverflowReason, EdgeKind), CctCounters>,
    pub cct_health: CounterHealth,
    pub terminal_health: BoundaryHealthSnapshot,
    pub spans: HashMap<CallRef, SpanEvidence>,
    pub errors: HashMap<ErrorCaptureId, ErrorCapture>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorStack {
    Complete(Vec<ContextTuple>),
    StackIncomplete {
        throw_function_id: crate::ids::FunctionId,
        throw_site: Option<super::ThrowSite>,
    },
}

impl DurableRunReader {
    pub fn open(
        store_root: impl Into<PathBuf>,
        boundary_id: BoundaryId,
    ) -> Result<Self, RunReadError> {
        let store_root = store_root.into();
        let run_directory = store_root
            .join("runs")
            .join(hex::encode(boundary_id.as_bytes()));
        let meta_path = run_directory.join("run.meta");
        let meta = decode_run_meta(&read(&meta_path)?).map_err(|source| RunReadError::Decode {
            path: meta_path,
            source,
        })?;
        if meta.boundary_id != boundary_id {
            return Err(RunReadError::MetadataMismatch {
                kind: SegmentKind::Cct,
                sequence: 0,
            });
        }
        let end_path = run_directory.join("run.end");
        let end = match fs::read(&end_path) {
            Ok(bytes) => Some(
                decode_run_end(&bytes).map_err(|source| RunReadError::Decode {
                    path: end_path,
                    source,
                })?,
            ),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(source) => {
                return Err(RunReadError::Io {
                    path: end_path,
                    source,
                });
            }
        };
        if let Some(end) = &end {
            validate_fence(SegmentKind::Cct, end.fence.cct)?;
            validate_fence(SegmentKind::Evidence, end.fence.evidence)?;
        }
        Ok(Self {
            store_root,
            run_directory,
            meta,
            end,
        })
    }

    #[must_use]
    pub fn sealed_fence(&self) -> Option<RunEndSegmentFence> {
        self.end.as_ref().map(|end| end.fence)
    }

    pub fn read_next_cct(
        &self,
        cursor: &mut RunReaderCursor,
        committed_sequence: Option<u64>,
    ) -> Result<Option<DecodedCctSegment>, RunReadError> {
        let sequence = cursor.next_cct_sequence.max(1);
        let limit =
            committed_sequence.or_else(|| self.end.as_ref().map(|end| end.fence.cct.last_sequence));
        if limit.is_some_and(|limit| sequence > limit) {
            return Ok(None);
        }
        let path = segment_path(&self.run_directory, SegmentKind::Cct, sequence);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound && limit.is_none() => {
                return Ok(None);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(RunReadError::MissingSegment {
                    kind: SegmentKind::Cct,
                    sequence,
                });
            }
            Err(source) => return Err(RunReadError::Io { path, source }),
        };
        let segment =
            decode_cct_segment(&bytes).map_err(|source| RunReadError::Decode { path, source })?;
        self.validate_segment(
            SegmentKind::Cct,
            sequence,
            segment.sequence,
            segment.boundary_id,
            segment.program_id,
        )?;
        cursor.next_cct_sequence = sequence
            .checked_add(1)
            .ok_or(RunReadError::SequenceExhausted)?;
        Ok(Some(segment))
    }

    pub fn read_next_evidence(
        &self,
        cursor: &mut RunReaderCursor,
        committed_sequence: Option<u64>,
    ) -> Result<Option<DecodedEvidenceSegment>, RunReadError> {
        let sequence = cursor.next_evidence_sequence.max(1);
        let limit = committed_sequence.or_else(|| {
            self.end
                .as_ref()
                .map(|end| end.fence.evidence.last_sequence)
        });
        if limit.is_some_and(|limit| sequence > limit) {
            return Ok(None);
        }
        let path = segment_path(&self.run_directory, SegmentKind::Evidence, sequence);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound && limit.is_none() => {
                return Ok(None);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(RunReadError::MissingSegment {
                    kind: SegmentKind::Evidence,
                    sequence,
                });
            }
            Err(source) => return Err(RunReadError::Io { path, source }),
        };
        let segment = decode_evidence_segment(&bytes)
            .map_err(|source| RunReadError::Decode { path, source })?;
        self.validate_segment(
            SegmentKind::Evidence,
            sequence,
            segment.sequence,
            segment.boundary_id,
            segment.program_id,
        )?;
        cursor.next_evidence_sequence = sequence
            .checked_add(1)
            .ok_or(RunReadError::SequenceExhausted)?;
        Ok(Some(segment))
    }

    pub fn load(&self) -> Result<ProfileRun, RunReadError> {
        let mut run = ProfileRun {
            meta: self.meta.clone(),
            end: self.end.clone(),
            contexts: HashMap::new(),
            overflow: HashMap::new(),
            cct_health: CounterHealth::default(),
            terminal_health: self.terminal_health()?,
            spans: HashMap::new(),
            errors: HashMap::new(),
        };
        let mut cursor = RunReaderCursor::new();
        while let Some(segment) = self.read_next_cct(&mut cursor, None)? {
            run.merge_cct(segment)?;
        }
        while let Some(segment) = self.read_next_evidence(&mut cursor, None)? {
            run.merge_evidence(segment)?;
        }
        if let Some(end) = &self.end {
            self.reject_segment_beyond_fence(SegmentKind::Cct, end.fence.cct.last_sequence)?;
            self.reject_segment_beyond_fence(
                SegmentKind::Evidence,
                end.fence.evidence.last_sequence,
            )?;
        }
        run.validate_dependencies()?;
        Ok(run)
    }

    pub fn read_value(&self, cid: ValueCid) -> Result<DecodedCasObject, RunReadError> {
        let digest = hex::encode(cid.0);
        let path = self
            .store_root
            .join("cas/sha256")
            .join(&digest[..2])
            .join(format!("{digest}.bamlvalue"));
        let object = decode_cas_object(&read(&path)?)
            .map_err(|source| RunReadError::Decode { path, source })?;
        if object.cid != cid {
            return Err(RunReadError::CasIdentityMismatch(cid));
        }
        Ok(object)
    }

    pub fn terminal_health(&self) -> Result<BoundaryHealthSnapshot, RunReadError> {
        self.end
            .as_ref()
            .map_or(Ok(BoundaryHealthSnapshot::default()), |end| {
                BoundaryHealthSnapshot::decode(&end.end.terminal_health)
                    .ok_or(RunReadError::InvalidTerminalHealth)
            })
    }

    fn validate_segment(
        &self,
        kind: SegmentKind,
        expected: u64,
        actual: u64,
        boundary_id: BoundaryId,
        program_id: crate::ids::ProgramId,
    ) -> Result<(), RunReadError> {
        if actual != expected {
            return Err(RunReadError::SequenceMismatch {
                kind,
                expected,
                actual,
            });
        }
        if boundary_id != self.meta.boundary_id || program_id != self.meta.program_id {
            return Err(RunReadError::MetadataMismatch {
                kind,
                sequence: actual,
            });
        }
        Ok(())
    }

    fn reject_segment_beyond_fence(
        &self,
        kind: SegmentKind,
        last: u64,
    ) -> Result<(), RunReadError> {
        let Some(next) = last.checked_add(1) else {
            return Ok(());
        };
        if segment_path(&self.run_directory, kind, next).exists() {
            return Err(RunReadError::SegmentBeyondFence {
                kind,
                sequence: next,
            });
        }
        Ok(())
    }
}

impl ProfileRun {
    fn merge_cct(&mut self, segment: DecodedCctSegment) -> Result<(), RunReadError> {
        self.cct_health.counter_saturated |= segment.data.health.counter_saturated;
        self.cct_health.await_counter_saturated |= segment.data.health.await_counter_saturated;
        self.cct_health.self_time_underflow |= segment.data.health.self_time_underflow;
        for ContextDelta {
            key,
            tuple,
            counters,
        } in segment.data.contexts
        {
            let entry = self.contexts.entry(key).or_insert(MergedContext {
                tuple: None,
                counters: CctCounters::default(),
            });
            if let Some(tuple) = tuple {
                if entry.tuple.is_some_and(|existing| existing != tuple) {
                    return Err(RunReadError::ConflictingContextDefinition(key));
                }
                entry.tuple = Some(tuple);
            }
            add_counters(&mut entry.counters, counters, &mut self.cct_health);
        }
        for OverflowDelta {
            reason,
            edge_kind,
            counters,
        } in segment.data.overflow
        {
            let entry = self.overflow.entry((reason, edge_kind)).or_default();
            add_counters(entry, counters, &mut self.cct_health);
        }
        Ok(())
    }

    fn merge_evidence(&mut self, segment: DecodedEvidenceSegment) -> Result<(), RunReadError> {
        for fact in segment.facts {
            match fact {
                EvidenceFact::SpanStart(start) => {
                    let span = self.spans.entry(start.call_ref).or_default();
                    if span.start.replace(start).is_some() {
                        return Err(RunReadError::DuplicateSpanStart(start.call_ref));
                    }
                }
                EvidenceFact::SpanEnd(end) => {
                    let span = self.spans.entry(end.call_ref).or_default();
                    if span.end.replace(end).is_some() {
                        return Err(RunReadError::DuplicateSpanEnd(end.call_ref));
                    }
                }
                EvidenceFact::SpanRuntimeId(annotation) => self
                    .spans
                    .entry(annotation.call_ref)
                    .or_default()
                    .runtime_ids
                    .push(annotation),
                EvidenceFact::ValueOccurrence(occurrence) => {
                    let span = self.spans.entry(occurrence.call_ref).or_default();
                    let target = match occurrence.role {
                        ValueRole::Input => &mut span.input,
                        ValueRole::Output => &mut span.output,
                    };
                    if target.replace(occurrence).is_some() {
                        return Err(RunReadError::DuplicateValueOccurrence {
                            call_ref: occurrence.call_ref,
                            role: occurrence.role,
                        });
                    }
                }
                EvidenceFact::ErrorCapture(capture) => {
                    if self.errors.insert(capture.id, capture).is_some() {
                        return Err(RunReadError::DuplicateErrorCapture(capture.id));
                    }
                }
                EvidenceFact::TerminalErrorRef(terminal) => {
                    self.spans
                        .entry(terminal.call_ref)
                        .or_default()
                        .terminal_error = Some(terminal);
                }
            }
        }
        Ok(())
    }

    fn validate_dependencies(&self) -> Result<(), RunReadError> {
        for (call_ref, span) in &self.spans {
            let Some(start) = span.start else {
                return Err(RunReadError::MissingSpanStart(*call_ref));
            };
            if let ContextRef::Normal(key) = start.context_ref
                && self
                    .contexts
                    .get(&key)
                    .is_none_or(|context| context.tuple.is_none())
            {
                return Err(RunReadError::MissingContextDefinition(key));
            }
            if let Some(TerminalErrorRef {
                target: TerminalErrorTarget::Capture(id),
                ..
            }) = span.terminal_error
                && !self.errors.contains_key(&id)
            {
                return Err(RunReadError::MissingErrorCapture(id));
            }
        }
        for capture in self.errors.values() {
            if let ContextRef::Normal(key) = capture.throw_context_ref
                && self
                    .contexts
                    .get(&key)
                    .is_none_or(|context| context.tuple.is_none())
            {
                return Err(RunReadError::MissingContextDefinition(key));
            }
        }
        Ok(())
    }

    pub fn error_stack(&self, id: ErrorCaptureId) -> Result<ErrorStack, RunReadError> {
        let capture = self
            .errors
            .get(&id)
            .ok_or(RunReadError::MissingErrorCapture(id))?;
        let ContextRef::Normal(key) = capture.throw_context_ref else {
            return Ok(ErrorStack::StackIncomplete {
                throw_function_id: capture.throw_function_id,
                throw_site: capture.throw_site,
            });
        };
        Ok(ErrorStack::Complete(context_chain(&self.contexts, key)?))
    }
}

/// Root-first parent chain of `key`. A well-formed chain visits each context
/// at most once, so walking more than `contexts.len()` steps proves a cycle
/// without a visited set on the read path.
fn context_chain(
    contexts: &HashMap<ContextKey, MergedContext>,
    mut key: ContextKey,
) -> Result<Vec<ContextTuple>, RunReadError> {
    let mut stack = Vec::new();
    let max_depth = contexts.len();
    loop {
        let context = contexts
            .get(&key)
            .ok_or(RunReadError::MissingContextDefinition(key))?;
        let tuple = context
            .tuple
            .ok_or(RunReadError::MissingContextDefinition(key))?;
        stack.push(tuple);
        let Some(parent) = tuple.parent_context_key else {
            break;
        };
        if stack.len() >= max_depth {
            return Err(RunReadError::CyclicContextChain(key));
        }
        key = parent;
    }
    stack.reverse();
    Ok(stack)
}

fn validate_fence(kind: SegmentKind, fence: SegmentHighWater) -> Result<(), RunReadError> {
    if fence.last_sequence != fence.segment_count {
        return Err(RunReadError::InvalidFence { kind, fence });
    }
    Ok(())
}

fn segment_path(run_directory: &Path, kind: SegmentKind, sequence: u64) -> PathBuf {
    let (directory, extension) = match kind {
        SegmentKind::Cct => ("cct", "bamlcct"),
        SegmentKind::Evidence => ("evidence", "bamlspans"),
    };
    run_directory
        .join(directory)
        .join(format!("{sequence:020}.{extension}"))
}

fn read(path: &Path) -> Result<Vec<u8>, RunReadError> {
    fs::read(path).map_err(|source| RunReadError::Io {
        path: path.to_owned(),
        source,
    })
}

fn add_counters(target: &mut CctCounters, delta: CctCounters, health: &mut CounterHealth) {
    macro_rules! add {
        ($field:ident) => {
            match target.$field.checked_add(delta.$field) {
                Some(value) => target.$field = value,
                None => {
                    target.$field = target.$field.saturating_add(delta.$field);
                    health.counter_saturated = true;
                }
            }
        };
    }
    add!(invocations_started);
    add!(spans_selected);
    add!(completed_ok);
    add!(completed_error);
    add!(completed_cancelled);
    add!(completed_exit);
    add!(inclusive_ns);
    add!(direct_call_child_inclusive_ns);
    match target.await_ns.checked_add(delta.await_ns) {
        Some(value) => target.await_ns = value,
        None => {
            target.await_ns = u128::MAX;
            health.await_counter_saturated = true;
        }
    }
    match target.await_count.checked_add(delta.await_count) {
        Some(value) => target.await_count = value,
        None => {
            target.await_count = u64::MAX;
            health.await_counter_saturated = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{ContextKey, ContextTuple, MergedContext, RunReadError, context_chain};
    use crate::{
        ids::{FunctionId, ProgramId},
        prof::backend::{CctCounters, EdgeKind},
    };

    fn context(parent: Option<ContextKey>, edge_kind: EdgeKind) -> MergedContext {
        MergedContext {
            tuple: Some(ContextTuple {
                program_id: ProgramId([0; 16]),
                parent_context_key: parent,
                function_id: FunctionId(1),
                call_site: None,
                edge_kind,
            }),
            counters: CctCounters::default(),
        }
    }

    #[test]
    fn context_chain_walks_root_first() {
        let root = ContextKey([1; 32]);
        let child = ContextKey([2; 32]);
        let contexts = HashMap::from([
            (root, context(None, EdgeKind::Root)),
            (child, context(Some(root), EdgeKind::Call)),
        ]);
        let chain = context_chain(&contexts, child).unwrap();
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].edge_kind, EdgeKind::Root);
        assert_eq!(chain[1].parent_context_key, Some(root));
    }

    #[test]
    fn context_chain_rejects_cycles_instead_of_hanging() {
        let a = ContextKey([1; 32]);
        let b = ContextKey([2; 32]);
        let contexts = HashMap::from([
            (a, context(Some(b), EdgeKind::Call)),
            (b, context(Some(a), EdgeKind::Call)),
        ]);
        assert!(matches!(
            context_chain(&contexts, a),
            Err(RunReadError::CyclicContextChain(_))
        ));
        let contexts = HashMap::from([(a, context(Some(a), EdgeKind::Call))]);
        assert!(matches!(
            context_chain(&contexts, a),
            Err(RunReadError::CyclicContextChain(_))
        ));
    }
}
