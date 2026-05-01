//! Incremental SSE (Server-Sent Events) parser.
//!
//! Parses raw bytes into SSE events per the W3C spec:
//! - Fields: `event`, `data`, `id`, `retry`
//! - Events are delimited by blank lines (double newline)
//! - Lines starting with `:` are comments (ignored)

/// A single Server-Sent Event.
#[derive(Debug, Clone)]
pub struct SseEvent {
    /// Event type (e.g., "message", "error").
    pub event: String,
    /// Event data payload.
    pub data: String,
    /// Optional event ID.
    pub id: Option<String>,
}

/// Incremental SSE parser that buffers incomplete lines.
pub struct SseParser {
    /// Buffered bytes from incomplete lines.
    buffer: Vec<u8>,
    /// Current event being assembled.
    event_type: String,
    data_lines: Vec<String>,
    id: Option<String>,
}

impl SseParser {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            event_type: String::new(),
            data_lines: Vec::new(),
            id: None,
        }
    }

    /// Feed raw bytes into the parser and return any complete events.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
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
                        id: self.id.clone(),
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
                    "id" if !value.contains('\0') => {
                        self.id = Some(value.to_string());
                    }
                    "id" => {}    // NUL-containing IDs ignored per WHATWG spec
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

    /// Flush any pending event at end-of-stream.
    ///
    /// Per the WHATWG SSE spec, when the stream closes the parser should
    /// behave as if a final newline was received: a partial trailing line is
    /// processed, then any buffered event is dispatched even without the
    /// usual trailing blank line.
    pub fn finish(&mut self) -> Vec<SseEvent> {
        // Treat any remaining bytes as a complete line by appending a newline,
        // then run the standard parser to consume it.
        if !self.buffer.is_empty() {
            self.buffer.push(b'\n');
        }
        let mut events = self.feed(&[]);

        // After processing the last line, an event may still be buffered in
        // `data_lines` if no terminating blank line was seen.
        if !self.data_lines.is_empty() {
            let data = self.data_lines.join("\n");
            events.push(SseEvent {
                event: if self.event_type.is_empty() {
                    "message".to_string()
                } else {
                    std::mem::take(&mut self.event_type)
                },
                data,
                id: self.id.clone(),
            });
            self.data_lines.clear();
        }
        self.event_type.clear();
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

impl Default for SseParser {
    fn default() -> Self {
        Self::new()
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

    #[test]
    fn test_bare_cr_line_endings() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: hello\r\r");
        assert!(events.is_empty());

        let events = parser.feed(b"\r");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn test_crlf_line_endings() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: hello\r\n\r\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn test_mixed_line_endings() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"event: x\ndata: a\r\ndata: b\r\r\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "x");
        assert_eq!(events[0].data, "a\nb");
    }

    #[test]
    fn test_field_no_space_after_colon() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data:no-space\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "no-space");
    }

    #[test]
    fn test_field_multiple_colons() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: key:value:extra\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "key:value:extra");
    }

    #[test]
    fn test_field_value_with_leading_spaces() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data:  two spaces\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, " two spaces");
    }

    #[test]
    fn test_field_name_only_no_colon() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "");
    }

    #[test]
    fn test_unknown_field_ignored() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"foo: bar\ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn test_retry_field_parsed_but_ignored() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"retry: 3000\ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
        assert_eq!(events[0].event, "message");
    }

    #[test]
    fn test_empty_data_field() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data:\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "");
    }

    #[test]
    fn test_empty_data_field_with_space() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: \n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "");
    }

    #[test]
    fn test_comment_only_no_event() {
        let mut parser = SseParser::new();
        let events = parser.feed(b": comment\n\n");
        assert!(events.is_empty());
    }

    #[test]
    fn test_comment_between_data_lines() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: a\n: comment\ndata: b\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "a\nb");
    }

    #[test]
    fn test_empty_comment() {
        let mut parser = SseParser::new();
        let events = parser.feed(b":\n\n");
        assert!(events.is_empty());
    }

    #[test]
    fn test_event_type_cleared_after_dispatch() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"event: custom\ndata: first\n\ndata: second\n\n");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event, "custom");
        assert_eq!(events[0].data, "first");
        assert_eq!(events[1].event, "message");
        assert_eq!(events[1].data, "second");
    }

    #[test]
    fn test_event_type_set_then_overridden() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"event: first\nevent: second\ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "second");
    }

    #[test]
    fn test_id_sticky_across_events() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"id: 1\ndata: a\n\ndata: b\n\n");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, Some("1".to_string()));
        // Per WHATWG spec, id persists until next id: field.
        assert_eq!(events[1].id, Some("1".to_string()));
    }

    #[test]
    fn test_id_sticky_overridden_by_new_id() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"id: 1\ndata: a\n\nid: 2\ndata: b\n\n");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, Some("1".to_string()));
        assert_eq!(events[1].id, Some("2".to_string()));
    }

    #[test]
    fn test_id_with_null_byte_ignored() {
        let mut parser = SseParser::new();
        // NUL-containing id: field is ignored per WHATWG spec.
        let events = parser.feed(b"id: abc\x00def\ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, None);
    }

    #[test]
    fn test_event_split_across_many_chunks() {
        let mut parser = SseParser::new();
        let input = b"data: hello\n\n";
        let mut all_events = Vec::new();
        for &byte in input {
            all_events.extend(parser.feed(&[byte]));
        }
        assert_eq!(all_events.len(), 1);
        assert_eq!(all_events[0].data, "hello");
    }

    #[test]
    fn test_multiple_blank_lines_between_events() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: a\n\n\n\ndata: b\n\n");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "a");
        assert_eq!(events[1].data, "b");
    }

    #[test]
    fn test_no_trailing_blank_line() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: hello\n");
        assert!(events.is_empty());

        let events = parser.feed(b"\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn test_empty_chunk() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"");
        assert!(events.is_empty());
    }

    #[test]
    fn test_large_data_payload() {
        let mut parser = SseParser::new();
        let payload = "x".repeat(100_000);
        let input = format!("data: {payload}\n\n");
        let events = parser.feed(input.as_bytes());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data.len(), 100_000);
        assert_eq!(events[0].data, payload);
    }

    #[test]
    fn test_finish_flushes_pending_event_with_trailing_newline() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: hello\n");
        assert!(events.is_empty());

        let events = parser.finish();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
        assert_eq!(events[0].event, "message");
    }

    #[test]
    fn test_finish_flushes_pending_event_without_trailing_newline() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: hello");
        assert!(events.is_empty());

        let events = parser.finish();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn test_finish_with_no_pending_event_is_empty() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: a\n\n");
        assert_eq!(events.len(), 1);

        let events = parser.finish();
        assert!(events.is_empty());
    }

    #[test]
    fn test_finish_preserves_event_type_and_id() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"id: 42\nevent: custom\ndata: hello\n");
        assert!(events.is_empty());

        let events = parser.finish();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "custom");
        assert_eq!(events[0].data, "hello");
        assert_eq!(events[0].id, Some("42".to_string()));
    }

    #[test]
    fn test_finish_with_partial_unterminated_line() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: line1\ndata: line2");
        assert!(events.is_empty());

        let events = parser.finish();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "line1\nline2");
    }
}
