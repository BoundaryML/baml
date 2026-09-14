//! Target-neutral `.bamlvalue` encoding helpers.

use prost::Message;

use crate::{
    ids::BoundaryId,
    value::{
        CaptureLossRecord, LogRecord, RunCompletedRecord, RunStartedRecord, ValueFileRecord, pb,
    },
};

pub fn encode_header(out: &mut Vec<u8>, boundary_id: BoundaryId) -> Result<(), prost::EncodeError> {
    pb::ValueFileHeaderV1 {
        boundary_id: boundary_id.as_bytes().to_vec(),
    }
    .encode_length_delimited(out)
}

pub fn encode_log_event(out: &mut Vec<u8>, record: &LogRecord) -> Result<(), prost::EncodeError> {
    encode_file_record(out, &ValueFileRecord::LogEvent(record.clone()))
}

pub fn encode_capture_loss(
    out: &mut Vec<u8>,
    record: &CaptureLossRecord,
) -> Result<(), prost::EncodeError> {
    encode_file_record(out, &ValueFileRecord::CaptureLoss(record.clone()))
}

pub fn encode_run_started(
    out: &mut Vec<u8>,
    record: &RunStartedRecord,
) -> Result<(), prost::EncodeError> {
    encode_file_record(out, &ValueFileRecord::RunStarted(record.clone()))
}

pub fn encode_run_completed(
    out: &mut Vec<u8>,
    record: &RunCompletedRecord,
) -> Result<(), prost::EncodeError> {
    encode_file_record(out, &ValueFileRecord::RunCompleted(record.clone()))
}

pub fn encode_file_record(
    out: &mut Vec<u8>,
    record: &ValueFileRecord,
) -> Result<(), prost::EncodeError> {
    match record {
        ValueFileRecord::LogEvent(record) => pb::ValueRecordV1 {
            metadata: Some((&record.value_ref).into()),
            body: record.body.clone(),
            run_started: None,
            run_completed: None,
            log_event: Some((&record.event).into()),
            capture_loss: None,
            blob: record.blob_ref.as_ref().map(Into::into),
        },
        ValueFileRecord::CaptureLoss(record) => pb::ValueRecordV1 {
            metadata: None,
            body: Vec::new(),
            run_started: None,
            run_completed: None,
            log_event: None,
            capture_loss: Some(record.into()),
            blob: None,
        },
        ValueFileRecord::RunStarted(record) => pb::ValueRecordV1 {
            metadata: None,
            body: Vec::new(),
            run_started: Some(record.into()),
            run_completed: None,
            log_event: None,
            capture_loss: None,
            blob: None,
        },
        ValueFileRecord::RunCompleted(record) => pb::ValueRecordV1 {
            metadata: None,
            body: Vec::new(),
            run_started: None,
            run_completed: Some(record.into()),
            log_event: None,
            capture_loss: None,
            blob: None,
        },
    }
    .encode_length_delimited(out)
}

#[cfg(test)]
mod tests {
    use prost::Message;

    use crate::{
        ids::{BexCallId, BexThreadId, BoundaryId, EngineId, ProcessEuid},
        run::{RunError, RunErrorClass, RunStatus, SourceLocation, TraceCallKey},
        value::{
            CaptureLossKind, CaptureLossReason, CaptureLossRecord, LogEventRecord, LogRecord,
            RunCompletedRecord, ValueCodec, ValueFileRecord, ValueRef, pb,
            read::read_bamlvalue_from_bytes,
        },
    };

    #[test]
    fn log_event_record_metadata_round_trips_through_prost() {
        let record = LogRecord {
            value_ref: ValueRef::available("value_log", ValueCodec::BamlOutboundValue, 3, 3),
            body: vec![4, 5, 6],
            blob_ref: None,
            event: LogEventRecord {
                call: TraceCallKey {
                    process_euid: ProcessEuid([1; 16]),
                    engine_id: EngineId(2),
                    thread_id: BexThreadId(3),
                    call_id: BexCallId(4),
                },
                level: Some("info".to_string()),
                source: Some(SourceLocation {
                    file_path: Some("main.baml".to_string()),
                    file_id: Some(9),
                    line: 12,
                    column: 3,
                    end_line: Some(12),
                    end_column: Some(20),
                    start_offset: Some(100),
                    end_offset: Some(117),
                }),
                timestamp_ms: 170,
                message_preview: Some("hello".to_string()),
            },
        };
        let mut bytes = Vec::new();
        super::encode_header(&mut bytes, BoundaryId::from_bytes([7; 16])).unwrap();
        super::encode_log_event(&mut bytes, &record).unwrap();

        let parsed = read_bamlvalue_from_bytes(&bytes).unwrap();
        assert_eq!(parsed.records, vec![ValueFileRecord::LogEvent(record)]);
        assert!(!parsed.truncated);
    }

