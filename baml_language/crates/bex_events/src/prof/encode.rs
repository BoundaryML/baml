//! Target-neutral `.bamlprof` encoding helpers.
//!
//! Native file output and WASM byte/chunk output must share the same protobuf
//! byte contract. File paths, profile directories, and background consumer
//! threads live in native adapters; these helpers only encode headers/events.

use prost::Message;

use crate::prof::pb;

/// Encode any protobuf message with `.bamlprof` length-delimited framing.
pub fn encode_length_delimited_message(
    out: &mut Vec<u8>,
    msg: &impl Message,
) -> Result<(), prost::EncodeError> {
    msg.encode_length_delimited(out)
}

/// Append one length-delimited profile event to `out`.
///
/// The two hot records - `CallFunction` and `EndFunction` - use the
/// hand-rolled encoder below. Cold records stay on prost.
pub fn encode_disk_event(out: &mut Vec<u8>, event: &pb::DiskEventV1) {
    use pb::disk_event_v1::Event;
    match &event.event {
        Some(Event::CallFunction(cf)) => encode_call_function_event(out, cf),
        Some(Event::EndFunction(ef)) => encode_end_function_event(out, ef),
        _ => {
            event
                .encode_length_delimited(out)
                .expect("protobuf encode into a Vec never runs out of capacity");
        }
    }
}

// These emit a length-delimited `DiskEventV1{ call_function | end_function }`
// byte-for-byte as prost would, in one pass. Both messages are < 128 bytes
// (<= 5 small scalar fields), so the outer (`DiskEventV1`) and inner length
// prefixes are each a single varint byte we back-patch. Field numbers and
// wire types come from `bamlprof.proto`.

#[inline]
#[expect(
    clippy::cast_possible_truncation,
    reason = "varint extracts the low 7 bits per byte"
)]
fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

#[inline]
fn put_scalar(out: &mut Vec<u8>, key: u8, v: u64) {
    if v != 0 {
        out.push(key);
        put_varint(out, v);
    }
}

#[inline]
#[expect(
    clippy::cast_possible_truncation,
    reason = "messages are < 128 bytes; asserted"
)]
fn patch_lengths(out: &mut [u8], outer_at: usize, inner_len_at: usize, inner_start: usize) {
    let inner_len = out.len() - inner_start;
    debug_assert!(
        inner_len < 128,
        "event message exceeds single-byte length prefix"
    );
    out[inner_len_at] = inner_len as u8;

    let outer_len = out.len() - outer_at - 1;
    debug_assert!(
        outer_len < 128,
        "DiskEventV1 exceeds single-byte length prefix"
    );
    out[outer_at] = outer_len as u8;
}

fn encode_call_function_event(out: &mut Vec<u8>, cf: &pb::CallFunction) {
    let outer_at = out.len();
    out.push(0); // DiskEventV1 length (patched)
    out.push(0x1A); // field 3 (call_function), wire-type 2 (LEN)
    let inner_len_at = out.len();
    out.push(0); // CallFunction length (patched)
    let inner_start = out.len();

    put_scalar(out, 0x08, cf.thread_id);
    put_scalar(out, 0x10, cf.call_id);
    if let Some(parent_call_id) = cf.parent_call_id {
        out.push(0x18);
        put_varint(out, parent_call_id);
    }
    put_scalar(out, 0x20, u64::from(cf.function_id));
    put_scalar(out, 0x28, cf.timestamp_ns);
    if let Some(call_site_file_id) = cf.call_site_file_id {
        out.push(0x30);
        put_varint(out, u64::from(call_site_file_id));
    }
    if let Some(call_site_start_offset) = cf.call_site_start_offset {
        out.push(0x38);
        put_varint(out, u64::from(call_site_start_offset));
    }
    if let Some(call_site_end_offset) = cf.call_site_end_offset {
        out.push(0x40);
        put_varint(out, u64::from(call_site_end_offset));
    }
    if let Some(call_site_line) = cf.call_site_line {
        out.push(0x48);
        put_varint(out, u64::from(call_site_line));
    }

    patch_lengths(out, outer_at, inner_len_at, inner_start);
}

fn encode_end_function_event(out: &mut Vec<u8>, ef: &pb::EndFunction) {
    let outer_at = out.len();
    out.push(0); // DiskEventV1 length (patched)
    out.push(0x2A); // field 5 (end_function), wire-type 2 (LEN)
    let inner_len_at = out.len();
    out.push(0); // EndFunction length (patched)
    let inner_start = out.len();

    put_scalar(out, 0x08, ef.thread_id);
    put_scalar(out, 0x10, ef.call_id);
    if ef.status != 0 {
        out.push(0x18);
        put_varint(out, u64::try_from(ef.status).unwrap_or(0));
    }
    put_scalar(out, 0x20, ef.timestamp_ns);

    patch_lengths(out, outer_at, inner_len_at, inner_start);
}

