//! The event stream wire format: one length-prefixed, checksummed message per
//! frame.
//!
//! ```text
//! total_len: u32 BE | headers_len: u32 BE | prelude_crc: u32 BE
//! headers: headers_len bytes
//! payload: total_len - headers_len - 16 bytes
//! message_crc: u32 BE
//! ```
//!
//! `prelude_crc` is the CRC32 of the first 8 bytes, `message_crc` the CRC32 of
//! everything before it. A header is a 1-byte name length, the UTF-8 name, a
//! 1-byte value type and the value. Validation follows upstream's
//! `read_message_from` check for check, so a malformed frame fails with the
//! same [`ErrorKind`] (see `tests/upstream_vectors.rs`).

use std::fmt;

const PRELUDE_LEN: usize = 12;
const MESSAGE_CRC_LEN: usize = 4;
const MAX_HEADER_NAME_LEN: usize = 255;
const MIN_HEADER_LEN: usize = 2;
/// The event stream spec caps a message at 16 MiB. A larger length prefix
/// behind a valid prelude checksum is refused rather than buffered.
const MAX_MESSAGE_LEN: usize = 16 * 1024 * 1024;

const TYPE_TRUE: u8 = 0;
const TYPE_FALSE: u8 = 1;
const TYPE_BYTE: u8 = 2;
const TYPE_INT16: u8 = 3;
const TYPE_INT32: u8 = 4;
const TYPE_INT64: u8 = 5;
const TYPE_BYTE_ARRAY: u8 = 6;
const TYPE_STRING: u8 = 7;
const TYPE_TIMESTAMP: u8 = 8;
const TYPE_UUID: u8 = 9;

/// A header value, one variant per wire type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderValue {
    Bool(bool),
    Byte(i8),
    Int16(i16),
    Int32(i32),
    Int64(i64),
    ByteArray(Vec<u8>),
    String(String),
    /// Milliseconds since the Unix epoch.
    Timestamp(i64),
    Uuid(u128),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub name: String,
    pub value: HeaderValue,
}

impl Header {
    pub fn new(name: impl Into<String>, value: HeaderValue) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }
}

/// One decoded event stream message.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Message {
    pub headers: Vec<Header>,
    pub payload: Vec<u8>,
}

impl Message {
    /// The first header named `name`.
    pub fn header(&self, name: &str) -> Option<&HeaderValue> {
        self.headers
            .iter()
            .find(|header| header.name == name)
            .map(|header| &header.value)
    }

    /// The first header named `name`, when it is a string.
    pub fn string_header(&self, name: &str) -> Option<&str> {
        match self.header(name)? {
            HeaderValue::String(value) => Some(value),
            _ => None,
        }
    }
}

/// Why a frame could not be read or written. Variant names match upstream's
/// `aws_smithy_eventstream::error::ErrorKind`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    HeadersTooLong,
    HeaderValueTooLong,
    InvalidHeaderNameLength,
    InvalidHeaderValue,
    InvalidHeaderValueType(u8),
    InvalidHeadersLength,
    InvalidMessageLength,
    InvalidUtf8String,
    /// (expected, actual)
    MessageChecksumMismatch(u32, u32),
    MessageTooLong,
    PayloadTooLong,
    /// (expected, actual)
    PreludeChecksumMismatch(u32, u32),
    /// The stream ended inside a frame. Fork-only: upstream's decoder has no
    /// end-of-stream step.
    TruncatedStream(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    kind: ErrorKind,
}

impl Error {
    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Self { kind }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ErrorKind::{
            HeaderValueTooLong, HeadersTooLong, InvalidHeaderNameLength, InvalidHeaderValue,
            InvalidHeaderValueType, InvalidHeadersLength, InvalidMessageLength, InvalidUtf8String,
            MessageChecksumMismatch, MessageTooLong, PayloadTooLong, PreludeChecksumMismatch,
            TruncatedStream,
        };
        match &self.kind {
            HeadersTooLong => write!(f, "headers too long to fit in event stream frame"),
            HeaderValueTooLong => write!(f, "header value too long to fit in event stream frame"),
            InvalidHeaderNameLength => write!(f, "invalid header name length"),
            InvalidHeaderValue => write!(f, "invalid header value"),
            InvalidHeaderValueType(value) => write!(f, "invalid header value type: {value}"),
            InvalidHeadersLength => write!(f, "invalid headers length"),
            InvalidMessageLength => write!(f, "invalid message length"),
            InvalidUtf8String => write!(f, "encountered invalid UTF-8 string"),
            MessageChecksumMismatch(expected, actual) => write!(
                f,
                "message checksum 0x{actual:X} didn't match expected checksum 0x{expected:X}"
            ),
            MessageTooLong => write!(f, "message too long to fit in event stream frame"),
            PayloadTooLong => write!(f, "message payload too long to fit in event stream frame"),
            PreludeChecksumMismatch(expected, actual) => write!(
                f,
                "prelude checksum 0x{actual:X} didn't match expected checksum 0x{expected:X}"
            ),
            TruncatedStream(bytes) => {
                write!(
                    f,
                    "event stream ended inside a frame ({bytes} trailing bytes)"
                )
            }
        }
    }
}

