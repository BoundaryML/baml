//! Target-neutral `.bamlvalue` parsing helpers.

use std::io::{self, Read};

use prost::Message;

use crate::value::{LogRecord, ValueFileRecord, ValueRecord, pb};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BamlvalueContents {
    pub header: pb::ValueFileHeaderV1,
    pub records: Vec<ValueFileRecord>,
    pub truncated: bool,
}

pub fn read_bamlvalue_from_reader(mut reader: impl Read) -> io::Result<BamlvalueContents> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    read_bamlvalue_from_bytes(&bytes)
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
    let has_lifecycle = record.run_started.is_some() || record.run_completed.is_some();
    let has_log_event = record.log_event.is_some();
    let has_capture_loss = record.capture_loss.is_some();
    if has_lifecycle && (record.metadata.is_some() || has_log_event || has_capture_loss) {
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
            event: log_event.try_into()?,
        }));
    }
    Ok(ValueFileRecord::CapturedValue(ValueRecord {
        value_ref: metadata.try_into()?,
        body: record.body,
        capture: record.capture.map(TryInto::try_into).transpose()?,
    }))
}

#[cfg(test)]
mod tests {
    use crate::{
        ids::BoundaryId,
        value::{
            ValueCodec, ValueRecord, ValueRef,
            encode::{encode_header, encode_record},
        },
    };

    #[test]
    fn trailing_partial_record_is_reported_as_truncated() {
        let mut bytes = Vec::new();
        encode_header(&mut bytes, BoundaryId::from_bytes([3; 16])).unwrap();
        let record = ValueRecord {
            value_ref: ValueRef::available("value_1", ValueCodec::BamlOutboundValue, 3, 3),
            body: vec![1, 2, 3],
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
}
