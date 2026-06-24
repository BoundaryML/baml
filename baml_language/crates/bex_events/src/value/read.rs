//! Target-neutral `.bamlvalue` parsing helpers.

use std::io::{self, Read};

use prost::Message;

use crate::value::{ValueRecord, pb};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BamlvalueContents {
    pub header: pb::ValueFileHeaderV1,
    pub records: Vec<ValueRecord>,
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
        let metadata = record.metadata.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "value record omitted metadata")
        })?;
        records.push(ValueRecord {
            value_ref: metadata.try_into()?,
            body: record.body,
        });
        buf = rest;
    }

    Ok(BamlvalueContents {
        header,
        records,
        truncated,
    })
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
