//! `.bamlprof` file reading (v2 §4 framing).
//!
//! Framing: one length-delimited [`pb::EventFileHeaderV1`], then a stream of
//! length-delimited [`pb::DiskEventV1`] messages. The per-engine
//! `ProfileWriter` that used to append these files from the consumer was
//! deleted in P9 step 4 — flight dumps (`prof::consumer`) and the wasm
//! cooperative drain's byte sinks (`prof::artifact`) are the remaining
//! producers of this framing, both via the shared encoders in
//! [`crate::prof::encode`]. Reading stays forever: goldens, flight dumps,
//! and historical archives all parse through here.

use std::{fs, io, path::Path};

pub use crate::prof::read::{
    BamlprofContents, header_started_at_epoch_ns, read_bamlprof_from_bytes,
    read_bamlprof_from_reader,
};

/// Reads a `.bamlprof` back: the header and every whole event, tolerating a
/// torn trailing message. Errors only when the file or its header is
/// unreadable. The reader for tests, gates, and ad-hoc tooling - the M5
/// renderer supersedes it for real consumption.
pub fn read_bamlprof(path: &Path) -> io::Result<BamlprofContents> {
    let bytes = fs::read(path)?;
    read_bamlprof_from_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use crate::prof::{
        artifact::{ByteProfileArtifactSink, ProfileArtifactRef, ProfileArtifactSink},
        encode::{encode_disk_event, encode_length_delimited_message},
        pb,
    };

    fn fixed_header() -> pb::EventFileHeaderV1 {
        pb::EventFileHeaderV1 {
            process_id: vec![1; 16],
            engine_id: 7,
            program_id: "program".to_string(),
            started_at_epoch_ns: 123u128.to_le_bytes().to_vec(),
            function_table: None,
            clock_kind: pb::ClockKind::Instant as i32,
            tick_ns_numer: 1,
            tick_ns_denom: 1,
            clock_quality: pb::ClockQuality::Exact as i32,
            source_snapshot_id: None,
            revision_id: None,
        }
    }

    fn fixed_events() -> Vec<pb::DiskEventV1> {
        vec![
            pb::DiskEventV1 {
                event: Some(pb::disk_event_v1::Event::CallFunction(pb::CallFunction {
                    thread_id: 1,
                    call_id: 2,
                    parent_call_id: None,
                    function_id: 3,
                    timestamp_ns: 4,
                    call_site_file_id: None,
                    call_site_start_offset: None,
                    call_site_end_offset: None,
                    call_site_line: None,
                })),
            },
            pb::DiskEventV1 {
                event: Some(pb::disk_event_v1::Event::EndFunction(pb::EndFunction {
                    thread_id: 1,
                    call_id: 2,
                    status: pb::FunctionEndStatus::Ok as i32,
                    timestamp_ns: 9,
                })),
            },
        ]
    }

    /// The shared encoders + byte sink produce exactly the framing this
    /// module reads back (the byte contract the deleted native file writer
    /// used to co-assert; the encoding side is the surviving producer).
    #[test]
    fn byte_sink_output_reads_back() {
        let header = fixed_header();
        let events = fixed_events();

        let mut sink = ByteProfileArtifactSink::new();
        let mut shared_bytes = Vec::new();
        encode_length_delimited_message(&mut shared_bytes, &header).unwrap();
        for event in &events {
            encode_disk_event(&mut shared_bytes, event);
        }
        sink.write_chunk(&shared_bytes).unwrap();
        assert_eq!(
            sink.flush().unwrap(),
            ProfileArtifactRef::Bytes {
                len: shared_bytes.len(),
                truncated: false,
                dropped_bytes: 0,
                dropped_chunks: 0
            }
        );

        let parsed = super::read_bamlprof_from_bytes(sink.bytes()).unwrap();
        assert_eq!(parsed.header, header);
        assert_eq!(parsed.events, events);
        assert!(!parsed.truncated);
    }
}