impl std::error::Error for Error {}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// A forward-only reader over a byte slice.
struct Cursor<'a> {
    bytes: &'a [u8],
    read: usize,
}

impl<'a> Cursor<'a> {
    fn remaining(&self) -> usize {
        self.bytes.len() - self.read
    }

    fn take(&mut self, n: usize) -> &'a [u8] {
        let out = &self.bytes[self.read..self.read + n];
        self.read += n;
        out
    }

    fn take_array<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        if self.remaining() < N {
            return Err(ErrorKind::InvalidHeaderValue.into());
        }
        Ok(self.take(N).try_into().expect("length checked"))
    }
}

fn read_header_value(cursor: &mut Cursor<'_>) -> Result<HeaderValue, Error> {
    let value_type = cursor.take(1)[0];
    Ok(match value_type {
        TYPE_TRUE => HeaderValue::Bool(true),
        TYPE_FALSE => HeaderValue::Bool(false),
        TYPE_BYTE => HeaderValue::Byte(i8::from_be_bytes(cursor.take_array()?)),
        TYPE_INT16 => HeaderValue::Int16(i16::from_be_bytes(cursor.take_array()?)),
        TYPE_INT32 => HeaderValue::Int32(i32::from_be_bytes(cursor.take_array()?)),
        TYPE_INT64 => HeaderValue::Int64(i64::from_be_bytes(cursor.take_array()?)),
        TYPE_BYTE_ARRAY | TYPE_STRING => {
            // Upstream requires strictly more than the length prefix.
            if cursor.remaining() <= 2 {
                return Err(ErrorKind::InvalidHeaderValue.into());
            }
            let len = usize::from(u16::from_be_bytes(cursor.take_array()?));
            if cursor.remaining() < len {
                return Err(ErrorKind::InvalidHeaderValue.into());
            }
            let bytes = cursor.take(len).to_vec();
            if value_type == TYPE_STRING {
                HeaderValue::String(
                    String::from_utf8(bytes).map_err(|_| ErrorKind::InvalidUtf8String)?,
                )
            } else {
                HeaderValue::ByteArray(bytes)
            }
        }
        TYPE_TIMESTAMP => HeaderValue::Timestamp(i64::from_be_bytes(cursor.take_array()?)),
        TYPE_UUID => HeaderValue::Uuid(u128::from_be_bytes(cursor.take_array()?)),
        other => return Err(ErrorKind::InvalidHeaderValueType(other).into()),
    })
}

fn read_header(cursor: &mut Cursor<'_>) -> Result<Header, Error> {
    if cursor.remaining() < MIN_HEADER_LEN {
        return Err(ErrorKind::InvalidHeadersLength.into());
    }
    let name_len = usize::from(cursor.take(1)[0]);
    if name_len >= cursor.remaining() {
        return Err(ErrorKind::InvalidHeaderNameLength.into());
    }
    let name = std::str::from_utf8(cursor.take(name_len))
        .map_err(|_| ErrorKind::InvalidUtf8String)?
        .to_owned();
    let value = read_header_value(cursor)?;
    Ok(Header { name, value })
}

/// Validate the 12-byte prelude at the start of `bytes`; returns
/// `(total_len, headers_len)`.
fn read_prelude(bytes: &[u8]) -> Result<(usize, usize), Error> {
    let total_len = read_u32(bytes, 0) as usize;
    let headers_len = read_u32(bytes, 4) as usize;
    let expected = crc32fast::hash(&bytes[..8]);
    let actual = read_u32(bytes, 8);
    if expected != actual {
        return Err(ErrorKind::PreludeChecksumMismatch(expected, actual).into());
    }
    let max_headers_len = total_len
        .checked_sub(PRELUDE_LEN + MESSAGE_CRC_LEN)
        .ok_or(ErrorKind::InvalidMessageLength)?;
    // The headers length can be 0 or >= 2, but must fit within the frame.
    if headers_len == 1 || headers_len > max_headers_len {
        return Err(ErrorKind::InvalidHeadersLength.into());
    }
    if total_len > MAX_MESSAGE_LEN {
        return Err(ErrorKind::InvalidMessageLength.into());
    }
    Ok((total_len, headers_len))
}

