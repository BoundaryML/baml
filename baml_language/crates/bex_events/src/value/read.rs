//! Target-neutral `.bamlvalue` parsing helpers.

use std::io;

use prost::Message;

use crate::value::{LogRecord, ValueFileRecord, ValueRecord, pb};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BamlvalueContents {
    pub header: pb::ValueFileHeaderV1,
    pub records: Vec<ValueFileRecord>,
    pub truncated: bool,
}

pub fn read_bamlvalue_from_bytes(bytes: &[u8]) -> io::Result<BamlvalueContents> {
    let mut buf = bytes;
    let header = pb::ValueFileHeaderV1::decode_length_delimited(&mut buf)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut records = Vec::new();
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
        let record = pb::ValueRecordV1::decode(frame)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        records.push(file_record_from_proto(record)?);
        buf = rest;
    }

    Ok(BamlvalueContents {
        header,
        records,
        truncated,
    })
}

fn file_record_from_proto(record: pb::ValueRecordV1) -> io::Result<ValueFileRecord> {
    let has_run_started = record.run_started.is_some();
    let has_run_completed = record.run_completed.is_some();
    let has_lifecycle = has_run_started || has_run_completed;
    let has_log_event = record.log_event.is_some();
    let has_capture_loss = record.capture_loss.is_some();
    if has_run_started && has_run_completed {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "value record mixed run_started with run_completed lifecycle metadata",
        ));
    }
    if has_lifecycle
        && (record.metadata.is_some()
            || !record.body.is_empty()
            || record.capture.is_some()
            || has_log_event
            || has_capture_loss
            || record.blob.is_some())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "value record mixed body metadata with lifecycle metadata",
        ));
    }
    if let Some(started) = record.run_started {
        return Ok(ValueFileRecord::RunStarted(started.try_into()?));
    }
    if let Some(completed) = record.run_completed {
        return Ok(ValueFileRecord::RunCompleted(completed.try_into()?));
    }
    if let Some(loss) = record.capture_loss {
        if record.metadata.is_some() || has_log_event || record.capture.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "value record mixed capture loss metadata with body metadata",
            ));
        }
        return Ok(ValueFileRecord::CaptureLoss(loss.try_into()?));
    }
    let metadata = record.metadata.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "value record omitted metadata")
    })?;
    if has_log_event && record.capture.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "value record mixed log metadata with value capture metadata",
        ));
    }
    if let Some(log_event) = record.log_event {
        return Ok(ValueFileRecord::LogEvent(LogRecord {
            value_ref: metadata.try_into()?,
            body: record.body,
            blob_ref: record.blob.map(TryInto::try_into).transpose()?,
            event: log_event.try_into()?,
        }));
    }
    Ok(ValueFileRecord::CapturedValue(ValueRecord {
        value_ref: metadata.try_into()?,
        body: record.body,
        blob_ref: record.blob.map(TryInto::try_into).transpose()?,
        capture: record.capture.map(TryInto::try_into).transpose()?,
    }))
}

#[cfg(test)]
mod tests {
    use prost::Message as _;

    use crate::{
        ids::BoundaryId,
        value::{
            ValueCodec, ValueRecord, ValueRef,
            encode::{encode_header, encode_record},
            pb,
        },
    };

    fn run_started_proto() -> pb::RunStartedV1 {
        pb::RunStartedV1 {
            project_id: "project".to_string(),
            project_generation: 1,
            target: Some(pb::RunTargetV1 {
                target: Some(pb::run_target_v1::Target::Function(
                    pb::FunctionRunTargetV1 {
                        function_name: "user.Extract".to_string(),
                    },
                )),
            }),
            args_summary: None,
            options_summary: None,
            created_at_ms: 10,
            time_anchor: Some(pb::TimeAnchorV1 {
                epoch_created_at_ms: 10,
                trace_zero_ns: 0,
            }),
        }
    }

    fn run_completed_proto() -> pb::RunCompletedV1 {
        pb::RunCompletedV1 {
            status: pb::RunStatus::Succeeded as i32,
            completed_at_ms: 20,
            renderer_hint: None,
            error: None,
            cancellation: None,
            result_value_ref: None,
        }
    }

