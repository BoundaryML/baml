//! Minimal Server-Sent Events decoder over a raw byte stream, replacing the
//! external `eventsource-stream` adapter. It implements the subset of the SSE
//! spec the LLM streaming code relies on (event/data/id fields, `data`
//! continuation joins, comment and blank-line handling) and surfaces transport
//! errors unchanged so callers can still classify timeouts from the error text.

use std::collections::VecDeque;

use bytes::Bytes;
use futures::stream::{Stream, StreamExt};

/// A decoded SSE message. Mirrors the fields of `eventsource_stream::Event`
/// that the streaming code consumes.
pub struct SseEvent {
    pub event: String,
    pub data: String,
    pub id: String,
}

struct Decoder {
    /// Raw bytes not yet split into complete lines (may end mid-line, or
    /// mid-UTF-8, until more chunks arrive).
    buf: Vec<u8>,
    event_type: String,
    data: String,
    has_data: bool,
    /// Last seen `id:` value, carried forward across events (last-event-id).
    last_id: String,
}

impl Decoder {
    fn new() -> Self {
        Decoder {
            buf: Vec::new(),
            event_type: String::new(),
            data: String::new(),
            has_data: false,
            last_id: String::new(),
        }
    }

    /// Append a chunk and drain any completed events into `out`.
    fn push(&mut self, chunk: &[u8], out: &mut VecDeque<SseEvent>) {
        self.buf.extend_from_slice(chunk);
        while let Some(line) = self.take_line() {
            self.handle_line(&line, out);
        }
    }

    /// Pop the next complete line (without its terminator), or `None` if no full
    /// line terminator is buffered yet. Handles `\n`, `\r\n`, and lone `\r`;
    /// splitting on these ASCII bytes is UTF-8 safe.
    fn take_line(&mut self) -> Option<String> {
        let pos = self.buf.iter().position(|&b| b == b'\n' || b == b'\r')?;
        let is_cr = self.buf[pos] == b'\r';
        // A trailing '\r' at the end might begin a "\r\n" whose '\n' has not
        // arrived yet; wait for more bytes before splitting.
        if is_cr && pos == self.buf.len() - 1 {
            return None;
        }
        let line = String::from_utf8_lossy(&self.buf[..pos]).into_owned();
        let drain_to = if is_cr && self.buf.get(pos + 1) == Some(&b'\n') {
            pos + 2
        } else {
            pos + 1
        };
        self.buf.drain(..drain_to);
        Some(line)
    }

    fn handle_line(&mut self, line: &str, out: &mut VecDeque<SseEvent>) {
        if line.is_empty() {
            self.dispatch(out);
            return;
        }
        if line.starts_with(':') {
            return; // comment line
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "event" => self.event_type = value.to_string(),
            "data" => {
                self.data.push_str(value);
                self.data.push('\n');
                self.has_data = true;
            }
            "id" => {
                if !value.contains('\0') {
                    self.last_id = value.to_string();
                }
            }
            // "retry" and unknown fields are ignored.
            _ => {}
        }
    }

    /// A blank line dispatches the buffered event. Per the SSE spec, an event
    /// with no `data` is dropped, and the data/event-type buffers reset each time
    /// (the id persists).
    fn dispatch(&mut self, out: &mut VecDeque<SseEvent>) {
        if !self.has_data {
            self.event_type.clear();
            self.data.clear();
            return;
        }
        if self.data.ends_with('\n') {
            self.data.pop();
        }
        let event = if self.event_type.is_empty() {
            "message".to_string()
        } else {
            self.event_type.clone()
        };
        out.push_back(SseEvent {
            event,
            data: self.data.clone(),
            id: self.last_id.clone(),
        });
        self.event_type.clear();
        self.data.clear();
        self.has_data = false;
    }
}

