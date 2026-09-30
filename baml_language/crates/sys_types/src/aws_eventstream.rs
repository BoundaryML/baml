//! Incremental decoder for the AWS event stream wire format
//! (`application/vnd.amazon.eventstream`), the binary framing Bedrock
//! `ConverseStream` answers with.
//!
//! Each message is length-prefixed and checksummed:
//!
//! ```text
//! total_len: u32 BE | headers_len: u32 BE | prelude_crc: u32 BE
//! headers: headers_len bytes
//! payload: total_len - headers_len - 16 bytes
//! message_crc: u32 BE
//! ```
//!
//! `prelude_crc` is the CRC32 of the first 8 bytes and `message_crc` the CRC32
//! of everything before it. A header is a 1-byte name length, the name, a
//! 1-byte value type, and the value.
//!
//! Messages are surfaced as [`SseEvent`]s so a stream of them flows through the
//! same buffer, handle and BAML `SseStream.next()` batches as a text SSE
//! stream:
//!
//! * `:message-type` `event` → `event` is the `:event-type` header
//!   (`contentBlockDelta`, `messageStop`, …), `data` the payload.
//! * `:message-type` `exception` → `event` is `exception:` plus the
//!   `:exception-type` header (`exception:throttlingException`), `data` the
//!   payload (a JSON `{"message": …}` object).
//! * `:message-type` `error` → `event` is `error:` plus the `:error-code`
//!   header, `data` the `:error-message` header.

use crate::sse::SseEvent;

/// Prelude (12) plus trailing message CRC (4).
const FRAME_OVERHEAD: usize = 16;
/// The SDKs reject frames over 16 MiB; a larger length prefix means the byte
/// stream is corrupt or is not an event stream at all.
const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;

/// A malformed event stream. The stream cannot be resynchronized after one: a
/// frame boundary is only known from the previous frame's length prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventStreamError(pub String);

impl std::fmt::Display for EventStreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "malformed AWS event stream: {}", self.0)
    }
}

impl std::error::Error for EventStreamError {}

/// A header value. Only strings are surfaced; the other types are decoded so
/// the cursor lands on the next header.
#[derive(Debug, Clone, PartialEq, Eq)]
enum HeaderValue {
    String(String),
    Other,
}

/// Incremental decoder that buffers a partial frame across `feed` calls.
#[derive(Default)]
pub struct EventStreamDecoder {
    buffer: Vec<u8>,
}

impl EventStreamDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed raw bytes and return every message they complete.
    pub fn feed(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, EventStreamError> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();
        let mut offset = 0;
        while let Some(frame_len) = self.complete_frame_len(offset)? {
            let frame = &self.buffer[offset..offset + frame_len];
            events.push(decode_frame(frame)?);
            offset += frame_len;
        }
        self.buffer.drain(..offset);
        Ok(events)
    }

    /// End of input: leftover bytes are a frame the connection cut short.
    pub fn finish(&mut self) -> Result<Vec<SseEvent>, EventStreamError> {
        if self.buffer.is_empty() {
            Ok(Vec::new())
        } else {
            Err(EventStreamError(format!(
                "stream ended inside a frame ({} trailing bytes)",
                self.buffer.len()
            )))
        }
    }

    /// The length of the frame starting at `offset` when all of it is
    /// buffered, `None` while more bytes are needed.
    fn complete_frame_len(&self, offset: usize) -> Result<Option<usize>, EventStreamError> {
        let available = &self.buffer[offset..];
        if available.len() < 12 {
            return Ok(None);
        }
        let total_len = read_u32(available, 0) as usize;
        let headers_len = read_u32(available, 4) as usize;
        let prelude_crc = read_u32(available, 8);
        if crc32fast::hash(&available[..8]) != prelude_crc {
            return Err(EventStreamError("prelude checksum mismatch".into()));
        }
        if !(FRAME_OVERHEAD..=MAX_FRAME_LEN).contains(&total_len)
            || headers_len > total_len - FRAME_OVERHEAD
        {
            return Err(EventStreamError(format!(
                "invalid frame lengths (total {total_len}, headers {headers_len})"
            )));
        }
        Ok((available.len() >= total_len).then_some(total_len))
    }
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Decode one whole frame (prelude CRC already verified).
fn decode_frame(frame: &[u8]) -> Result<SseEvent, EventStreamError> {
    let total_len = frame.len();
    let headers_len = read_u32(frame, 4) as usize;
    let message_crc = read_u32(frame, total_len - 4);
    if crc32fast::hash(&frame[..total_len - 4]) != message_crc {
        return Err(EventStreamError("message checksum mismatch".into()));
    }
    let headers = decode_headers(&frame[12..12 + headers_len])?;
    let payload = String::from_utf8_lossy(&frame[12 + headers_len..total_len - 4]).into_owned();
    let header = |name: &str| {
        headers.iter().find_map(|(n, v)| match v {
            HeaderValue::String(s) if n == name => Some(s.clone()),
            _ => None,
        })
    };

    let message_type = header(":message-type").unwrap_or_else(|| "event".into());
    let (event, data) = match message_type.as_str() {
        "event" => (header(":event-type").unwrap_or_default(), payload),
        "exception" => (
            format!(
                "exception:{}",
                header(":exception-type").unwrap_or_default()
            ),
            payload,
        ),
        "error" => (
            format!("error:{}", header(":error-code").unwrap_or_default()),
            header(":error-message").unwrap_or_default(),
        ),
        other => {
            return Err(EventStreamError(format!("unknown :message-type {other:?}")));
        }
    };
    Ok(SseEvent {
        event,
        data,
        id: None,
    })
}