    fn value_metadata_proto() -> pb::ValueMetadataV1 {
        pb::ValueMetadataV1 {
            id: "value_1".to_string(),
            codec: pb::ValueCodec::BamlOutboundValue as i32,
            availability: pb::ValueAvailability::Available as i32,
            original_size_bytes: Some(3),
            retained_size_bytes: Some(3),
            diagnostic: None,
        }
    }

    fn assert_record_is_invalid(record: &pb::ValueRecordV1, expected: &str) {
        let mut bytes = Vec::new();
        encode_header(&mut bytes, BoundaryId::from_bytes([3; 16])).unwrap();
        record.encode_length_delimited(&mut bytes).unwrap();

        let Err(error) = super::read_bamlvalue_from_bytes(&bytes) else {
            panic!("mixed record should fail");
        };
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(
            error.to_string().contains(expected),
            "expected `{expected}` in `{error}`"
        );
    }

    #[test]
    fn trailing_partial_record_is_reported_as_truncated() {
        let mut bytes = Vec::new();
        encode_header(&mut bytes, BoundaryId::from_bytes([3; 16])).unwrap();
        let record = ValueRecord {
            value_ref: ValueRef::available("value_1", ValueCodec::BamlOutboundValue, 3, 3),
            body: vec![1, 2, 3],
            blob_ref: None,
            capture: None,
        };
        let start = bytes.len();
        encode_record(&mut bytes, &record).unwrap();
        bytes.truncate(start + 2);

        let parsed = super::read_bamlvalue_from_bytes(&bytes).unwrap();
        assert!(parsed.truncated);
        assert!(parsed.records.is_empty());
    }

    #[test]
    fn malformed_complete_record_is_invalid_data() {
        let mut bytes = Vec::new();
        encode_header(&mut bytes, BoundaryId::from_bytes([3; 16])).unwrap();
        bytes.extend_from_slice(&[1, 0]);

        let Err(error) = super::read_bamlvalue_from_bytes(&bytes) else {
            panic!("malformed complete record should fail");
        };
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn mixed_lifecycle_records_are_rejected() {
        assert_record_is_invalid(
            &pb::ValueRecordV1 {
                metadata: None,
                body: Vec::new(),
                capture: None,
                run_started: Some(run_started_proto()),
                run_completed: Some(run_completed_proto()),
                log_event: None,
                capture_loss: None,
                blob: None,
            },
            "mixed run_started with run_completed",
        );
    }

    #[test]
    fn lifecycle_records_with_value_body_fields_are_rejected() {
        for record in [
            pb::ValueRecordV1 {
                metadata: None,
                body: vec![1, 2, 3],
                capture: None,
                run_started: Some(run_started_proto()),
                run_completed: None,
                log_event: None,
                capture_loss: None,
                blob: None,
            },
            pb::ValueRecordV1 {
                metadata: None,
                body: Vec::new(),
                capture: Some(pb::ValueCaptureV1 {
                    kind: pb::ValueCaptureKind::RootOutput as i32,
                    call: Some(pb::TraceCallKeyV1 {
                        process_id: vec![1; 16],
                        engine_id: 1,
                        thread_id: 1,
                        call_id: 1,
                    }),
                }),
                run_started: None,
                run_completed: Some(run_completed_proto()),
                log_event: None,
                capture_loss: None,
                blob: None,
            },
            pb::ValueRecordV1 {
                metadata: None,
                body: Vec::new(),
                capture: None,
                run_started: Some(run_started_proto()),
                run_completed: None,
                log_event: None,
                capture_loss: None,
                blob: Some(pb::BlobRefV1 {
                    algorithm: "sha256".to_string(),
                    digest: "0".repeat(64),
                    size_bytes: 3,
                }),
            },
            pb::ValueRecordV1 {
                metadata: Some(value_metadata_proto()),
                body: Vec::new(),
                capture: None,
                run_started: None,
                run_completed: Some(run_completed_proto()),
                log_event: None,
                capture_loss: None,
                blob: None,
            },
        ] {
            assert_record_is_invalid(&record, "mixed body metadata with lifecycle metadata");
        }
    }
}
