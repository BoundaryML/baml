//! Shared exact-evidence domain types and frozen error-record codecs.
//!
//! The fact types are shared with producers on every target; the codecs are
//! consumer-side and wasm32 has no consumer.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use super::{
    CodecVersion, ContextRef, EdgeKind, OverflowReason, RoleMask, SelectionReasons, ValueCid,
};
use crate::ids::{BoundaryId, CallRef, FunctionId, ThreadRef};

const ERROR_CAPTURE_MAGIC: &[u8; 8] = b"BAMLERR1";
const TERMINAL_ERROR_MAGIC: &[u8; 8] = b"BAMLTER1";
const ERROR_CODEC_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueRole {
    Input,
    Output,
}

/// Frozen wire tags (`as u8`): both evidence codecs encode the discriminant
/// and decode through `ValueLossReason::from_tag`. Append only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ValueLossReason {
    ValueMemoryExceeded = 0,
    ValueAttemptTransportExceeded = 1,
    ErrorCaptureAttemptTransportExceeded = 2,
    ValueTooLarge = 3,
    CopyFailed = 4,
    EncodeFailed = 5,
    CasWriteFailed = 6,
    CasConflict = 7,
    DiskGuardExceeded = 8,
    EvidenceSegmentPublishFailed = 9,
    StoreUnavailable = 10,
}

impl ValueLossReason {
    pub(super) const ALL: [Self; 11] = [
        Self::ValueMemoryExceeded,
        Self::ValueAttemptTransportExceeded,
        Self::ErrorCaptureAttemptTransportExceeded,
        Self::ValueTooLarge,
        Self::CopyFailed,
        Self::EncodeFailed,
        Self::CasWriteFailed,
        Self::CasConflict,
        Self::DiskGuardExceeded,
        Self::EvidenceSegmentPublishFailed,
        Self::StoreUnavailable,
    ];

