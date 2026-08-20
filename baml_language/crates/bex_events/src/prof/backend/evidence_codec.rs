//! Version-1 exact-evidence segment payload codec.

use super::{
    CodecVersion, ContextRef, EdgeKind, ErrorCapture, ErrorCodecError, RoleMask,
    RuntimeIdAnnotation, SelectionReasons, SpanEnd, SpanRuntimeId, SpanStart, TerminalErrorRef,
    ValueCid, ValueLossReason, ValueOccurrence, ValueRole, ValueState, decode_error_capture,
    decode_terminal_error_ref, encode_error_capture, encode_terminal_error_ref,
};
use crate::{
    ids::{
        BexCallId, BexThreadId, BoundaryId, CallRef, EngineId, FunctionId, ProcessEuid, ThreadRef,
    },
    prof::record::{CallSiteSourceSpan, FunctionEndStatus},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvidenceFact {
    SpanStart(SpanStart),
    SpanEnd(SpanEnd),
    SpanRuntimeId(SpanRuntimeId),
    ValueOccurrence(ValueOccurrence),
    ErrorCapture(ErrorCapture),
    TerminalErrorRef(TerminalErrorRef),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EncodedEvidenceBatch {
    pub record_count: u64,
    pub payload: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvidenceCodecError {
    Truncated,
    InvalidTag,
    InvalidBits,
    InvalidNestedError,
    CountOverflow,
    RecordCountMismatch,
    TrailingBytes,
}

#[must_use]
pub(crate) fn encode_evidence_facts(facts: &[EvidenceFact]) -> EncodedEvidenceBatch {
    let mut payload = Vec::with_capacity(facts.len().saturating_mul(160).saturating_add(8));
    payload.extend_from_slice(&u64::try_from(facts.len()).unwrap_or(u64::MAX).to_be_bytes());
    for fact in facts {
        let (tag, body) = encode_fact(fact);
        payload.push(tag);
        payload.extend_from_slice(&u32::try_from(body.len()).unwrap_or(u32::MAX).to_be_bytes());
        payload.extend_from_slice(&body);
    }
    EncodedEvidenceBatch {
        record_count: u64::try_from(facts.len()).unwrap_or(u64::MAX),
        payload,
    }
}

pub(crate) fn decode_evidence_payload(
    payload: &[u8],
    record_count: u64,
) -> Result<Vec<EvidenceFact>, EvidenceCodecError> {
    let mut cursor = Cursor::new(payload);
    let count = cursor.usize_count()?;
    if count > cursor.remaining_len() / 5 {
        return Err(EvidenceCodecError::Truncated);
    }
    let mut facts = Vec::with_capacity(count);
    for _ in 0..count {
        let tag = cursor.u8()?;
        let length =
            usize::try_from(cursor.u32()?).map_err(|_| EvidenceCodecError::CountOverflow)?;
        let body = cursor.take(length)?;
        facts.push(decode_fact(tag, body)?);
    }
    if !cursor.is_empty() {
        return Err(EvidenceCodecError::TrailingBytes);
    }
    if u64::try_from(count).map_err(|_| EvidenceCodecError::CountOverflow)? != record_count {
        return Err(EvidenceCodecError::RecordCountMismatch);
    }
    Ok(facts)
}

fn encode_fact(fact: &EvidenceFact) -> (u8, Vec<u8>) {
    match fact {
        EvidenceFact::SpanStart(span) => (0, encode_span_start(*span)),
        EvidenceFact::SpanEnd(span) => (1, encode_span_end(*span)),
        EvidenceFact::SpanRuntimeId(annotation) => (2, encode_runtime_id(*annotation)),
        EvidenceFact::ValueOccurrence(occurrence) => (3, encode_value_occurrence(*occurrence)),
        EvidenceFact::ErrorCapture(capture) => (4, encode_error_capture(capture)),
        EvidenceFact::TerminalErrorRef(terminal) => (5, encode_terminal_error_ref(terminal)),
    }
}

fn decode_fact(tag: u8, body: &[u8]) -> Result<EvidenceFact, EvidenceCodecError> {
    match tag {
        0 => decode_span_start(body).map(EvidenceFact::SpanStart),
        1 => decode_span_end(body).map(EvidenceFact::SpanEnd),
        2 => decode_runtime_id(body).map(EvidenceFact::SpanRuntimeId),
        3 => decode_value_occurrence(body).map(EvidenceFact::ValueOccurrence),
        4 => decode_error_capture(body)
            .map(EvidenceFact::ErrorCapture)
            .map_err(map_error_codec),
        5 => decode_terminal_error_ref(body)
            .map(EvidenceFact::TerminalErrorRef)
            .map_err(map_error_codec),
        _ => Err(EvidenceCodecError::InvalidTag),
    }
}

fn encode_span_start(span: SpanStart) -> Vec<u8> {
    let mut body = Vec::with_capacity(192);
    body.extend_from_slice(&span.boundary_id.as_bytes());
    encode_call_ref(&mut body, span.call_ref);
    encode_optional_call_ref(&mut body, span.parent_call_ref);
    encode_thread_ref(&mut body, span.thread_ref);
    encode_context_ref(&mut body, span.context_ref);
    body.extend_from_slice(&span.function_id.0.to_be_bytes());
    encode_call_site(&mut body, span.call_site);
    body.push(span.edge_kind as u8);
    body.extend_from_slice(&span.started_ns.to_be_bytes());
    body.push(span.selection_reasons.bits());
    body.push(span.roles.bits());
    match span.runtime_id {
        None => body.push(0),
        Some(annotation) => {
            body.push(1);
            body.extend_from_slice(&annotation.annotation_ordinal.to_be_bytes());
            body.extend_from_slice(&annotation.runtime_id.as_bytes());
        }
    }
    body
}

fn decode_span_start(bytes: &[u8]) -> Result<SpanStart, EvidenceCodecError> {
    let mut cursor = Cursor::new(bytes);
    let boundary_id = BoundaryId::from_bytes(cursor.array()?);
    let call_ref = decode_call_ref(&mut cursor)?;
    let parent_call_ref = decode_optional_call_ref(&mut cursor)?;
    let thread_ref = decode_thread_ref(&mut cursor)?;
    let context_ref = decode_context_ref(&mut cursor)?;
    let function_id = FunctionId(cursor.u32()?);
    let call_site = decode_call_site(&mut cursor)?;
    let edge_kind = decode_edge(cursor.u8()?)?;
    let started_ns = cursor.u64()?;
    let selection_reasons =
        SelectionReasons::from_bits(cursor.u8()?).ok_or(EvidenceCodecError::InvalidBits)?;
    let roles = RoleMask::from_bits(cursor.u8()?).ok_or(EvidenceCodecError::InvalidBits)?;
    let runtime_id = match cursor.u8()? {
        0 => None,
        1 => Some(RuntimeIdAnnotation {
            annotation_ordinal: cursor.u32()?,
            runtime_id: BoundaryId::from_bytes(cursor.array()?),
        }),
        _ => return Err(EvidenceCodecError::InvalidTag),
    };
    cursor.finish()?;
    Ok(SpanStart {
        boundary_id,
        call_ref,
        parent_call_ref,
        thread_ref,
        context_ref,
        function_id,
        call_site,
        edge_kind,
        started_ns,
        selection_reasons,
        roles,
        runtime_id,
    })
}

fn encode_span_end(span: SpanEnd) -> Vec<u8> {
    let mut body = Vec::with_capacity(64);
    encode_call_ref(&mut body, span.call_ref);
    body.extend_from_slice(&span.ended_ns.to_be_bytes());
    body.push(span.status as u8);
    body.extend_from_slice(&span.inclusive_ns.to_be_bytes());
    body
}

fn decode_span_end(bytes: &[u8]) -> Result<SpanEnd, EvidenceCodecError> {
    let mut cursor = Cursor::new(bytes);
    let span = SpanEnd {
        call_ref: decode_call_ref(&mut cursor)?,
        ended_ns: cursor.u64()?,
        status: decode_status(cursor.u8()?)?,
        inclusive_ns: cursor.u64()?,
    };
    cursor.finish()?;
    Ok(span)
}

fn encode_runtime_id(annotation: SpanRuntimeId) -> Vec<u8> {
    let mut body = Vec::with_capacity(60);
    encode_call_ref(&mut body, annotation.call_ref);
    body.extend_from_slice(&annotation.annotation_ordinal.to_be_bytes());
    body.extend_from_slice(&annotation.runtime_id.as_bytes());
    body
}

fn decode_runtime_id(bytes: &[u8]) -> Result<SpanRuntimeId, EvidenceCodecError> {
    let mut cursor = Cursor::new(bytes);
    let annotation = SpanRuntimeId {
        call_ref: decode_call_ref(&mut cursor)?,
        annotation_ordinal: cursor.u32()?,
        runtime_id: BoundaryId::from_bytes(cursor.array()?),
    };
    cursor.finish()?;
    Ok(annotation)
}

fn encode_value_occurrence(occurrence: ValueOccurrence) -> Vec<u8> {
    let mut body = Vec::with_capacity(128);
    encode_call_ref(&mut body, occurrence.call_ref);
    encode_context_ref(&mut body, occurrence.context_ref);
    body.push(match occurrence.role {
        ValueRole::Input => 0,
        ValueRole::Output => 1,
    });
    encode_value_state(&mut body, occurrence.state);
    body
}

fn decode_value_occurrence(bytes: &[u8]) -> Result<ValueOccurrence, EvidenceCodecError> {
    let mut cursor = Cursor::new(bytes);
    let occurrence = ValueOccurrence {
        call_ref: decode_call_ref(&mut cursor)?,
        context_ref: decode_context_ref(&mut cursor)?,
        role: match cursor.u8()? {
            0 => ValueRole::Input,
            1 => ValueRole::Output,
            _ => return Err(EvidenceCodecError::InvalidTag),
        },
        state: decode_value_state(&mut cursor)?,
    };
    cursor.finish()?;
    Ok(occurrence)
}

fn encode_optional_call_ref(output: &mut Vec<u8>, value: Option<CallRef>) {
    match value {
        None => output.push(0),
        Some(value) => {
            output.push(1);
            encode_call_ref(output, value);
        }
    }
}

fn decode_optional_call_ref(
    cursor: &mut Cursor<'_>,
) -> Result<Option<CallRef>, EvidenceCodecError> {
    match cursor.u8()? {
        0 => Ok(None),
        1 => decode_call_ref(cursor).map(Some),
        _ => Err(EvidenceCodecError::InvalidTag),
    }
}

fn encode_call_ref(output: &mut Vec<u8>, value: CallRef) {
    encode_thread_ref(
        output,
        ThreadRef {
            process_euid: value.process_euid,
            engine_id: value.engine_id,
            thread_id: value.thread_id,
        },
    );
    output.extend_from_slice(&value.call_id.0.to_be_bytes());
}

fn decode_call_ref(cursor: &mut Cursor<'_>) -> Result<CallRef, EvidenceCodecError> {
    let thread = decode_thread_ref(cursor)?;
    Ok(CallRef {
        process_euid: thread.process_euid,
        engine_id: thread.engine_id,
        thread_id: thread.thread_id,
        call_id: BexCallId(cursor.u64()?),
    })
}

fn encode_thread_ref(output: &mut Vec<u8>, value: ThreadRef) {
    output.extend_from_slice(&value.process_euid.0);
    output.extend_from_slice(&value.engine_id.0.to_be_bytes());
    output.extend_from_slice(&value.thread_id.0.to_be_bytes());
}

fn decode_thread_ref(cursor: &mut Cursor<'_>) -> Result<ThreadRef, EvidenceCodecError> {
    Ok(ThreadRef {
        process_euid: ProcessEuid(cursor.array()?),
        engine_id: EngineId(cursor.u64()?),
        thread_id: BexThreadId(cursor.u64()?),
    })
}

fn encode_context_ref(output: &mut Vec<u8>, context: ContextRef) {
    match context {
        ContextRef::Normal(key) => {
            output.push(0);
            output.extend_from_slice(&key.0);
        }
        ContextRef::Overflow {
            boundary,
            reason,
            edge_kind,
        } => {
            output.push(1);
            output.extend_from_slice(&boundary.process_euid.0);
            output.extend_from_slice(&boundary.engine_id.0.to_be_bytes());
            output.extend_from_slice(&boundary.boundary_id.as_bytes());
            output.push(match reason {
                super::OverflowReason::ContextMemoryUnavailableAfterDrain => 0,
                super::OverflowReason::InvalidParentContext => 1,
            });
            output.push(edge_kind as u8);
        }
    }
}

fn decode_context_ref(cursor: &mut Cursor<'_>) -> Result<ContextRef, EvidenceCodecError> {
    match cursor.u8()? {
        0 => Ok(ContextRef::Normal(super::ContextKey(cursor.array()?))),
        1 => Ok(ContextRef::Overflow {
            boundary: super::BoundaryRef {
                process_euid: ProcessEuid(cursor.array()?),
                engine_id: EngineId(cursor.u64()?),
                boundary_id: BoundaryId::from_bytes(cursor.array()?),
            },
            reason: match cursor.u8()? {
                0 => super::OverflowReason::ContextMemoryUnavailableAfterDrain,
                1 => super::OverflowReason::InvalidParentContext,
                _ => return Err(EvidenceCodecError::InvalidTag),
            },
            edge_kind: decode_edge(cursor.u8()?)?,
        }),
        _ => Err(EvidenceCodecError::InvalidTag),
    }
}

fn encode_call_site(output: &mut Vec<u8>, site: Option<CallSiteSourceSpan>) {
    match site {
        None => output.push(0),
        Some(site) => {
            output.push(1);
            for value in [site.file_id, site.start_offset, site.end_offset, site.line] {
                output.extend_from_slice(&value.to_be_bytes());
            }
        }
    }
}

fn decode_call_site(
    cursor: &mut Cursor<'_>,
) -> Result<Option<CallSiteSourceSpan>, EvidenceCodecError> {
    match cursor.u8()? {
        0 => Ok(None),
        1 => Ok(Some(CallSiteSourceSpan {
            file_id: cursor.u32()?,
            start_offset: cursor.u32()?,
            end_offset: cursor.u32()?,
            line: cursor.u32()?,
        })),
        _ => Err(EvidenceCodecError::InvalidTag),
    }
}

fn encode_value_state(output: &mut Vec<u8>, state: ValueState) {
    match state {
        ValueState::Available {
            cid,
            codec,
            encoded_bytes,
        } => {
            output.push(0);
            output.extend_from_slice(&cid.0);
            output.extend_from_slice(&codec.0.to_be_bytes());
            output.extend_from_slice(&encoded_bytes.to_be_bytes());
        }
        ValueState::Lost(reason) => {
            output.push(1);
            output.push(reason as u8);
        }
    }
}

fn decode_value_state(cursor: &mut Cursor<'_>) -> Result<ValueState, EvidenceCodecError> {
    match cursor.u8()? {
        0 => Ok(ValueState::Available {
            cid: ValueCid(cursor.array()?),
            codec: CodecVersion(cursor.u16()?),
            encoded_bytes: cursor.u64()?,
        }),
        1 => Ok(ValueState::Lost(decode_value_loss(cursor.u8()?)?)),
        _ => Err(EvidenceCodecError::InvalidTag),
    }
}

fn decode_value_loss(tag: u8) -> Result<ValueLossReason, EvidenceCodecError> {
    match tag {
        0 => Ok(ValueLossReason::ValueMemoryExceeded),
        1 => Ok(ValueLossReason::ValueAttemptTransportExceeded),
        2 => Ok(ValueLossReason::ErrorCaptureAttemptTransportExceeded),
        3 => Ok(ValueLossReason::ValueTooLarge),
        4 => Ok(ValueLossReason::CopyFailed),
        5 => Ok(ValueLossReason::EncodeFailed),
        6 => Ok(ValueLossReason::CasWriteFailed),
        7 => Ok(ValueLossReason::CasConflict),
        8 => Ok(ValueLossReason::DiskGuardExceeded),
        9 => Ok(ValueLossReason::EvidenceSegmentPublishFailed),
        10 => Ok(ValueLossReason::StoreUnavailable),
        _ => Err(EvidenceCodecError::InvalidTag),
    }
}

fn decode_edge(tag: u8) -> Result<EdgeKind, EvidenceCodecError> {
    match tag {
        0 => Ok(EdgeKind::Root),
        1 => Ok(EdgeKind::Call),
        2 => Ok(EdgeKind::Spawn),
        _ => Err(EvidenceCodecError::InvalidTag),
    }
}

fn decode_status(tag: u8) -> Result<FunctionEndStatus, EvidenceCodecError> {
    match tag {
        0 => Ok(FunctionEndStatus::Ok),
        1 => Ok(FunctionEndStatus::Errored),
        2 => Ok(FunctionEndStatus::Cancelled),
        3 => Ok(FunctionEndStatus::Exited),
        _ => Err(EvidenceCodecError::InvalidTag),
    }
}

fn map_error_codec(_error: ErrorCodecError) -> EvidenceCodecError {
    EvidenceCodecError::InvalidNestedError
}

struct Cursor<'a> {
    remaining: &'a [u8],
}

impl<'a> Cursor<'a> {
    fn new(remaining: &'a [u8]) -> Self {
        Self { remaining }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], EvidenceCodecError> {
        if self.remaining.len() < length {
            return Err(EvidenceCodecError::Truncated);
        }
        let (value, remaining) = self.remaining.split_at(length);
        self.remaining = remaining;
        Ok(value)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], EvidenceCodecError> {
        self.take(N)?
            .try_into()
            .map_err(|_| EvidenceCodecError::Truncated)
    }

    fn u8(&mut self) -> Result<u8, EvidenceCodecError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, EvidenceCodecError> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, EvidenceCodecError> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, EvidenceCodecError> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    fn usize_count(&mut self) -> Result<usize, EvidenceCodecError> {
        usize::try_from(self.u64()?).map_err(|_| EvidenceCodecError::CountOverflow)
    }

    fn remaining_len(&self) -> usize {
        self.remaining.len()
    }

    fn is_empty(&self) -> bool {
        self.remaining.is_empty()
    }

    fn finish(self) -> Result<(), EvidenceCodecError> {
        self.is_empty()
            .then_some(())
            .ok_or(EvidenceCodecError::TrailingBytes)
    }
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::prof::backend::{
        BoundaryRef, ContextKey, ErrorCaptureId, ErrorSource, ErrorUnwindKind, TerminalErrorTarget,
    };

    fn call(byte: u8) -> CallRef {
        CallRef {
            process_euid: ProcessEuid([byte; 16]),
            engine_id: EngineId(2),
            thread_id: BexThreadId(3),
            call_id: BexCallId(4),
        }
    }

    fn fixture() -> Vec<EvidenceFact> {
        let call_ref = call(1);
        let thread_ref = ThreadRef {
            process_euid: call_ref.process_euid,
            engine_id: call_ref.engine_id,
            thread_id: call_ref.thread_id,
        };
        let context_ref = ContextRef::Normal(ContextKey([5; 32]));
        let error_id = ErrorCaptureId {
            thread_ref,
            unwind_ordinal: 6,
        };
        vec![
            EvidenceFact::SpanStart(SpanStart {
                boundary_id: BoundaryId::from_bytes([7; 16]),
                call_ref,
                parent_call_ref: None,
                thread_ref,
                context_ref,
                function_id: FunctionId(8),
                call_site: Some(CallSiteSourceSpan {
                    file_id: 9,
                    start_offset: 10,
                    end_offset: 11,
                    line: 12,
                }),
                edge_kind: EdgeKind::Root,
                started_ns: 13,
                selection_reasons: SelectionReasons::from_bits(1).unwrap(),
                roles: RoleMask::ALL,
                runtime_id: Some(RuntimeIdAnnotation {
                    annotation_ordinal: 0,
                    runtime_id: BoundaryId::from_bytes([14; 16]),
                }),
            }),
            EvidenceFact::SpanRuntimeId(SpanRuntimeId {
                call_ref,
                annotation_ordinal: 1,
                runtime_id: BoundaryId::from_bytes([15; 16]),
            }),
            EvidenceFact::ValueOccurrence(ValueOccurrence {
                call_ref,
                context_ref,
                role: ValueRole::Input,
                state: ValueState::Lost(ValueLossReason::ValueMemoryExceeded),
            }),
            EvidenceFact::ErrorCapture(ErrorCapture {
                id: error_id,
                boundary_id: BoundaryId::from_bytes([7; 16]),
                throw_call_ref: call_ref,
                throw_context_ref: ContextRef::Overflow {
                    boundary: BoundaryRef {
                        process_euid: call_ref.process_euid,
                        engine_id: call_ref.engine_id,
                        boundary_id: BoundaryId::from_bytes([7; 16]),
                    },
                    reason: super::super::OverflowReason::InvalidParentContext,
                    edge_kind: EdgeKind::Call,
                },
                throw_function_id: FunctionId(8),
                throw_site: None,
                kind: ErrorUnwindKind::Fresh,
                source: ErrorSource::Bytecode,
                value: ValueState::Lost(ValueLossReason::CopyFailed),
            }),
            EvidenceFact::TerminalErrorRef(TerminalErrorRef {
                call_ref,
                target: TerminalErrorTarget::Capture(error_id),
            }),
            EvidenceFact::SpanEnd(SpanEnd {
                call_ref,
                ended_ns: 20,
                status: FunctionEndStatus::Errored,
                inclusive_ns: 7,
            }),
        ]
    }

    #[test]
    fn every_evidence_fact_round_trips_with_truncation_and_trailing_checks() {
        let facts = fixture();
        let encoded = encode_evidence_facts(&facts);
        assert_eq!(
            decode_evidence_payload(&encoded.payload, encoded.record_count),
            Ok(facts)
        );
        for cut in 0..encoded.payload.len() {
            assert_eq!(
                decode_evidence_payload(&encoded.payload[..cut], encoded.record_count),
                Err(EvidenceCodecError::Truncated),
                "cut at {cut}"
            );
        }
        let mut trailing = encoded.payload.clone();
        trailing.push(0);
        assert_eq!(
            decode_evidence_payload(&trailing, encoded.record_count),
            Err(EvidenceCodecError::TrailingBytes)
        );
        assert_eq!(
            decode_evidence_payload(&encoded.payload, encoded.record_count + 1),
            Err(EvidenceCodecError::RecordCountMismatch)
        );
    }

    #[test]
    fn evidence_payload_golden_checksum_is_cross_platform() {
        let encoded = encode_evidence_facts(&fixture());
        assert_eq!(
            hex::encode(Sha256::digest(&encoded.payload)),
            "c6c37276dc6acb4184c25966c1b9f2b72a8d5f231afa35fa8eff09d544aab8b1"
        );
        assert_eq!(encoded.record_count, 6);
    }
}
