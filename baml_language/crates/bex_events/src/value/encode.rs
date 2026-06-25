//! Target-neutral `.bamlvalue` encoding helpers.

use prost::Message;

use crate::{
    ids::BoundaryId,
    value::{
        CaptureLossRecord, LogRecord, RunCompletedRecord, RunStartedRecord, ValueFileRecord,
        ValueRecord, pb,
    },
};

pub fn encode_length_delimited_message(
    out: &mut Vec<u8>,
    msg: &impl Message,
) -> Result<(), prost::EncodeError> {
    msg.encode_length_delimited(out)
}

pub fn encode_header(out: &mut Vec<u8>, boundary_id: BoundaryId) -> Result<(), prost::EncodeError> {
    pb::ValueFileHeaderV1 {
        boundary_id: boundary_id.as_bytes().to_vec(),
    }
    .encode_length_delimited(out)
}

pub fn encode_record(out: &mut Vec<u8>, record: &ValueRecord) -> Result<(), prost::EncodeError> {
    encode_file_record(out, &ValueFileRecord::CapturedValue(record.clone()))
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
        ValueFileRecord::CapturedValue(record) => pb::ValueRecordV1 {
            metadata: Some((&record.value_ref).into()),
            body: record.body.clone(),
            capture: record.capture.as_ref().map(Into::into),
            run_started: None,
            run_completed: None,
            log_event: None,
            capture_loss: None,
            blob: record.blob_ref.as_ref().map(Into::into),
        },
        ValueFileRecord::LogEvent(record) => pb::ValueRecordV1 {
            metadata: Some((&record.value_ref).into()),
            body: record.body.clone(),
            capture: None,
            run_started: None,
            run_completed: None,
            log_event: Some((&record.event).into()),
            capture_loss: None,
            blob: record.blob_ref.as_ref().map(Into::into),
        },
        ValueFileRecord::CaptureLoss(record) => pb::ValueRecordV1 {
            metadata: None,
            body: Vec::new(),
            capture: None,
            run_started: None,
            run_completed: None,
            log_event: None,
            capture_loss: Some(record.into()),
            blob: None,
        },
        ValueFileRecord::RunStarted(record) => pb::ValueRecordV1 {
            metadata: None,
            body: Vec::new(),
            capture: None,
            run_started: Some(record.into()),
            run_completed: None,
            log_event: None,
            capture_loss: None,
            blob: None,
        },
        ValueFileRecord::RunCompleted(record) => pb::ValueRecordV1 {
            metadata: None,
            body: Vec::new(),
            capture: None,
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
        run::{SourceLocation, TraceCallKey},
        value::{
            CaptureLossKind, CaptureLossReason, CaptureLossRecord, LogEventRecord, LogRecord,
            ValueAvailability, ValueCodec, ValueFileRecord, ValueRecord, ValueRef, pb,
            read::read_bamlvalue_from_bytes,
        },
    };

    #[test]
    fn value_record_metadata_round_trips_through_prost() {
        let value_ref = ValueRef::available("value_7", ValueCodec::BamlOutboundValue, 3, 3);
        let record = ValueRecord {
            value_ref,
            body: vec![1, 2, 3],
            blob_ref: None,
            capture: None,
        };
        let mut bytes = Vec::new();
        super::encode_header(&mut bytes, BoundaryId::from_bytes([7; 16])).unwrap();
        super::encode_record(&mut bytes, &record).unwrap();

        let parsed = read_bamlvalue_from_bytes(&bytes).unwrap();
        assert_eq!(parsed.header.boundary_id, vec![7; 16]);
        assert_eq!(
            parsed.records,
            vec![crate::value::ValueFileRecord::CapturedValue(record)]
        );
        assert!(!parsed.truncated);

        let metadata = pb::ValueMetadataV1 {
            id: "lost".to_string(),
            codec: pb::ValueCodec::BamlOutboundValue as i32,
            availability: pb::ValueAvailability::Lost as i32,
            original_size_bytes: None,
            retained_size_bytes: Some(0),
            diagnostic: Some("queue full".to_string()),
        };
        let value_ref = ValueRef::try_from(metadata).unwrap();
        assert_eq!(value_ref.availability, ValueAvailability::Lost);
    }

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
            capture: None,
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
