//! The events of a streamed response body, decoded as its chunks arrive:
//! server-sent events, or the binary event stream of AWS, which a
//! `baml.http.SseStream` hands on in the same shape.

use aws_smithy_eventstream::frame::{Message, MessageFrameDecoder};

/// The content type of a binary AWS event stream.
const AWS_EVENT_STREAM: &str = "application/vnd.amazon.eventstream";

/// One event.
pub(super) struct Event {
    /// The kind of event: `message` when the stream names none.
    kind: String,
    data: String,
    id: Option<String>,
}

/// `events` as the JSON text `SseStream.next` returns: an array of
/// objects, each with its `event`, its `data` and its `id` or `null`.
pub(super) fn to_json(events: &[Event]) -> String {
    let text = |text: &str| serde_json::to_string(text).expect("a string is JSON");
    let objects: Vec<String> = events
        .iter()
        .map(|event| {
            format!(
                "{{\"event\":{},\"data\":{},\"id\":{}}}",
                text(&event.kind),
                text(&event.data),
                event.id.as_deref().map_or_else(|| "null".to_owned(), text)
            )
        })
        .collect();
    format!("[{}]", objects.join(","))
}

/// Server-sent events, as the WHATWG specification reads them.
#[derive(Default)]
pub(super) struct Lines {
    /// The bytes of a line that has not ended yet.
    unread: Vec<u8>,
    /// The event being put together.
    event: String,
    data: Vec<String>,
    id: Option<String>,
}

impl Lines {
    /// Where the next line ends, and how many bytes end it. A carriage
    /// return that is the last byte waits: a line feed may follow it.
    fn line_end(&self) -> Option<(usize, usize)> {
        let at = self
            .unread
            .iter()
            .position(|byte| matches!(byte, b'\n' | b'\r'))?;
        match (self.unread[at], self.unread.get(at + 1)) {
            (b'\n', _) => Some((at, 1)),
            (_, None) => None,
            (_, Some(b'\n')) => Some((at, 2)),
            (_, Some(_)) => Some((at, 1)),
        }
    }

    /// The event put together so far, if it has any data.
    fn dispatch(&mut self, into: &mut Vec<Event>) {
        if !self.data.is_empty() {
            into.push(Event {
                kind: match std::mem::take(&mut self.event) {
                    event if event.is_empty() => "message".to_owned(),
                    event => event,
                },
                data: self.data.join("\n"),
                id: self.id.clone(),
            });
            self.data.clear();
        }
        self.event.clear();
    }

    fn feed(&mut self, chunk: &[u8]) -> Vec<Event> {
        self.unread.extend_from_slice(chunk);
        let mut events = Vec::new();
        while let Some((end, skip)) = self.line_end() {
            let line = String::from_utf8_lossy(&self.unread[..end]).into_owned();
            self.unread.drain(..end + skip);
            if line.is_empty() {
                self.dispatch(&mut events);
                continue;
            }
            // A line that starts with a colon is a comment, and a field
            // without one has an empty value.
            let (field, value) = match line.split_once(':') {
                Some(("", _)) => continue,
                Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
                None => (line.as_str(), ""),
            };
            match field {
                "event" => value.clone_into(&mut self.event),
                "data" => self.data.push(value.to_owned()),
                "id" if !value.contains('\0') => self.id = Some(value.to_owned()),
                _ => {}
            }
        }
        events
    }

    /// The end of the stream ends the last line and the last event.
    fn finish(&mut self) -> Vec<Event> {
        let mut events = if self.unread.is_empty() {
            Vec::new()
        } else {
            self.feed(b"\n")
        };
        self.dispatch(&mut events);
        events
    }
}

/// A message of an AWS event stream as an event: the kind of event is its
/// `event`, and what it carries its `data`.
fn of_message(message: &Message) -> Result<Event, String> {
    let header = |name: &str| message.string_header(name).unwrap_or_default().to_owned();
    let payload = || String::from_utf8_lossy(&message.payload).into_owned();
    let (kind, data) = match message.string_header(":message-type").unwrap_or("event") {
        "event" => (header(":event-type"), payload()),
        "exception" => (
            format!("exception:{}", header(":exception-type")),
            payload(),
        ),
        "error" => (
            format!("error:{}", header(":error-code")),
            header(":error-message"),
        ),
        other => return Err(format!("unrecognized :message-type {other:?}")),
    };
    Ok(Event {
        kind,
        data,
        id: None,
    })
}

/// What decodes a body into events, chosen by the body's content type.
pub(super) enum Decoder {
    Lines(Lines),
    Frames(MessageFrameDecoder),
}

impl Decoder {
    pub(super) fn for_content_type(content_type: Option<&str>) -> Self {
        let frames = content_type
            .and_then(|content_type| content_type.split(';').next())
            .is_some_and(|mime| mime.trim().eq_ignore_ascii_case(AWS_EVENT_STREAM));
        if frames {
            Self::Frames(MessageFrameDecoder::new())
        } else {
            Self::Lines(Lines::default())
        }
    }

    /// The events `chunk` completes. Only a binary stream can be
    /// malformed.
    pub(super) fn feed(&mut self, chunk: &[u8]) -> Result<Vec<Event>, String> {
        match self {
            Self::Lines(lines) => Ok(lines.feed(chunk)),
            Self::Frames(frames) => frames
                .feed(chunk)
                .map_err(|error| error.to_string())?
                .iter()
                .map(of_message)
                .collect(),
        }
    }

    /// The events the end of the body completes.
    pub(super) fn finish(&mut self) -> Result<Vec<Event>, String> {
        match self {
            Self::Lines(lines) => Ok(lines.finish()),
            Self::Frames(frames) => frames
                .finish()
                .map(|()| Vec::new())
                .map_err(|error| error.to_string()),
        }
    }
}
