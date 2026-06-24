//! Target-neutral `.bamlvalue` encoding helpers.

use prost::Message;

use crate::{
    ids::BoundaryId,
    value::{RunCompletedRecord, RunStartedRecord, ValueFileRecord, ValueRecord, pb},
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
        },
        ValueFileRecord::RunStarted(record) => pb::ValueRecordV1 {
            metadata: None,
            body: Vec::new(),
            capture: None,
            run_started: Some(record.into()),
            run_completed: None,
        },
        ValueFileRecord::RunCompleted(record) => pb::ValueRecordV1 {
            metadata: None,
            body: Vec::new(),
            capture: None,
            run_started: None,
            run_completed: Some(record.into()),
        },
    }
    .encode_length_delimited(out)
}

#[cfg(test)]
mod tests {
    use prost::Message;

    use crate::{
        ids::BoundaryId,
        value::{
            ValueAvailability, ValueCodec, ValueRecord, ValueRef, pb,
            read::read_bamlvalue_from_bytes,
        },
    };

    #[test]
    fn value_record_metadata_round_trips_through_prost() {
        let value_ref = ValueRef::available("value_7", ValueCodec::BamlOutboundValue, 3, 3);
        let record = ValueRecord {
            value_ref: value_ref.clone(),
            body: vec![1, 2, 3],
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
        };
        let mut bytes = Vec::new();
        record.encode_length_delimited(&mut bytes).unwrap();
        assert!(!bytes.is_empty());
    }
}