/// Read one message from the start of `bytes`, which must hold the whole
/// frame. Returns the message and the frame's length.
pub fn read_message_from(bytes: &[u8]) -> Result<(Message, usize), Error> {
    if bytes.len() < PRELUDE_LEN {
        return Err(ErrorKind::InvalidMessageLength.into());
    }
    let total_len = read_u32(bytes, 0) as usize;
    if bytes.len() < total_len {
        return Err(ErrorKind::InvalidMessageLength.into());
    }
    let (total_len, headers_len) = read_prelude(bytes)?;

    // Headers are read from the rest of the frame, as upstream does, so a
    // header that runs past `headers_len` is reported as InvalidHeaderValue.
    let mut cursor = Cursor {
        bytes: &bytes[PRELUDE_LEN..total_len],
        read: 0,
    };
    let mut headers = Vec::new();
    while cursor.read < headers_len {
        headers.push(read_header(&mut cursor)?);
        if cursor.read > headers_len {
            return Err(ErrorKind::InvalidHeaderValue.into());
        }
    }

    let payload_start = PRELUDE_LEN + headers_len;
    let payload = bytes[payload_start..total_len - MESSAGE_CRC_LEN].to_vec();

    let expected = crc32fast::hash(&bytes[..total_len - MESSAGE_CRC_LEN]);
    let actual = read_u32(bytes, total_len - MESSAGE_CRC_LEN);
    if expected != actual {
        return Err(ErrorKind::MessageChecksumMismatch(expected, actual).into());
    }
    Ok((Message { headers, payload }, total_len))
}

fn write_header_value(value: &HeaderValue, out: &mut Vec<u8>) -> Result<(), Error> {
    match value {
        HeaderValue::Bool(true) => out.push(TYPE_TRUE),
        HeaderValue::Bool(false) => out.push(TYPE_FALSE),
        HeaderValue::Byte(v) => {
            out.push(TYPE_BYTE);
            out.extend_from_slice(&v.to_be_bytes());
        }
        HeaderValue::Int16(v) => {
            out.push(TYPE_INT16);
            out.extend_from_slice(&v.to_be_bytes());
        }
        HeaderValue::Int32(v) => {
            out.push(TYPE_INT32);
            out.extend_from_slice(&v.to_be_bytes());
        }
        HeaderValue::Int64(v) => {
            out.push(TYPE_INT64);
            out.extend_from_slice(&v.to_be_bytes());
        }
        HeaderValue::ByteArray(bytes) => {
            out.push(TYPE_BYTE_ARRAY);
            let len = u16::try_from(bytes.len()).map_err(|_| ErrorKind::HeaderValueTooLong)?;
            out.extend_from_slice(&len.to_be_bytes());
            out.extend_from_slice(bytes);
        }
        HeaderValue::String(value) => {
            out.push(TYPE_STRING);
            let len = u16::try_from(value.len()).map_err(|_| ErrorKind::HeaderValueTooLong)?;
            out.extend_from_slice(&len.to_be_bytes());
            out.extend_from_slice(value.as_bytes());
        }
        HeaderValue::Timestamp(millis) => {
            out.push(TYPE_TIMESTAMP);
            out.extend_from_slice(&millis.to_be_bytes());
        }
        HeaderValue::Uuid(v) => {
            out.push(TYPE_UUID);
            out.extend_from_slice(&v.to_be_bytes());
        }
    }
    Ok(())
}

/// Encode `message` as one frame.
pub fn write_message(message: &Message) -> Result<Vec<u8>, Error> {
    let mut headers = Vec::new();
    for header in &message.headers {
        if header.name.len() > MAX_HEADER_NAME_LEN {
            return Err(ErrorKind::InvalidHeaderNameLength.into());
        }
        headers.push(u8::try_from(header.name.len()).expect("bounds checked above"));
        headers.extend_from_slice(header.name.as_bytes());
        write_header_value(&header.value, &mut headers)?;
    }
    let headers_len = u32::try_from(headers.len()).map_err(|_| ErrorKind::HeadersTooLong)?;
    let payload_len =
        u32::try_from(message.payload.len()).map_err(|_| ErrorKind::PayloadTooLong)?;
    // 12-byte prelude + 4-byte message CRC.
    let total_len = [16, headers_len, payload_len]
        .into_iter()
        .try_fold(0u32, u32::checked_add)
        .ok_or(ErrorKind::MessageTooLong)?;

    let mut out = Vec::with_capacity(total_len as usize);
    out.extend_from_slice(&total_len.to_be_bytes());
    out.extend_from_slice(&headers_len.to_be_bytes());
    let prelude_crc = crc32fast::hash(&out);
    out.extend_from_slice(&prelude_crc.to_be_bytes());
    out.extend_from_slice(&headers);
    out.extend_from_slice(&message.payload);
    let message_crc = crc32fast::hash(&out);
    out.extend_from_slice(&message_crc.to_be_bytes());
    Ok(out)
}

