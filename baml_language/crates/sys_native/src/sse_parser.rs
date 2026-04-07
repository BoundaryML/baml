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

    // ========================================================================
    // Line ending variants
    // ========================================================================

    #[test]
    fn test_bare_cr_line_endings() {
        // A trailing \r is held by the parser to handle split CRLF. We need a
        // follow-up byte (or another \r) to resolve it. In practice this means
        // bare-CR-only streams dispatch on the next chunk.
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: hello\r\r");
        // Second \r is held (could be start of \r\n), so no event yet.
        assert!(events.is_empty());

        // Any non-\n byte resolves the held \r as a bare CR blank line.
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
        // event line uses \n, first data uses \r\n, second data uses bare \r,
        // blank line uses \r\n
        let events = parser.feed(b"event: x\ndata: a\r\ndata: b\r\r\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "x");
        assert_eq!(events[0].data, "a\nb");
    }

    // ========================================================================
    // Field parsing edge cases
    // ========================================================================

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
        // Per spec, only ONE space after colon is stripped.
        // "data:  two spaces" → field="data", value starts at index 6 (colon+space),
        // so value = " two spaces" (one leading space preserved).
        let mut parser = SseParser::new();
        let events = parser.feed(b"data:  two spaces\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, " two spaces");
    }

    #[test]
    fn test_field_name_only_no_colon() {
        // "data" with no colon → treated as field with empty string value.
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
        // "data:" with no value → empty string pushed to data_lines.
        let mut parser = SseParser::new();
        let events = parser.feed(b"data:\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "");
    }

    #[test]
    fn test_empty_data_field_with_space() {
        // "data: " → space is stripped, producing empty string.
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: \n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "");
    }

    // ========================================================================
    // Comment handling
    // ========================================================================

    #[test]
    fn test_comment_only_no_event() {
        // Comment followed by blank line should NOT produce event.
        let mut parser = SseParser::new();
        let events = parser.feed(b": comment\n\n");
        assert!(events.is_empty());
    }

    #[test]
    fn test_comment_between_data_lines() {
        // Comment in middle of event fields doesn't split the event.
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: a\n: comment\ndata: b\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "a\nb");
    }

    #[test]
    fn test_empty_comment() {
        // Line with just ":" is an empty comment. No event produced.
        let mut parser = SseParser::new();
        let events = parser.feed(b":\n\n");
        assert!(events.is_empty());
    }

    // ========================================================================
    // Event type behavior
    // ========================================================================

    #[test]
    fn test_event_type_cleared_after_dispatch() {
        // After dispatching, event_type is cleared. Second event defaults to "message".
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
        // Last event: field wins within the same event block.
        let mut parser = SseParser::new();
        let events = parser.feed(b"event: first\nevent: second\ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "second");
    }

    // ========================================================================
    // ID behavior
    // ========================================================================

    #[test]
    fn test_id_consumed_after_dispatch() {
        // Our impl uses `self.id.take()`, so id is consumed by the first event
        // and not carried forward to the next event (unlike W3C spec which says
        // last event ID persists). This test pins our current behavior.
        let mut parser = SseParser::new();
        let events = parser.feed(b"id: 1\ndata: a\n\ndata: b\n\n");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, Some("1".to_string()));
        assert_eq!(events[1].id, None);
    }

    #[test]
    fn test_id_with_null_byte() {
        // Per W3C spec, id field containing U+0000 should be ignored.
        // Our impl does NOT check for this — it stores the value as-is.
        // This test documents our current (spec-deviating) behavior.
        let mut parser = SseParser::new();
        let events = parser.feed(b"id: abc\x00def\ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        // from_utf8_lossy replaces invalid sequences but \x00 is valid UTF-8
        assert_eq!(events[0].id, Some("abc\x00def".to_string()));
    }

    // ========================================================================
    // Chunking / streaming edge cases
    // ========================================================================

    #[test]
    fn test_event_split_across_many_chunks() {
        // Feed one byte at a time.
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
        // Extra blank lines should NOT produce extra events (no data = no dispatch).
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: a\n\n\n\ndata: b\n\n");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "a");
        assert_eq!(events[1].data, "b");
    }

    #[test]
    fn test_no_trailing_blank_line() {
        // Without a blank line, the event is not dispatched — data is buffered.
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: hello\n");
        assert!(events.is_empty());

        // Now send the blank line to dispatch.
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
}
