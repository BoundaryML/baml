//! Target-neutral `.bamlprof` parsing helpers.
//!
//! `read_bamlprof(path)` remains a native convenience wrapper in `file.rs`.
//! Bytes and readers are shared so WASM hosts can replay artifacts without
//! pretending to have filesystem paths.

use std::io::{self, Read};

use prost::Message;

use crate::prof::pb;

/// A parsed `.bamlprof`.
pub struct BamlprofContents {
    /// The file header.
    pub header: pb::EventFileHeaderV1,
    /// Every whole event, in file order. Consumers sort or reconstruct by
    /// event timestamps where needed.
    pub events: Vec<pb::DiskEventV1>,
    /// True when the artifact ended mid-message. `events` keeps the
    /// whole-message prefix so live or crashed writers can still be inspected.
    pub truncated: bool,
}

pub fn read_bamlprof_from_reader(mut reader: impl Read) -> io::Result<BamlprofContents> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    read_bamlprof_from_bytes(&bytes)
}

pub fn read_bamlprof_from_bytes(bytes: &[u8]) -> io::Result<BamlprofContents> {
    let mut buf = bytes;
    let header = pb::EventFileHeaderV1::decode_length_delimited(&mut buf)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut events = Vec::new();
    let mut truncated = false;

    while !buf.is_empty() {
        let delimiter_len = buf.len();
        let frame_len = match prost::encoding::decode_length_delimiter(&mut buf) {
            Ok(frame_len) => frame_len,
            Err(err) => {
                if delimiter_len < 10 {
                    truncated = true;
                    break;
                }
                return Err(io::Error::new(io::ErrorKind::InvalidData, err));
            }
        };
        if buf.len() < frame_len {
            truncated = true;
            break;
        }
        let (frame, rest) = buf.split_at(frame_len);
        let event = pb::DiskEventV1::decode(frame)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        events.push(event);
        buf = rest;
    }

    Ok(BamlprofContents {
        header,
        events,
        truncated,
    })
}

/// Decodes the header's 16-byte little-endian wall anchor.
#[must_use]
pub fn header_started_at_epoch_ns(header: &pb::EventFileHeaderV1) -> Option<u128> {
    let bytes: [u8; 16] = header.started_at_epoch_ns.as_slice().try_into().ok()?;
    Some(u128::from_le_bytes(bytes))
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

    #[test]
    fn read_bamlprof_from_bytes_and_reader_parse_same_framing() {
        let header = fixed_header();
        let events = fixed_events();
        let mut bytes = Vec::new();
        encode_length_delimited_message(&mut bytes, &header).unwrap();
        for event in &events {
            encode_disk_event(&mut bytes, event);
        }

        let from_bytes = super::read_bamlprof_from_bytes(&bytes).unwrap();
        assert_eq!(from_bytes.header.engine_id, 7);
        assert_eq!(from_bytes.events.len(), 2);
        assert!(!from_bytes.truncated);

        let from_reader = super::read_bamlprof_from_reader(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(from_reader.header.engine_id, 7);
        assert_eq!(from_reader.events.len(), 2);
        assert!(!from_reader.truncated);
    }

    #[test]
    fn byte_sink_artifact_parses_through_shared_reader() {
        let header = fixed_header();
        let events = fixed_events();

        let mut expected = Vec::new();
        encode_length_delimited_message(&mut expected, &header).unwrap();
        for event in &events {
            encode_disk_event(&mut expected, event);
        }

        let mut sink = ByteProfileArtifactSink::new();
        let mut header_chunk = Vec::new();
        encode_length_delimited_message(&mut header_chunk, &header).unwrap();
        sink.write_chunk(&header_chunk).unwrap();

        let mut event_chunk = Vec::new();
        for event in &events {
            encode_disk_event(&mut event_chunk, event);
        }
        sink.write_chunk(&event_chunk).unwrap();

        assert_eq!(
            sink.flush().unwrap(),
            ProfileArtifactRef::Bytes {
                len: expected.len(),
                truncated: false,
                dropped_bytes: 0,
                dropped_chunks: 0
            }
        );
        assert_eq!(sink.bytes(), expected);

        let parsed = super::read_bamlprof_from_bytes(sink.bytes()).unwrap();
        assert_eq!(parsed.header.engine_id, header.engine_id);
        assert_eq!(parsed.events, events);
        assert!(!parsed.truncated);
    }

    #[test]
    fn trailing_partial_event_frame_is_reported_as_truncated() {
        let header = fixed_header();
        let mut bytes = Vec::new();
        encode_length_delimited_message(&mut bytes, &header).unwrap();
        bytes.push(0x80);

        let parsed = super::read_bamlprof_from_bytes(&bytes).unwrap();
        assert!(parsed.truncated);
        assert!(parsed.events.is_empty());
    }

    #[test]
    fn malformed_complete_event_frame_is_invalid_data() {
        let header = fixed_header();
        let mut bytes = Vec::new();
        encode_length_delimited_message(&mut bytes, &header).unwrap();
        bytes.extend_from_slice(&[1, 0]);

        let Err(error) = super::read_bamlprof_from_bytes(&bytes) else {
            panic!("malformed event frame should fail");
        };
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }
}