/// Decode a byte stream (e.g. `baml_http::Response::bytes_stream()`) into a stream
/// of SSE events. Transport errors are surfaced unchanged.
pub fn eventsource<S>(stream: S) -> impl Stream<Item = Result<SseEvent, baml_http::Error>>
where
    S: Stream<Item = baml_http::Result<Bytes>>,
{
    struct State<S> {
        inner: std::pin::Pin<Box<S>>,
        decoder: Decoder,
        pending: VecDeque<SseEvent>,
        done: bool,
    }

    let state = State {
        inner: Box::pin(stream),
        decoder: Decoder::new(),
        pending: VecDeque::new(),
        done: false,
    };

    futures::stream::unfold(state, |mut st| async move {
        loop {
            if let Some(ev) = st.pending.pop_front() {
                return Some((Ok(ev), st));
            }
            if st.done {
                return None;
            }
            match st.inner.next().await {
                Some(Ok(chunk)) => st.decoder.push(&chunk, &mut st.pending),
                Some(Err(e)) => {
                    st.done = true;
                    return Some((Err(e), st));
                }
                None => return None,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed the decoder a sequence of byte chunks and collect every event that
    /// results, as `(event, data, id)` tuples.
    fn decode(chunks: &[&str]) -> Vec<(String, String, String)> {
        let mut decoder = Decoder::new();
        let mut out = VecDeque::new();
        for chunk in chunks {
            decoder.push(chunk.as_bytes(), &mut out);
        }
        out.into_iter().map(|e| (e.event, e.data, e.id)).collect()
    }

    #[test]
    fn basic_event_and_default_type() {
        assert_eq!(
            decode(&["event: ping\ndata: hello\n\n"]),
            vec![("ping".into(), "hello".into(), "".into())]
        );
        // No `event:` field defaults the type to "message".
        assert_eq!(
            decode(&["data: hi\n\n"]),
            vec![("message".into(), "hi".into(), "".into())]
        );
    }

    #[test]
    fn multi_line_data_is_joined_with_newlines() {
        assert_eq!(
            decode(&["data: a\ndata: b\ndata: c\n\n"]),
            vec![("message".into(), "a\nb\nc".into(), "".into())]
        );
    }

    #[test]
    fn crlf_and_lone_cr_line_endings() {
        // CRLF terminators.
        assert_eq!(
            decode(&["data: x\r\n\r\n"]),
            vec![("message".into(), "x".into(), "".into())]
        );
        // A lone CR acts as a line separator between two `data:` fields (a
        // trailing lone CR is instead held back, in case a `\n` follows, so that
        // a CRLF split across chunk boundaries is decoded correctly).
        assert_eq!(
            decode(&["data: a\rdata: b\n\n"]),
            vec![("message".into(), "a\nb".into(), "".into())]
        );
    }

    #[test]
    fn events_split_across_chunks() {
        // A `data:` line and its terminator arrive in separate chunks, including
        // a CRLF split down the middle.
        assert_eq!(
            decode(&["data: hel", "lo\n", "\n"]),
            vec![("message".into(), "hello".into(), "".into())]
        );
        assert_eq!(
            decode(&["data: split\r", "\n\r\n"]),
            vec![("message".into(), "split".into(), "".into())]
        );
    }

    #[test]
    fn multibyte_utf8_split_across_chunks() {
        // The 'é' (0xC3 0xA9) is split across two chunks; it must not be decoded
        // until the full line terminator arrives.
        let bytes = "data: café".as_bytes();
        let (head, tail) = bytes.split_at(bytes.len() - 1);
        let mut decoder = Decoder::new();
        let mut out = VecDeque::new();
        decoder.push(head, &mut out);
        decoder.push(tail, &mut out);
        decoder.push(b"\n\n", &mut out);
        let events: Vec<_> = out.into_iter().map(|e| e.data).collect();
        assert_eq!(events, vec!["café".to_string()]);
    }

    #[test]
    fn comments_and_dataless_events_are_dropped() {
        // A comment line followed by a blank line yields no event.
        assert_eq!(decode(&[": keep-alive\n\n"]), Vec::new());
        // A blank line with no buffered data yields no event.
        assert_eq!(decode(&["\n"]), Vec::new());
    }

    #[test]
    fn done_sentinel_is_passed_through_as_data() {
        assert_eq!(
            decode(&["data: [DONE]\n\n"]),
            vec![("message".into(), "[DONE]".into(), "".into())]
        );
    }

    #[test]
    fn id_persists_across_events() {
        assert_eq!(
            decode(&["id: 42\ndata: a\n\ndata: b\n\n"]),
            vec![
                ("message".into(), "a".into(), "42".into()),
                ("message".into(), "b".into(), "42".into()),
            ]
        );
    }

    #[test]
    fn value_leading_space_is_stripped_once() {
        // Only a single leading space after the colon is stripped.
        assert_eq!(
            decode(&["data:  two-spaces\n\n"]),
            vec![("message".into(), " two-spaces".into(), "".into())]
        );
    }
}