    #[test]
    fn run_completed_terminal_value_refs_round_trip() {
        let result_ref = ValueRef::available("value_result", ValueCodec::BamlOutboundValue, 4, 4);
        let error_ref = ValueRef::available("value_error", ValueCodec::BamlOutboundValue, 2, 2);
        let success = RunCompletedRecord {
            status: RunStatus::Succeeded,
            completed_at_ms: 123,
            renderer_hint: Some("baml.outbound.base64".to_string()),
            result_value_ref: Some(result_ref),
            error: None,
            cancellation: None,
        };
        let failure = RunCompletedRecord {
            status: RunStatus::Failed,
            completed_at_ms: 124,
            renderer_hint: None,
            result_value_ref: None,
            error: Some(RunError {
                class: RunErrorClass::Runtime,
                message: "boom".to_string(),
                details: Some("details".to_string()),
                value_ref: Some(error_ref),
            }),
            cancellation: None,
        };
        let mut bytes = Vec::new();
        super::encode_header(&mut bytes, BoundaryId::from_bytes([8; 16])).unwrap();
        super::encode_run_completed(&mut bytes, &success).unwrap();
        super::encode_run_completed(&mut bytes, &failure).unwrap();

        let parsed = read_bamlvalue_from_bytes(&bytes).unwrap();
        assert_eq!(
            parsed.records,
            vec![
                ValueFileRecord::RunCompleted(success),
                ValueFileRecord::RunCompleted(failure)
            ]
        );
        assert!(!parsed.truncated);
    }

    #[test]
    fn run_completed_decodes_old_records_without_terminal_value_refs() {
        let old_record = pb::ValueRecordV1 {
            metadata: None,
            body: Vec::new(),
            run_started: None,
            run_completed: Some(pb::RunCompletedV1 {
                status: pb::RunStatus::Failed as i32,
                completed_at_ms: 456,
                renderer_hint: None,
                error: Some(pb::RunErrorV1 {
                    class: "runtime".to_string(),
                    message: "old boom".to_string(),
                    details: None,
                    value_ref: None,
                }),
                cancellation: None,
                result_value_ref: None,
            }),
            log_event: None,
            capture_loss: None,
            blob: None,
        };
        let mut bytes = Vec::new();
        super::encode_header(&mut bytes, BoundaryId::from_bytes([9; 16])).unwrap();
        old_record.encode_length_delimited(&mut bytes).unwrap();

        let parsed = read_bamlvalue_from_bytes(&bytes).unwrap();
        let [ValueFileRecord::RunCompleted(record)] = parsed.records.as_slice() else {
            panic!("expected run completed record");
        };
        assert_eq!(record.result_value_ref, None);
        assert_eq!(
            record
                .error
                .as_ref()
                .and_then(|error| error.value_ref.as_ref()),
            None
        );
    }

    #[test]
    fn capture_loss_record_round_trips() {
        let record = CaptureLossRecord {
            kind: CaptureLossKind::Log,
            reason: CaptureLossReason::QueueFull,
            skipped_count: 3,
            call: None,
            message: Some("Skipped 3 captured log value(s)".to_string()),
            timestamp_ms: 1234,
        };
        let mut bytes = Vec::new();
        super::encode_header(&mut bytes, BoundaryId::from_bytes([9; 16])).unwrap();
        super::encode_capture_loss(&mut bytes, &record).unwrap();

        let parsed = read_bamlvalue_from_bytes(&bytes).unwrap();
        assert_eq!(parsed.records, vec![ValueFileRecord::CaptureLoss(record)]);
        assert!(!parsed.truncated);
    }

    #[test]
    fn prost_record_shape_is_length_delimited() {
        let record = pb::ValueRecordV1 {
            metadata: Some(pb::ValueMetadataV1 {
                id: "value_1".to_string(),
                codec: pb::ValueCodec::BamlOutboundValue as i32,
                availability: pb::ValueAvailability::Available as i32,
                original_size_bytes: Some(1),
                retained_size_bytes: Some(1),
                diagnostic: None,
            }),
            body: vec![9],
            run_started: None,
            run_completed: None,
            log_event: None,
            capture_loss: None,
            blob: None,
        };
        let mut bytes = Vec::new();
        record.encode_length_delimited(&mut bytes).unwrap();
        assert!(!bytes.is_empty());
    }
}