/// Incremental decoder for a byte stream of frames: feed it chunks as they
/// arrive and it returns each message they complete. A frame boundary is only
/// known from the previous frame's length prefix, so after any error the
/// stream cannot be resynchronized.
#[derive(Debug, Default)]
pub struct MessageFrameDecoder {
    buffer: Vec<u8>,
}

impl MessageFrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Buffer `chunk` and return every message now complete.
    pub fn feed(&mut self, chunk: &[u8]) -> Result<Vec<Message>, Error> {
        self.buffer.extend_from_slice(chunk);
        let mut messages = Vec::new();
        let mut offset = 0;
        loop {
            let available = &self.buffer[offset..];
            if available.len() < PRELUDE_LEN {
                break;
            }
            // Checked as soon as the prelude arrives (upstream waits for the
            // whole frame), so a body that is not an event stream fails on its
            // first 12 bytes instead of being buffered.
            let (total_len, _) = read_prelude(available)?;
            if available.len() < total_len {
                break;
            }
            let (message, len) = read_message_from(&available[..total_len])?;
            messages.push(message);
            offset += len;
        }
        self.buffer.drain(..offset);
        Ok(messages)
    }

    /// End of input. Leftover bytes are a frame the connection cut short.
    pub fn finish(&mut self) -> Result<(), Error> {
        if self.buffer.is_empty() {
            Ok(())
        } else {
            Err(ErrorKind::TruncatedStream(self.buffer.len()).into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Message {
        Message {
            headers: vec![
                Header::new(":event-type", HeaderValue::String("messageStop".into())),
                Header::new("flag", HeaderValue::Bool(false)),
                Header::new("byte", HeaderValue::Byte(-3)),
                Header::new("short", HeaderValue::Int16(-300)),
                Header::new("int", HeaderValue::Int32(70_000)),
                Header::new("long", HeaderValue::Int64(-5_000_000_000)),
                Header::new("bytes", HeaderValue::ByteArray(vec![0, 1, 255])),
                Header::new("ts", HeaderValue::Timestamp(1_700_000_000_000)),
                Header::new(
                    "id",
                    HeaderValue::Uuid(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef),
                ),
            ],
            payload: br#"{"stopReason":"end_turn"}"#.to_vec(),
        }
    }

    #[test]
    fn round_trips_every_header_type() {
        let message = sample();
        let frame = write_message(&message).unwrap();
        let (decoded, len) = read_message_from(&frame).unwrap();
        assert_eq!(decoded, message);
        assert_eq!(len, frame.len());
        assert_eq!(decoded.string_header(":event-type"), Some("messageStop"));
        assert_eq!(decoded.string_header("int"), None);
    }

    #[test]
    fn decoder_handles_split_and_batched_frames() {
        let frame = write_message(&sample()).unwrap();
        let mut stream = frame.clone();
        stream.extend_from_slice(&frame);

        // One byte at a time, including inside the prelude.
        let mut decoder = MessageFrameDecoder::new();
        let mut messages = Vec::new();
        for byte in &stream {
            messages.extend(decoder.feed(std::slice::from_ref(byte)).unwrap());
        }
        assert_eq!(messages, vec![sample(), sample()]);
        decoder.finish().unwrap();

        // Both frames in one chunk.
        let mut decoder = MessageFrameDecoder::new();
        assert_eq!(decoder.feed(&stream).unwrap().len(), 2);
    }

    #[test]
    fn decoder_fails_fast_on_a_non_event_stream_body() {
        let err = MessageFrameDecoder::new()
            .feed(b"data: {\"a\":1}\n\n")
            .unwrap_err();
        assert!(
            matches!(err.kind(), ErrorKind::PreludeChecksumMismatch(..)),
            "{err}"
        );
    }

    #[test]
    fn decoder_reports_a_truncated_stream() {
        let frame = write_message(&sample()).unwrap();
        let mut decoder = MessageFrameDecoder::new();
        assert!(decoder.feed(&frame[..frame.len() - 3]).unwrap().is_empty());
        let err = decoder.finish().unwrap_err();
        assert_eq!(err.kind(), &ErrorKind::TruncatedStream(frame.len() - 3));
    }
}