fn decode_headers(mut bytes: &[u8]) -> Result<Vec<(String, HeaderValue)>, EventStreamError> {
    fn take<'a>(bytes: &mut &'a [u8], n: usize) -> Result<&'a [u8], EventStreamError> {
        if bytes.len() < n {
            return Err(EventStreamError("truncated header".into()));
        }
        let (head, rest) = bytes.split_at(n);
        *bytes = rest;
        Ok(head)
    }
    fn take_u16(bytes: &mut &[u8]) -> Result<usize, EventStreamError> {
        let b = take(bytes, 2)?;
        Ok(usize::from(u16::from_be_bytes([b[0], b[1]])))
    }

    let mut headers = Vec::new();
    while !bytes.is_empty() {
        let name_len = usize::from(take(&mut bytes, 1)?[0]);
        let name = String::from_utf8_lossy(take(&mut bytes, name_len)?).into_owned();
        let value = match take(&mut bytes, 1)?[0] {
            // bool true / bool false carry no value bytes.
            0 | 1 => HeaderValue::Other,
            2 => take(&mut bytes, 1).map(|_| HeaderValue::Other)?,
            3 => take(&mut bytes, 2).map(|_| HeaderValue::Other)?,
            4 => take(&mut bytes, 4).map(|_| HeaderValue::Other)?,
            // long, timestamp
            5 | 8 => take(&mut bytes, 8).map(|_| HeaderValue::Other)?,
            6 => {
                let len = take_u16(&mut bytes)?;
                take(&mut bytes, len).map(|_| HeaderValue::Other)?
            }
            7 => {
                let len = take_u16(&mut bytes)?;
                HeaderValue::String(String::from_utf8_lossy(take(&mut bytes, len)?).into_owned())
            }
            9 => take(&mut bytes, 16).map(|_| HeaderValue::Other)?,
            other => {
                return Err(EventStreamError(format!(
                    "unknown header value type {other} for {name:?}"
                )));
            }
        };
        headers.push((name, value));
    }
    Ok(headers)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Encode one frame with string headers, as the AWS SDKs do.
    pub(crate) fn frame(headers: &[(&str, &str)], payload: &[u8]) -> Vec<u8> {
        let mut header_bytes = Vec::new();
        for (name, value) in headers {
            header_bytes.push(u8::try_from(name.len()).unwrap());
            header_bytes.extend_from_slice(name.as_bytes());
            header_bytes.push(7);
            header_bytes.extend_from_slice(&u16::try_from(value.len()).unwrap().to_be_bytes());
            header_bytes.extend_from_slice(value.as_bytes());
        }
        let total_len = u32::try_from(FRAME_OVERHEAD + header_bytes.len() + payload.len()).unwrap();
        let mut out = Vec::new();
        out.extend_from_slice(&total_len.to_be_bytes());
        out.extend_from_slice(&u32::try_from(header_bytes.len()).unwrap().to_be_bytes());
        let prelude_crc = crc32fast::hash(&out);
        out.extend_from_slice(&prelude_crc.to_be_bytes());
        out.extend_from_slice(&header_bytes);
        out.extend_from_slice(payload);
        let message_crc = crc32fast::hash(&out);
        out.extend_from_slice(&message_crc.to_be_bytes());
        out
    }

    fn event(event_type: &str, payload: &str) -> Vec<u8> {
        frame(
            &[
                (":event-type", event_type),
                (":content-type", "application/json"),
                (":message-type", "event"),
            ],
            payload.as_bytes(),
        )
    }

    #[test]
    fn decodes_event_frames() {
        let mut bytes = event("messageStart", r#"{"role":"assistant"}"#);
        bytes.extend(event(
            "contentBlockDelta",
            r#"{"contentBlockIndex":0,"delta":{"text":"Hi"}}"#,
        ));
        let events = EventStreamDecoder::new().feed(&bytes).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event, "messageStart");
        assert_eq!(events[0].data, r#"{"role":"assistant"}"#);
        assert_eq!(events[1].event, "contentBlockDelta");
        assert!(events[0].id.is_none());
    }

    #[test]
    fn buffers_frames_split_across_chunks() {
        let bytes = event("messageStop", r#"{"stopReason":"end_turn"}"#);
        let mut decoder = EventStreamDecoder::new();
        // One byte at a time, including inside the prelude.
        let mut events = Vec::new();
        for b in &bytes {
            events.extend(decoder.feed(std::slice::from_ref(b)).unwrap());
        }
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "messageStop");
        assert!(decoder.finish().unwrap().is_empty());
    }

    #[test]
    fn surfaces_exceptions_and_errors() {
        let mut bytes = frame(
            &[
                (":exception-type", "throttlingException"),
                (":content-type", "application/json"),
                (":message-type", "exception"),
            ],
            br#"{"message":"slow down"}"#,
        );
        bytes.extend(frame(
            &[
                (":message-type", "error"),
                (":error-code", "InternalFailure"),
                (":error-message", "boom"),
            ],
            b"",
        ));
        let events = EventStreamDecoder::new().feed(&bytes).unwrap();
        assert_eq!(events[0].event, "exception:throttlingException");
        assert_eq!(events[0].data, r#"{"message":"slow down"}"#);
        assert_eq!(events[1].event, "error:InternalFailure");
        assert_eq!(events[1].data, "boom");
    }

    #[test]
    fn skips_non_string_headers() {
        // A bool, an int, a timestamp and a uuid ahead of the string headers.
        let mut headers = vec![4u8];
        headers.extend_from_slice(b"flag");
        headers.push(0);
        headers.push(3);
        headers.extend_from_slice(b"num");
        headers.push(4);
        headers.extend_from_slice(&7u32.to_be_bytes());
        headers.push(2);
        headers.extend_from_slice(b"ts");
        headers.push(8);
        headers.extend_from_slice(&[0; 8]);
        headers.push(2);
        headers.extend_from_slice(b"id");
        headers.push(9);
        headers.extend_from_slice(&[0; 16]);
        let string_headers = frame(&[(":event-type", "metadata")], b"{}");
        // Splice the extra headers into a hand-built frame.
        let header_len = read_u32(&string_headers, 4) as usize;
        let mut all_headers = headers;
        all_headers.extend_from_slice(&string_headers[12..12 + header_len]);
        let total = u32::try_from(FRAME_OVERHEAD + all_headers.len() + 2).unwrap();
        let mut out = Vec::new();
        out.extend_from_slice(&total.to_be_bytes());
        out.extend_from_slice(&u32::try_from(all_headers.len()).unwrap().to_be_bytes());
        out.extend_from_slice(&crc32fast::hash(&out.clone()).to_be_bytes());
        out.extend_from_slice(&all_headers);
        out.extend_from_slice(b"{}");
        out.extend_from_slice(&crc32fast::hash(&out.clone()).to_be_bytes());

        let events = EventStreamDecoder::new().feed(&out).unwrap();
        assert_eq!(events[0].event, "metadata");
        assert_eq!(events[0].data, "{}");
    }

    #[test]
    fn rejects_corrupt_frames() {
        let mut bytes = event("messageStop", "{}");
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        let err = EventStreamDecoder::new().feed(&bytes).unwrap_err();
        assert!(err.0.contains("message checksum"), "{err}");

        let mut bytes = event("messageStop", "{}");
        bytes[0] ^= 0xff;
        let err = EventStreamDecoder::new().feed(&bytes).unwrap_err();
        assert!(err.0.contains("prelude checksum"), "{err}");

        // A text/event-stream body read as an event stream.
        let err = EventStreamDecoder::new()
            .feed(b"data: {\"a\":1}\n\n")
            .unwrap_err();
        assert!(err.0.contains("prelude checksum"), "{err}");
    }

    #[test]
    fn truncated_stream_is_an_error_at_finish() {
        let bytes = event("messageStop", "{}");
        let mut decoder = EventStreamDecoder::new();
        assert!(decoder.feed(&bytes[..bytes.len() - 3]).unwrap().is_empty());
        assert!(decoder.finish().is_err());
    }
}