    /// The single decode table for the frozen wire tag.
    #[must_use]
    pub(super) fn from_tag(tag: u8) -> Option<Self> {
        Self::ALL.get(usize::from(tag)).copied()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueState {
    Available {
        cid: ValueCid,
        codec: CodecVersion,
        encoded_bytes: u64,
    },
    Lost(ValueLossReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ErrorCaptureId {
    pub thread_ref: ThreadRef,
    pub unwind_ordinal: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorUnwindKind {
    Fresh,
    Rethrow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorSource {
    Bytecode,
    NativeCall,
    EngineCall,
    FutureResume,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThrowSite {
    pub file_id: u32,
    pub line: u32,
    pub start_offset: u32,
    pub end_offset: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ErrorCapture {
    pub id: ErrorCaptureId,
    pub boundary_id: BoundaryId,
    pub throw_call_ref: CallRef,
    pub throw_context_ref: ContextRef,
    pub throw_function_id: FunctionId,
    pub throw_site: Option<ThrowSite>,
    pub kind: ErrorUnwindKind,
    pub source: ErrorSource,
    pub value: ValueState,
}

/// Producer-admitted unwind metadata. The structural consumer fills the
/// throwing context and the value pipeline completes `value` separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ErrorCaptureAttempt {
    pub id: ErrorCaptureId,
    pub throw_call_ref: CallRef,
    pub throw_function_id: FunctionId,
    pub first_selected_call_ref: CallRef,
    pub throw_site: Option<ThrowSite>,
    pub kind: ErrorUnwindKind,
    pub source: ErrorSource,
    pub manual_eligible: bool,
}

/// Frozen wire tags (`as u8`); decode through
/// `ErrorCaptureLossReason::from_tag`. Append only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ErrorCaptureLossReason {
    ErrorCaptureAttemptTransportExceeded = 0,
    MissingStructuralJoin = 1,
    StartUncommitted = 2,
    EvidenceQueueFull = 3,
    EvidenceSegmentPublishFailed = 4,
    DiskGuardExceeded = 5,
    StoreUnavailable = 6,
}

impl ErrorCaptureLossReason {
    pub(super) const ALL: [Self; 7] = [
        Self::ErrorCaptureAttemptTransportExceeded,
        Self::MissingStructuralJoin,
        Self::StartUncommitted,
        Self::EvidenceQueueFull,
        Self::EvidenceSegmentPublishFailed,
        Self::DiskGuardExceeded,
        Self::StoreUnavailable,
    ];

    /// The single decode table for the frozen wire tag.
    #[must_use]
    pub(super) fn from_tag(tag: u8) -> Option<Self> {
        Self::ALL.get(usize::from(tag)).copied()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalErrorTarget {
    Capture(ErrorCaptureId),
    Lost(ErrorCaptureLossReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalErrorRef {
    pub call_ref: CallRef,
    pub target: TerminalErrorTarget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeIdAnnotation {
    pub annotation_ordinal: u32,
    pub runtime_id: BoundaryId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpanStart {
    pub boundary_id: BoundaryId,
    pub call_ref: CallRef,
    pub parent_call_ref: Option<CallRef>,
    pub thread_ref: ThreadRef,
    pub context_ref: ContextRef,
    pub function_id: FunctionId,
    pub call_site: Option<crate::prof::record::CallSiteSourceSpan>,
    pub edge_kind: EdgeKind,
    pub started_ns: u64,
    pub selection_reasons: SelectionReasons,
    pub roles: RoleMask,
    pub runtime_id: Option<RuntimeIdAnnotation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpanRuntimeId {
    pub call_ref: CallRef,
    pub annotation_ordinal: u32,
    pub runtime_id: BoundaryId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValueOccurrence {
    pub call_ref: CallRef,
    pub context_ref: ContextRef,
    pub role: ValueRole,
    pub state: ValueState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpanEnd {
    pub call_ref: CallRef,
    pub ended_ns: u64,
    pub status: crate::prof::record::FunctionEndStatus,
    pub inclusive_ns: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ErrorCodecError {
    Truncated,
    InvalidMagic,
    UnsupportedVersion(u16),
    InvalidTag,
    TrailingBytes,
}

#[must_use]
pub(crate) fn encode_error_capture(record: &ErrorCapture) -> Vec<u8> {
    let mut out = Vec::with_capacity(192);
    out.extend_from_slice(ERROR_CAPTURE_MAGIC);
    put_u16(&mut out, ERROR_CODEC_VERSION);
    encode_error_id(&mut out, record.id);
    out.extend_from_slice(&record.boundary_id.as_bytes());
    encode_call_ref(&mut out, record.throw_call_ref);
    encode_context_ref(&mut out, record.throw_context_ref);
    put_u32(&mut out, record.throw_function_id.0);
    match record.throw_site {
        None => out.push(0),
        Some(site) => {
            out.push(1);
            put_u32(&mut out, site.file_id);
            put_u32(&mut out, site.line);
            put_u32(&mut out, site.start_offset);
            put_u32(&mut out, site.end_offset);
        }
    }
    out.push(match record.kind {
        ErrorUnwindKind::Fresh => 0,
        ErrorUnwindKind::Rethrow => 1,
    });
    out.push(match record.source {
        ErrorSource::Bytecode => 0,
        ErrorSource::NativeCall => 1,
        ErrorSource::EngineCall => 2,
        ErrorSource::FutureResume => 3,
    });
    encode_value_state(&mut out, record.value);
    out
}

pub(crate) fn decode_error_capture(bytes: &[u8]) -> Result<ErrorCapture, ErrorCodecError> {
    let mut cursor = Cursor::new(bytes);
    if cursor.array::<8>()? != *ERROR_CAPTURE_MAGIC {
        return Err(ErrorCodecError::InvalidMagic);
    }
    let version = cursor.u16()?;
    if version != ERROR_CODEC_VERSION {
        return Err(ErrorCodecError::UnsupportedVersion(version));
    }
    let record = ErrorCapture {
        id: decode_error_id(&mut cursor)?,
        boundary_id: BoundaryId::from_bytes(cursor.array()?),
        throw_call_ref: decode_call_ref(&mut cursor)?,
        throw_context_ref: decode_context_ref(&mut cursor)?,
        throw_function_id: FunctionId(cursor.u32()?),
        throw_site: match cursor.u8()? {
            0 => None,
            1 => Some(ThrowSite {
                file_id: cursor.u32()?,
                line: cursor.u32()?,
                start_offset: cursor.u32()?,
                end_offset: cursor.u32()?,
            }),
            _ => return Err(ErrorCodecError::InvalidTag),
        },
        kind: match cursor.u8()? {
            0 => ErrorUnwindKind::Fresh,
            1 => ErrorUnwindKind::Rethrow,
            _ => return Err(ErrorCodecError::InvalidTag),
        },
        source: match cursor.u8()? {
            0 => ErrorSource::Bytecode,
            1 => ErrorSource::NativeCall,
            2 => ErrorSource::EngineCall,
            3 => ErrorSource::FutureResume,
            _ => return Err(ErrorCodecError::InvalidTag),
        },
        value: decode_value_state(&mut cursor)?,
    };
    cursor.finish()?;
    Ok(record)
}

#[must_use]
pub(crate) fn encode_terminal_error_ref(record: &TerminalErrorRef) -> Vec<u8> {
    let mut out = Vec::with_capacity(96);
    out.extend_from_slice(TERMINAL_ERROR_MAGIC);
    put_u16(&mut out, ERROR_CODEC_VERSION);
    encode_call_ref(&mut out, record.call_ref);
    match record.target {
        TerminalErrorTarget::Capture(id) => {
            out.push(0);
            encode_error_id(&mut out, id);
        }
        TerminalErrorTarget::Lost(reason) => {
            out.push(1);
            out.push(error_loss_tag(reason));
        }
    }
    out
}

pub(crate) fn decode_terminal_error_ref(bytes: &[u8]) -> Result<TerminalErrorRef, ErrorCodecError> {
    let mut cursor = Cursor::new(bytes);
    if cursor.array::<8>()? != *TERMINAL_ERROR_MAGIC {
        return Err(ErrorCodecError::InvalidMagic);
    }
    let version = cursor.u16()?;
    if version != ERROR_CODEC_VERSION {
        return Err(ErrorCodecError::UnsupportedVersion(version));
    }
    let call_ref = decode_call_ref(&mut cursor)?;
    let target = match cursor.u8()? {
        0 => TerminalErrorTarget::Capture(decode_error_id(&mut cursor)?),
        1 => TerminalErrorTarget::Lost(decode_error_loss(cursor.u8()?)?),
        _ => return Err(ErrorCodecError::InvalidTag),
    };
    cursor.finish()?;
    Ok(TerminalErrorRef { call_ref, target })
}

fn encode_context_ref(out: &mut Vec<u8>, context: ContextRef) {
    match context {
        ContextRef::Normal(key) => {
            out.push(0);
            out.extend_from_slice(&key.0);
        }
        ContextRef::Overflow {
            boundary,
            reason,
            edge_kind,
        } => {
            out.push(1);
            out.extend_from_slice(&boundary.process_euid.0);
            put_u64(out, boundary.engine_id.0);
            out.extend_from_slice(&boundary.boundary_id.as_bytes());
            out.push(match reason {
                OverflowReason::ContextMemoryUnavailableAfterDrain => 0,
                OverflowReason::InvalidParentContext => 1,
            });
            out.push(edge_kind as u8);
        }
    }
}

fn decode_context_ref(cursor: &mut Cursor<'_>) -> Result<ContextRef, ErrorCodecError> {
    match cursor.u8()? {
        0 => Ok(ContextRef::Normal(super::ContextKey(cursor.array()?))),
        1 => {
            let process_euid = crate::ids::ProcessEuid(cursor.array()?);
            let engine_id = crate::ids::EngineId(cursor.u64()?);
            let boundary_id = BoundaryId::from_bytes(cursor.array()?);
            let reason = match cursor.u8()? {
                0 => OverflowReason::ContextMemoryUnavailableAfterDrain,
                1 => OverflowReason::InvalidParentContext,
                _ => return Err(ErrorCodecError::InvalidTag),
            };
            let edge_kind = match cursor.u8()? {
                0 => EdgeKind::Root,
                1 => EdgeKind::Call,
                2 => EdgeKind::Spawn,
                _ => return Err(ErrorCodecError::InvalidTag),
            };
            Ok(ContextRef::Overflow {
                boundary: super::BoundaryRef {
                    process_euid,
                    engine_id,
                    boundary_id,
                },
                reason,
                edge_kind,
            })
        }
        _ => Err(ErrorCodecError::InvalidTag),
    }
}

fn encode_error_id(out: &mut Vec<u8>, id: ErrorCaptureId) {
    encode_thread_ref(out, id.thread_ref);
    put_u64(out, id.unwind_ordinal);
}

fn decode_error_id(cursor: &mut Cursor<'_>) -> Result<ErrorCaptureId, ErrorCodecError> {
    Ok(ErrorCaptureId {
        thread_ref: decode_thread_ref(cursor)?,
        unwind_ordinal: cursor.u64()?,
    })
}

fn encode_thread_ref(out: &mut Vec<u8>, value: ThreadRef) {
    out.extend_from_slice(&value.process_euid.0);
    put_u64(out, value.engine_id.0);
    put_u64(out, value.thread_id.0);
}

fn decode_thread_ref(cursor: &mut Cursor<'_>) -> Result<ThreadRef, ErrorCodecError> {
    Ok(ThreadRef {
        process_euid: crate::ids::ProcessEuid(cursor.array()?),
        engine_id: crate::ids::EngineId(cursor.u64()?),
        thread_id: crate::ids::BexThreadId(cursor.u64()?),
    })
}

fn encode_call_ref(out: &mut Vec<u8>, value: CallRef) {
    encode_thread_ref(
        out,
        ThreadRef {
            process_euid: value.process_euid,
            engine_id: value.engine_id,
            thread_id: value.thread_id,
        },
    );
    put_u64(out, value.call_id.0);
}

fn decode_call_ref(cursor: &mut Cursor<'_>) -> Result<CallRef, ErrorCodecError> {
    let thread = decode_thread_ref(cursor)?;
    Ok(CallRef {
        process_euid: thread.process_euid,
        engine_id: thread.engine_id,
        thread_id: thread.thread_id,
        call_id: crate::ids::BexCallId(cursor.u64()?),
    })
}

fn encode_value_state(out: &mut Vec<u8>, state: ValueState) {
    match state {
        ValueState::Available {
            cid,
            codec,
            encoded_bytes,
        } => {
            out.push(0);
            out.extend_from_slice(&cid.0);
            put_u16(out, codec.0);
            put_u64(out, encoded_bytes);
        }
        ValueState::Lost(reason) => {
            out.push(1);
            out.push(value_loss_tag(reason));
        }
    }
}

fn decode_value_state(cursor: &mut Cursor<'_>) -> Result<ValueState, ErrorCodecError> {
    match cursor.u8()? {
        0 => Ok(ValueState::Available {
            cid: ValueCid(cursor.array()?),
            codec: CodecVersion(cursor.u16()?),
            encoded_bytes: cursor.u64()?,
        }),
        1 => Ok(ValueState::Lost(decode_value_loss(cursor.u8()?)?)),
        _ => Err(ErrorCodecError::InvalidTag),
    }
}

fn value_loss_tag(reason: ValueLossReason) -> u8 {
    reason as u8
}

fn decode_value_loss(tag: u8) -> Result<ValueLossReason, ErrorCodecError> {
    ValueLossReason::from_tag(tag).ok_or(ErrorCodecError::InvalidTag)
}

fn error_loss_tag(reason: ErrorCaptureLossReason) -> u8 {
    reason as u8
}

fn decode_error_loss(tag: u8) -> Result<ErrorCaptureLossReason, ErrorCodecError> {
    ErrorCaptureLossReason::from_tag(tag).ok_or(ErrorCodecError::InvalidTag)
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_be_bytes());
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], ErrorCodecError> {
        let end = self
            .offset
            .checked_add(N)
            .ok_or(ErrorCodecError::Truncated)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(ErrorCodecError::Truncated)?
            .try_into()
            .expect("fixed length checked");
        self.offset = end;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, ErrorCodecError> {
        Ok(self.array::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16, ErrorCodecError> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, ErrorCodecError> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, ErrorCodecError> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    fn finish(self) -> Result<(), ErrorCodecError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(ErrorCodecError::TrailingBytes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ids::{BexCallId, BexThreadId, EngineId, ProcessEuid},
        prof::backend::{BoundaryRef, ContextKey},
    };

    fn thread_ref() -> ThreadRef {
        ThreadRef {
            process_euid: ProcessEuid([1; 16]),
            engine_id: EngineId(2),
            thread_id: BexThreadId(3),
        }
    }

    /// Frozen-codec guard: every loss reason round-trips through its wire
    /// tag, the tags are dense from zero, and every other byte is rejected.
    #[test]
    fn loss_reason_tags_round_trip_and_reject_unknown_bytes() {
        for (index, reason) in ValueLossReason::ALL.iter().enumerate() {
            assert_eq!(*reason as u8, u8::try_from(index).unwrap());
            assert_eq!(value_loss_tag(*reason), *reason as u8);
            assert_eq!(decode_value_loss(*reason as u8), Ok(*reason));
        }
        for (index, reason) in ErrorCaptureLossReason::ALL.iter().enumerate() {
            assert_eq!(*reason as u8, u8::try_from(index).unwrap());
            assert_eq!(error_loss_tag(*reason), *reason as u8);
            assert_eq!(decode_error_loss(*reason as u8), Ok(*reason));
        }
        for tag in 0..=u8::MAX {
            assert_eq!(
                ValueLossReason::from_tag(tag).is_some(),
                usize::from(tag) < ValueLossReason::ALL.len()
            );
            assert_eq!(
                ErrorCaptureLossReason::from_tag(tag).is_some(),
                usize::from(tag) < ErrorCaptureLossReason::ALL.len()
            );
        }
    }

    fn call_ref(call_id: u64) -> CallRef {
        let thread = thread_ref();
        CallRef {
            process_euid: thread.process_euid,
            engine_id: thread.engine_id,
            thread_id: thread.thread_id,
            call_id: BexCallId(call_id),
        }
    }

    fn capture() -> ErrorCapture {
        ErrorCapture {
            id: ErrorCaptureId {
                thread_ref: thread_ref(),
                unwind_ordinal: 4,
            },
            boundary_id: BoundaryId::from_bytes([5; 16]),
            throw_call_ref: call_ref(6),
            throw_context_ref: ContextRef::Normal(ContextKey([7; 32])),
            throw_function_id: FunctionId(8),
            throw_site: Some(ThrowSite {
                file_id: 9,
                line: 10,
                start_offset: 11,
                end_offset: 12,
            }),
            kind: ErrorUnwindKind::Rethrow,
            source: ErrorSource::FutureResume,
            value: ValueState::Available {
                cid: ValueCid([13; 32]),
                codec: CodecVersion(14),
                encoded_bytes: 15,
            },
        }
    }

    #[test]
    fn error_capture_roundtrip_and_every_truncation_are_checked() {
        let capture = capture();
        let bytes = encode_error_capture(&capture);
        assert_eq!(decode_error_capture(&bytes), Ok(capture));
        for end in 0..bytes.len() {
            assert!(decode_error_capture(&bytes[..end]).is_err(), "end={end}");
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert_eq!(
            decode_error_capture(&trailing),
            Err(ErrorCodecError::TrailingBytes)
        );
    }

    #[test]
    fn terminal_error_roundtrip_covers_capture_and_loss_targets() {
        for target in [
            TerminalErrorTarget::Capture(capture().id),
            TerminalErrorTarget::Lost(ErrorCaptureLossReason::MissingStructuralJoin),
        ] {
            let record = TerminalErrorRef {
                call_ref: call_ref(20),
                target,
            };
            let bytes = encode_terminal_error_ref(&record);
            assert_eq!(decode_terminal_error_ref(&bytes), Ok(record));
            for end in 0..bytes.len() {
                assert!(decode_terminal_error_ref(&bytes[..end]).is_err());
            }
        }
    }

    #[test]
    fn overflow_context_error_roundtrips_without_fabricated_parent() {
        let mut record = capture();
        record.throw_context_ref = ContextRef::Overflow {
            boundary: BoundaryRef {
                process_euid: ProcessEuid([21; 16]),
                engine_id: EngineId(22),
                boundary_id: BoundaryId::from_bytes([23; 16]),
            },
            reason: OverflowReason::InvalidParentContext,
            edge_kind: EdgeKind::Spawn,
        };
        let bytes = encode_error_capture(&record);
        assert_eq!(decode_error_capture(&bytes), Ok(record));
    }

    #[test]
    fn error_record_codecs_have_cross_platform_goldens() {
        assert_eq!(
            hex::encode(encode_error_capture(&capture())),
            "42414d4c4552523100010101010101010101010101010101010100000000000000020000000000000003000000000000000405050505050505050505050505050505010101010101010101010101010101010000000000000002000000000000000300000000000000060007070707070707070707070707070707070707070707070707070707070707070000000801000000090000000a0000000b0000000c0103000d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d000e000000000000000f"
        );
        let terminal = TerminalErrorRef {
            call_ref: call_ref(20),
            target: TerminalErrorTarget::Capture(capture().id),
        };
        assert_eq!(
            hex::encode(encode_terminal_error_ref(&terminal)),
            "42414d4c544552310001010101010101010101010101010101010000000000000002000000000000000300000000000000140001010101010101010101010101010101000000000000000200000000000000030000000000000004"
        );
    }
}
