//! Incremental SSE (Server-Sent Events) parser.
//!
//! Parses raw bytes into SSE events per the W3C spec:
//! - Fields: `event`, `data`, `id`, `retry`
//! - Events are delimited by blank lines (double newline)
//! - Lines starting with `:` are comments (ignored)

use crate::registry::SseEvent;

/// Incremental SSE parser that buffers incomplete lines.
pub(crate) struct SseParser {
    /// Buffered bytes from incomplete lines.
    buffer: Vec<u8>,
    /// Current event being assembled.
    event_type: String,
    data_lines: Vec<String>,
    id: Option<String>,
}

impl SseParser {
    pub(crate) fn new() -> Self {
        Self {
            buffer: Vec::new(),
            event_type: String::new(),
            data_lines: Vec::new(),
            id: None,
        }
    }

    /// Feed raw bytes into the parser and return any complete events.
    pub(crate) fn feed(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        self.buffer.extend_from_slice(chunk);

        let mut events = Vec::new();

        // Process complete lines (terminated by \n or \r\n or \r)
        loop {
            let line_end = self.find_line_end();
            let Some((end_pos, skip)) = line_end else {
                break;
            };

            let line = String::from_utf8_lossy(&self.buffer[..end_pos]).into_owned();
            self.buffer.drain(..end_pos + skip);

            if line.is_empty() {
                // Blank line = dispatch event if we have data
                if !self.data_lines.is_empty() {
                    let data = self.data_lines.join("\n");
                    events.push(SseEvent {
                        event: if self.event_type.is_empty() {
                            "message".to_string()
                        } else {
                            std::mem::take(&mut self.event_type)
                        },
                        data,
                        id: self.id.take(),
                    });
                    self.data_lines.clear();
                }
                self.event_type.clear();
            } else if line.starts_with(':') {
                // Comment, ignore
            } else if let Some(colon_pos) = line.find(':') {
                let field = &line[..colon_pos];
                // Skip optional space after colon
                let value_start = if line.as_bytes().get(colon_pos + 1) == Some(&b' ') {
                    colon_pos + 2
                } else {
                    colon_pos + 1
                };
                let value = &line[value_start..];

                match field {
                    "event" => self.event_type = value.to_string(),
                    "data" => self.data_lines.push(value.to_string()),
                    "id" => self.id = Some(value.to_string()),
                    "retry" => {} // Ignored for now
                    _ => {}       // Unknown fields ignored per spec
                }
            } else {
                // Field with no value (e.g., "data" alone = "data:")
                match line.as_str() {
                    "data" => self.data_lines.push(String::new()),
                    "event" => self.event_type.clear(),
                    "id" => self.id = Some(String::new()),
                    _ => {}
                }
            }
        }

        events
    }

    /// Find the end of the next line in the buffer.
    /// Returns `(end_position, bytes_to_skip_for_delimiter)`.
    fn find_line_end(&self) -> Option<(usize, usize)> {
        let bytes = self.buffer.as_slice();
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'\n' {
                return Some((i, 1));
            }
            if b == b'\r' {
                // Hold a trailing '\r' so a split CRLF can be recognized correctly.
                if i + 1 == bytes.len() {
                    return None;
                }

                // \r\n or bare \r
                let skip = if bytes.get(i + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
                return Some((i, skip));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_event() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: hello world\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "message");
        assert_eq!(events[0].data, "hello world");
    }

    #[test]
    fn test_named_event() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"event: update\ndata: {\"key\": \"value\"}\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "update");
        assert_eq!(events[0].data, "{\"key\": \"value\"}");
    }

    #[test]
    fn test_multi_line_data() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: line1\ndata: line2\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "line1\nline2");
    }

    #[test]
    fn test_incremental_parsing() {
        let mut parser = SseParser::new();
        let events1 = parser.feed(b"data: hel");
        assert_eq!(events1.len(), 0); // Incomplete line

        let events2 = parser.feed(b"lo\n\n");
        assert_eq!(events2.len(), 1);
        assert_eq!(events2[0].data, "hello");
    }

    #[test]
    fn test_multiple_events() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: first\n\ndata: second\n\n");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "first");
        assert_eq!(events[1].data, "second");
    }

    #[test]
    fn test_comment_ignored() {
        let mut parser = SseParser::new();
        let events = parser.feed(b": this is a comment\ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn test_done_event() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: [DONE]\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "[DONE]");
    }

    #[test]
    fn test_event_with_id() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"id: 42\ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, Some("42".to_string()));
    }

    #[test]
    fn test_split_utf8_multibyte() {
        let mut parser = SseParser::new();
        let events1 = parser.feed(b"data: \xE2\x82");
        assert!(events1.is_empty());

        let events2 = parser.feed(b"\xAC\n\n");
        assert_eq!(events2.len(), 1);
        assert_eq!(events2[0].data, "€");
    }

    #[test]
    fn test_split_crlf_across_chunks() {
        let mut parser = SseParser::new();
        let events1 = parser.feed(b"data: hello\r");
        assert!(events1.is_empty());

        let events2 = parser.feed(b"\n\r\n");
        assert_eq!(events2.len(), 1);
        assert_eq!(events2[0].data, "hello");
    }

    #[test]
    fn test_empty_named_event_resets_event_type() {
        let mut parser = SseParser::new();
        let events1 = parser.feed(b"event: update\n\n");
        assert!(events1.is_empty());

        let events2 = parser.feed(b"data: hello\n\n");
        assert_eq!(events2.len(), 1);
        assert_eq!(events2[0].event, "message");
        assert_eq!(events2[0].data, "hello");
    }
}
