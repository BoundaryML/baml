use std::io;

use prost::Message;

pub(crate) fn read_length_delimited_records<Header, ProtoRecord, Record>(
    bytes: &[u8],
    mut convert_record: impl FnMut(ProtoRecord) -> io::Result<Record>,
) -> io::Result<(Header, Vec<Record>, bool)>
where
    Header: Message + Default,
    ProtoRecord: Message + Default,
{
    let mut buf = bytes;
    let header = Header::decode_length_delimited(&mut buf)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
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
        let record = ProtoRecord::decode(frame)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        records.push(convert_record(record)?);
        buf = rest;
    }

    Ok((header, records, truncated))
}