/// Builds the header message from registered engine metadata.
pub fn build_header(
    process_id: [u8; 16],
    engine_id: u64,
    started_at_epoch_ns: u128,
    meta: Option<&crate::prof::EngineProfileMetadata>,
    clock: &crate::prof::clock::TickConverter,
) -> pb::EventFileHeaderV1 {
    use crate::prof::clock::{ClockKind, ClockQuality};
    let (tick_ns_numer, tick_ns_denom) = clock.rate();
    pb::EventFileHeaderV1 {
        process_id: process_id.to_vec(),
        engine_id,
        program_id: meta.map(|m| m.program_id.clone()).unwrap_or_default(),
        started_at_epoch_ns: started_at_epoch_ns.to_le_bytes().to_vec(),
        clock_kind: match clock.kind() {
            ClockKind::Tsc => pb::ClockKind::Tsc,
            ClockKind::Cntvct => pb::ClockKind::Cntvct,
            ClockKind::Instant => pb::ClockKind::Instant,
            ClockKind::Stub => pb::ClockKind::Stub,
        } as i32,
        tick_ns_numer,
        tick_ns_denom,
        clock_quality: match clock.quality() {
            ClockQuality::Exact => pb::ClockQuality::Exact,
            ClockQuality::Calibrated => pb::ClockQuality::Calibrated,
            ClockQuality::Coarse => pb::ClockQuality::Coarse,
        } as i32,
        source_snapshot_id: meta.and_then(|m| m.source_snapshot_id.clone()),
        revision_id: meta.and_then(|m| m.revision_id.clone()),
        function_table: Some(pb::FunctionMetadataTable {
            functions: meta
                .map(|m| {
                    m.functions
                        .iter()
                        .map(|f| pb::FunctionMetadata {
                            function_id: f.function_id,
                            fqn: f.fqn.clone(),
                            source_file: f.source_file.clone(),
                            span_start: f.span_start,
                            span_end: f.span_end,
                            kind: f.kind.clone(),
                            definition_key: f.definition_key.clone(),
                            owner_type: f.owner_type.clone(),
                            parent_function: f.parent_function.clone(),
                            lambda_path: f.lambda_path.clone(),
                            package_name: f.package_name.clone(),
                            namespace: f.namespace.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use prost::Message;

    use super::{encode_call_function_event, encode_disk_event, encode_end_function_event};
    use crate::prof::pb;

    /// The hand-rolled encoders must produce byte-for-byte the same output as
    /// prost's `encode_length_delimited` for every input. That equality is the
    /// wire contract, including proto field numbers.
    #[test]
    fn hand_encoder_matches_prost() {
        let call_fns = [
            (1u64, 1u64, None, 0u32, 0u64, None),
            (5, 7, Some(3), 9, 12_345, None),
            (u64::MAX, 1, Some(2), u32::MAX, u64::MAX, None),
            (0, 0, None, 0, 0, None),
            (128, 16_384, Some(2_097_152), 100_000, 1 << 40, None),
            (9, 7, Some(0), 4, 1, None),
            (1, 2, Some(1), 3, 4, Some((0, 10, 20, 7))),
            (1, 2, Some(1), 3, 4, Some((u32::MAX, 0, 0, 0))),
        ];

        for &(thread_id, call_id, parent_call_id, function_id, timestamp_ns, call_site) in &call_fns
        {
            let cf = pb::CallFunction {
                thread_id,
                call_id,
                parent_call_id,
                function_id,
                timestamp_ns,
                call_site_file_id: call_site.map(|span| span.0),
                call_site_start_offset: call_site.map(|span| span.1),
                call_site_end_offset: call_site.map(|span| span.2),
                call_site_line: call_site.map(|span| span.3),
            };
            let event = pb::DiskEventV1 {
                event: Some(pb::disk_event_v1::Event::CallFunction(cf)),
            };
            let mut prost_bytes = Vec::new();
            event.encode_length_delimited(&mut prost_bytes).unwrap();
            let mut hand = Vec::new();
            encode_call_function_event(&mut hand, &cf);
            assert_eq!(hand, prost_bytes, "CallFunction {cf:?}");
            let mut shared = Vec::new();
            encode_disk_event(&mut shared, &event);
            assert_eq!(shared, prost_bytes, "shared CallFunction {cf:?}");
        }

        for status in 0..4i32 {
            for &(thread_id, call_id, timestamp_ns) in
                &[(1u64, 1u64, 0u64), (9, 7, 12_345), (u64::MAX, 1, u64::MAX)]
            {
                let ef = pb::EndFunction {
                    thread_id,
                    call_id,
                    status,
                    timestamp_ns,
                };
                let event = pb::DiskEventV1 {
                    event: Some(pb::disk_event_v1::Event::EndFunction(ef)),
                };
                let mut prost_bytes = Vec::new();
                event.encode_length_delimited(&mut prost_bytes).unwrap();
                let mut hand = Vec::new();
                encode_end_function_event(&mut hand, &ef);
                assert_eq!(hand, prost_bytes, "EndFunction {ef:?}");
                let mut shared = Vec::new();
                encode_disk_event(&mut shared, &event);
                assert_eq!(shared, prost_bytes, "shared EndFunction {ef:?}");
            }
        }
    }
}
