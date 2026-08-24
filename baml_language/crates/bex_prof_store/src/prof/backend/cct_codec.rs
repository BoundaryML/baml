//! Version-1 payload codec for immutable CCT segments.

use super::{
    CctCounters, ContextDelta, ContextKey, ContextTuple, CounterHealth, EdgeKind, OverflowDelta,
    OverflowReason, SealedCctEpoch,
};
use crate::{
    ids::{FunctionId, ProgramId},
    prof::record::CallSiteSourceSpan,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EncodedCctBatch {
    pub record_count: u64,
    pub terminal_health: [u8; 1],
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CctSegmentData {
    pub contexts: Vec<ContextDelta>,
    pub overflow: Vec<OverflowDelta>,
    pub health: CounterHealth,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CctCodecError {
    Truncated,
    InvalidTag,
    InvalidHealth,
    InvalidContextKey,
    CountOverflow,
    RecordCountMismatch,
    TrailingBytes,
}

#[must_use]
pub(crate) fn encode_cct_epoch(epoch: &SealedCctEpoch) -> EncodedCctBatch {
    let record_count = u64::try_from(epoch.contexts.len())
        .ok()
        .and_then(|contexts| {
            u64::try_from(epoch.overflow.len())
                .ok()
                .and_then(|overflow| contexts.checked_add(overflow))
        })
        .unwrap_or(u64::MAX);
    let mut payload = Vec::with_capacity(
        epoch
            .contexts
            .len()
            .saturating_mul(192)
            .saturating_add(epoch.overflow.len().saturating_mul(128))
            .saturating_add(16),
    );
    payload.extend_from_slice(
        &u64::try_from(epoch.contexts.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    for context in &epoch.contexts {
        payload.extend_from_slice(&context.key.0);
        match context.tuple {
            None => payload.push(0),
            Some(tuple) => {
                payload.push(1);
                encode_tuple(&mut payload, tuple);
            }
        }
        encode_counters(&mut payload, context.counters);
    }
    payload.extend_from_slice(
        &u64::try_from(epoch.overflow.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    for overflow in &epoch.overflow {
        payload.push(match overflow.reason {
            OverflowReason::ContextMemoryUnavailableAfterDrain => 0,
            OverflowReason::InvalidParentContext => 1,
        });
        payload.push(overflow.edge_kind as u8);
        encode_counters(&mut payload, overflow.counters);
    }
    EncodedCctBatch {
        record_count,
        terminal_health: [encode_health(epoch.health)],
        payload,
    }
}

pub(crate) fn decode_cct_payload(
    payload: &[u8],
    record_count: u64,
    terminal_health: &[u8],
) -> Result<CctSegmentData, CctCodecError> {
    let health = decode_health(terminal_health)?;
    let mut input = Decoder::new(payload);
    let context_count = input.usize_count()?;
    if context_count > input.remaining_len() / 137 {
        return Err(CctCodecError::Truncated);
    }
    let mut contexts = Vec::with_capacity(context_count);
    for _ in 0..context_count {
        let key = ContextKey(input.array::<32>()?);
        let tuple = match input.u8()? {
            0 => None,
            1 => Some(decode_tuple(&mut input)?),
            _ => return Err(CctCodecError::InvalidTag),
        };
        if tuple.is_some_and(|tuple| ContextKey::for_tuple(&tuple) != key) {
            return Err(CctCodecError::InvalidContextKey);
        }
        contexts.push(ContextDelta {
            key,
            tuple,
            counters: decode_counters(&mut input)?,
        });
    }
    let overflow_count = input.usize_count()?;
    if overflow_count > input.remaining_len() / 106 {
        return Err(CctCodecError::Truncated);
    }
    let mut overflow = Vec::with_capacity(overflow_count);
    for _ in 0..overflow_count {
        let reason = match input.u8()? {
            0 => OverflowReason::ContextMemoryUnavailableAfterDrain,
            1 => OverflowReason::InvalidParentContext,
            _ => return Err(CctCodecError::InvalidTag),
        };
        let edge_kind = decode_edge(input.u8()?)?;
        overflow.push(OverflowDelta {
            reason,
            edge_kind,
            counters: decode_counters(&mut input)?,
        });
    }
    if !input.is_empty() {
        return Err(CctCodecError::TrailingBytes);
    }
    let decoded_count = u64::try_from(context_count)
        .ok()
        .and_then(|contexts| {
            u64::try_from(overflow_count)
                .ok()
                .and_then(|overflow| contexts.checked_add(overflow))
        })
        .ok_or(CctCodecError::CountOverflow)?;
    if decoded_count != record_count {
        return Err(CctCodecError::RecordCountMismatch);
    }
    Ok(CctSegmentData {
        contexts,
        overflow,
        health,
    })
}

fn encode_tuple(output: &mut Vec<u8>, tuple: ContextTuple) {
    output.extend_from_slice(&tuple.program_id.0);
    match tuple.parent_context_key {
        None => output.push(0),
        Some(parent) => {
            output.push(1);
            output.extend_from_slice(&parent.0);
        }
    }
    output.extend_from_slice(&tuple.function_id.0.to_be_bytes());
    match tuple.call_site {
        None => output.push(0),
        Some(site) => {
            output.push(1);
            output.extend_from_slice(&site.file_id.to_be_bytes());
            output.extend_from_slice(&site.start_offset.to_be_bytes());
            output.extend_from_slice(&site.end_offset.to_be_bytes());
            output.extend_from_slice(&site.line.to_be_bytes());
        }
    }
    output.push(tuple.edge_kind as u8);
}

fn decode_tuple(input: &mut Decoder<'_>) -> Result<ContextTuple, CctCodecError> {
    let program_id = ProgramId(input.array::<16>()?);
    let parent_context_key = match input.u8()? {
        0 => None,
        1 => Some(ContextKey(input.array::<32>()?)),
        _ => return Err(CctCodecError::InvalidTag),
    };
    let function_id = FunctionId(input.u32()?);
    let call_site = match input.u8()? {
        0 => None,
        1 => Some(CallSiteSourceSpan {
            file_id: input.u32()?,
            start_offset: input.u32()?,
            end_offset: input.u32()?,
            line: input.u32()?,
        }),
        _ => return Err(CctCodecError::InvalidTag),
    };
    let edge_kind = decode_edge(input.u8()?)?;
    Ok(ContextTuple {
        program_id,
        parent_context_key,
        function_id,
        call_site,
        edge_kind,
    })
}

fn decode_edge(tag: u8) -> Result<EdgeKind, CctCodecError> {
    match tag {
        0 => Ok(EdgeKind::Root),
        1 => Ok(EdgeKind::Call),
        2 => Ok(EdgeKind::Spawn),
        _ => Err(CctCodecError::InvalidTag),
    }
}

fn encode_counters(output: &mut Vec<u8>, counters: CctCounters) {
    for value in [
        counters.invocations_started,
        counters.spans_selected,
        counters.completed_ok,
        counters.completed_error,
        counters.completed_cancelled,
        counters.completed_exit,
    ] {
        output.extend_from_slice(&value.to_be_bytes());
    }
    for value in [
        counters.inclusive_ns,
        counters.direct_call_child_inclusive_ns,
        counters.await_ns,
    ] {
        output.extend_from_slice(&value.to_be_bytes());
    }
    output.extend_from_slice(&counters.await_count.to_be_bytes());
}

fn decode_counters(input: &mut Decoder<'_>) -> Result<CctCounters, CctCodecError> {
    Ok(CctCounters {
        invocations_started: input.u64()?,
        spans_selected: input.u64()?,
        completed_ok: input.u64()?,
        completed_error: input.u64()?,
        completed_cancelled: input.u64()?,
        completed_exit: input.u64()?,
        inclusive_ns: input.u128()?,
        direct_call_child_inclusive_ns: input.u128()?,
        await_ns: input.u128()?,
        await_count: input.u64()?,
    })
}

pub(crate) fn encode_health(health: CounterHealth) -> u8 {
    u8::from(health.counter_saturated)
        | (u8::from(health.await_counter_saturated) << 1)
        | (u8::from(health.self_time_underflow) << 2)
}

pub(crate) fn decode_health(bytes: &[u8]) -> Result<CounterHealth, CctCodecError> {
    let [bits] = bytes else {
        return Err(CctCodecError::InvalidHealth);
    };
    if bits & !0b111 != 0 {
        return Err(CctCodecError::InvalidHealth);
    }
    Ok(CounterHealth {
        counter_saturated: bits & 1 != 0,
        await_counter_saturated: bits & 2 != 0,
        self_time_underflow: bits & 4 != 0,
    })
}

struct Decoder<'a> {
    remaining: &'a [u8],
}

impl<'a> Decoder<'a> {
    fn new(remaining: &'a [u8]) -> Self {
        Self { remaining }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], CctCodecError> {
        if self.remaining.len() < length {
            return Err(CctCodecError::Truncated);
        }
        let (value, remaining) = self.remaining.split_at(length);
        self.remaining = remaining;
        Ok(value)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], CctCodecError> {
        self.take(N)?
            .try_into()
            .map_err(|_| CctCodecError::Truncated)
    }

    fn u8(&mut self) -> Result<u8, CctCodecError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, CctCodecError> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, CctCodecError> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    fn u128(&mut self) -> Result<u128, CctCodecError> {
        Ok(u128::from_be_bytes(self.array()?))
    }

    fn usize_count(&mut self) -> Result<usize, CctCodecError> {
        usize::try_from(self.u64()?).map_err(|_| CctCodecError::CountOverflow)
    }

    fn is_empty(&self) -> bool {
        self.remaining.is_empty()
    }

    fn remaining_len(&self) -> usize {
        self.remaining.len()
    }
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::prof::{
        backend::{
            ActiveCctEpoch, MeasuredLayouts, ParentContextRef, ProfilerMemoryGovernor,
            ProfilerSizingPolicy,
        },
        record::FunctionEndStatus,
    };

    fn fixture() -> SealedCctEpoch {
        let sizing = ProfilerSizingPolicy::derive(32 * 1024 * 1024, MeasuredLayouts::V1).unwrap();
        let memory = ProfilerMemoryGovernor::new(sizing, MeasuredLayouts::V1);
        let mut epoch = ActiveCctEpoch::new(
            ProgramId([0x11; 16]),
            MeasuredLayouts::V1.population_item_min_bytes,
        );
        let root = epoch.record_start(
            ParentContextRef::Root,
            FunctionId(7),
            Some(CallSiteSourceSpan {
                file_id: 1,
                start_offset: 2,
                end_offset: 3,
                line: 4,
            }),
            EdgeKind::Root,
            true,
            &memory,
        );
        epoch.record_end(root, FunctionEndStatus::Ok, 100, 20, 2);
        epoch.seal()
    }

    #[test]
    fn cct_payload_round_trips_and_validates_every_truncation() {
        let epoch = fixture();
        let encoded = encode_cct_epoch(&epoch);
        let decoded = decode_cct_payload(
            &encoded.payload,
            encoded.record_count,
            &encoded.terminal_health,
        )
        .unwrap();
        assert_eq!(decoded.contexts, epoch.contexts);
        assert_eq!(decoded.overflow, epoch.overflow);
        assert_eq!(decoded.health, epoch.health);
        for length in 0..encoded.payload.len() {
            assert!(matches!(
                decode_cct_payload(
                    &encoded.payload[..length],
                    encoded.record_count,
                    &encoded.terminal_health,
                ),
                Err(CctCodecError::Truncated)
            ));
        }
        let mut trailing = encoded.payload.clone();
        trailing.push(0);
        assert_eq!(
            decode_cct_payload(&trailing, encoded.record_count, &encoded.terminal_health,),
            Err(CctCodecError::TrailingBytes)
        );
    }

    #[test]
    fn cct_payload_golden_checksum_is_cross_platform() {
        let encoded = encode_cct_epoch(&fixture());
        assert_eq!(
            hex::encode(Sha256::digest(&encoded.payload)),
            "571002989e5472f82280e134f87406b886f9348b4af4304e65ab88742ec8dd7f"
        );
        assert_eq!(encoded.record_count, 1);
        assert_eq!(encoded.terminal_health, [0]);
    }

    #[test]
    fn tuple_key_and_header_counts_are_checked() {
        let encoded = encode_cct_epoch(&fixture());
        let mut corrupt_key = encoded.payload.clone();
        corrupt_key[8] ^= 1;
        assert_eq!(
            decode_cct_payload(&corrupt_key, encoded.record_count, &encoded.terminal_health,),
            Err(CctCodecError::InvalidContextKey)
        );
        assert_eq!(
            decode_cct_payload(
                &encoded.payload,
                encoded.record_count + 1,
                &encoded.terminal_health,
            ),
            Err(CctCodecError::RecordCountMismatch)
        );
        assert_eq!(
            decode_cct_payload(&encoded.payload, encoded.record_count, &[0x80]),
            Err(CctCodecError::InvalidHealth)
        );
    }
}
