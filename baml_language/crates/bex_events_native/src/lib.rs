//! Native `EventSink` implementation: background thread + bounded channel,
//! optional JSONL file writer, and stderr rendering for runtime `log.*` events.
//!
//! Call `start(path)` to create a `NativeEventSink` and spawn the publisher thread.
//! When a trace file is configured, all runtime events are buffered in-memory
//! and written as JSONL on `flush()` or when the channel is closed.
//! Structured `log.info()` / `log.debug()` / `log.warn()` / `log.error()` events
//! are additionally rendered to stderr immediately using the same debug format
//! as the `typescript2` playground.
//!
//! **Guaranteed delivery:** Callers must call `flush()` before process shutdown (e.g. before
//! dropping the sink or exiting) to ensure all buffered events are written. The LSP and CFFI
//! bridges do this; short-lived processes that drop the sink without flushing may lose events.
//!
//! This crate does not read env vars — the caller decides where events go.

use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

use bex_events::{EventKind, EventSink, RuntimeEvent};
use time::{OffsetDateTime, format_description::FormatItem, macros::format_description};

/// Messages sent to the publisher thread.
#[allow(clippy::large_enum_variant)]
enum PublisherMessage {
    /// A new event to buffer.
    Event(RuntimeEvent),
    /// Flush buffered events to disk; ack when done.
    Flush(mpsc::SyncSender<()>),
}

const AUTO_FLUSH_INTERVAL: Duration = Duration::from_secs(5);
const AUTO_FLUSH_THRESHOLD: usize = 1024;
const TIMESTAMP_FORMAT: &[FormatItem<'static>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z");

/// Native event sink backed by a bounded channel and a background thread.
///
/// Created via [`start()`]. Implements [`EventSink`] — `send` dispatches to the
/// channel (non-blocking, drops on full), `flush` blocks until the publisher
/// thread writes all buffered events.
pub struct NativeEventSink {
    tx: mpsc::SyncSender<PublisherMessage>,
    dropped: AtomicUsize,
}

impl EventSink for NativeEventSink {
    fn send(&self, event: RuntimeEvent) {
        if self.tx.try_send(PublisherMessage::Event(event)).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn flush(&self) {
        let dropped = self.dropped.swap(0, Ordering::Relaxed);
        if dropped > 0 {
            tracing::warn!(dropped, "bex-publisher: events dropped (channel full)");
        }
        let (ack_tx, ack_rx) = mpsc::sync_channel(1);
        if self.tx.send(PublisherMessage::Flush(ack_tx)).is_ok() {
            let _ = ack_rx.recv_timeout(Duration::from_secs(30));
        }
    }
}

/// Start the native event sink with a JSONL file target.
///
/// Structured `log.*` events are always mirrored to stderr.
pub fn start(trace_file: PathBuf) -> Arc<dyn EventSink> {
    start_inner(Some(trace_file))
}

/// Start the native event sink with stderr-only logging.
pub fn start_stderr() -> Arc<dyn EventSink> {
    start_inner(None)
}

fn start_inner(trace_file: Option<PathBuf>) -> Arc<dyn EventSink> {
    let (tx, rx) = mpsc::sync_channel::<PublisherMessage>(4096);

    std::thread::Builder::new()
        .name("bex-publisher".into())
        .spawn(move || publisher_loop(rx, trace_file))
        .expect("failed to spawn bex-publisher thread");

    Arc::new(NativeEventSink {
        tx,
        dropped: AtomicUsize::new(0),
    })
}

/// The publisher worker loop.
///
/// Auto-flushes when the buffer reaches `AUTO_FLUSH_THRESHOLD` events or
/// when `AUTO_FLUSH_INTERVAL` elapses without an explicit flush, preventing
/// unbounded buffer growth.
#[allow(clippy::needless_pass_by_value)] // rx is moved into this thread and must be owned
fn publisher_loop(rx: mpsc::Receiver<PublisherMessage>, trace_file: Option<PathBuf>) {
    let mut buffer: Vec<RuntimeEvent> = Vec::new();

    // Block on the first message so we don't spin when idle.
    let first = rx.recv();
    match first {
        Ok(PublisherMessage::Event(e)) => {
            write_log_event_to_stderr(&e);
            if trace_file.is_some() {
                buffer.push(e);
            }
        }
        Ok(PublisherMessage::Flush(ack)) => {
            let _ = ack.send(());
        }
        Err(_) => return,
    }

    loop {
        match rx.recv_timeout(AUTO_FLUSH_INTERVAL) {
            Ok(PublisherMessage::Event(e)) => {
                write_log_event_to_stderr(&e);
                if trace_file.is_some() {
                    buffer.push(e);
                    if buffer.len() >= AUTO_FLUSH_THRESHOLD {
                        flush_buffer(&mut buffer, trace_file.as_deref());
                    }
                }
            }
            Ok(PublisherMessage::Flush(ack)) => {
                flush_buffer(&mut buffer, trace_file.as_deref());
                let _ = ack.send(());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                flush_buffer(&mut buffer, trace_file.as_deref());
                // Park until the next message so we don't spin on timeouts when idle.
                match rx.recv() {
                    Ok(PublisherMessage::Event(e)) => {
                        write_log_event_to_stderr(&e);
                        if trace_file.is_some() {
                            buffer.push(e);
                        }
                    }
                    Ok(PublisherMessage::Flush(ack)) => {
                        let _ = ack.send(());
                    }
                    Err(_) => break,
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                flush_buffer(&mut buffer, trace_file.as_deref());
                break;
            }
        }
    }
}

fn flush_buffer(buffer: &mut Vec<RuntimeEvent>, trace_file: Option<&Path>) {
    if let Some(trace_file) = trace_file {
        write_jsonl_to_file(buffer, trace_file);
    }
    buffer.clear();
}

fn write_log_event_to_stderr(event: &RuntimeEvent) {
    if let Some(line) = format_log_event_for_stderr(event) {
        #[allow(clippy::print_stderr)]
        {
            eprintln!("{line}");
        }
    }
}

fn format_log_event_for_stderr(event: &RuntimeEvent) -> Option<String> {
    let EventKind::Log(log) = &event.event else {
        return None;
    };

    let timestamp = format_timestamp(event.timestamp);
    let payload = bex_events::serialize::bex_value_to_debug_string(&log.data);
    Some(format!(
        "[{timestamp}] {} {payload}",
        log.level.to_uppercase()
    ))
}

fn format_timestamp(timestamp: web_time::SystemTime) -> String {
    let Ok(duration) = timestamp.duration_since(web_time::UNIX_EPOCH) else {
        return "unix:0.000".into();
    };

    let nanos =
        i128::from(duration.as_secs()) * 1_000_000_000 + i128::from(duration.subsec_nanos());

    OffsetDateTime::from_unix_timestamp_nanos(nanos)
        .ok()
        .and_then(|dt| dt.format(TIMESTAMP_FORMAT).ok())
        .unwrap_or_else(|| {
            format!(
                "unix:{}.{:03}",
                duration.as_secs(),
                duration.subsec_millis()
            )
        })
}

/// Write buffered events to the given JSONL file (append mode).
fn write_jsonl_to_file(events: &[RuntimeEvent], trace_file: &Path) {
    if events.is_empty() {
        return;
    }
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(trace_file)
    else {
        tracing::warn!(
            ?trace_file,
            "bex-publisher: failed to open trace file, dropping {} events",
            events.len()
        );
        return;
    };
    for event in events {
        let line = bex_events::serialize::event_to_jsonl(event);
        let _ = writeln!(file, "{line}");
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bex_events::{
        CallId, EventKind, FunctionEvent, FunctionStart, RuntimeEvent, SpanContext, SpanId,
    };
    use web_time::SystemTime;

    use super::*;

    fn make_event(span_id: SpanId) -> RuntimeEvent {
        RuntimeEvent {
            call_id: CallId(0),
            ctx: SpanContext {
                span_id: span_id.clone(),
                parent_span_id: None,
                root_span_id: span_id,
            },
            call_stack: vec![],
            timestamp: SystemTime::now(),
            event: EventKind::Function(FunctionEvent::Start(FunctionStart {
                name: "test_fn".into(),
                args: vec![],
                tags: vec![],
            })),
        }
    }

    #[test]
    fn test_emit_and_flush_to_file() {
        let dir = tempfile::tempdir().unwrap();
        let trace_path = dir.path().join("trace.jsonl");

        let sink = start(trace_path.clone());
        let span = SpanId::new();
        sink.send(make_event(span));
        sink.flush();

        let contents = std::fs::read_to_string(&trace_path).unwrap();
        assert!(!contents.is_empty(), "trace file should have content");
        assert!(contents.contains("test_fn"));
    }

    #[test]
    fn test_format_log_event_for_stderr() {
        let span_id = SpanId::new();
        let event = RuntimeEvent {
            call_id: CallId(0),
            ctx: SpanContext {
                span_id: span_id.clone(),
                parent_span_id: None,
                root_span_id: span_id,
            },
            call_stack: vec![],
            timestamp: web_time::UNIX_EPOCH + Duration::from_millis(1_234),
            event: EventKind::Log(bex_events::LogEvent {
                level: "info".into(),
                data: bex_external_types::BexExternalValue::Map {
                    key_type: baml_type::Ty::string(),
                    value_type: baml_type::Ty::string(),
                    entries: indexmap::IndexMap::from([
                        (
                            "status".into(),
                            bex_external_types::BexExternalValue::Variant {
                                enum_name: "State".into(),
                                variant_name: "Ready".into(),
                            },
                        ),
                        (
                            "user".into(),
                            bex_external_types::BexExternalValue::Instance {
                                class_name: "Person".into(),
                                fields: indexmap::IndexMap::from([(
                                    "name".into(),
                                    bex_external_types::BexExternalValue::String("Alice".into()),
                                )]),
                            },
                        ),
                    ]),
                },
                source: None,
            }),
        };

        let line = format_log_event_for_stderr(&event).expect("log events should render");
        assert_eq!(
            line,
            "[1970-01-01T00:00:01.234Z] INFO {status: State.Ready, user: Person { name: \"Alice\" }}"
        );
    }

    #[test]
    fn test_format_log_event_for_stderr_escapes_newlines() {
        // Log payloads that contain literal newlines/tabs must render on a
        // single line, otherwise stderr log readers see the payload split
        // across multiple lines.
        let span_id = SpanId::new();
        let event = RuntimeEvent {
            call_id: CallId(0),
            ctx: SpanContext {
                span_id: span_id.clone(),
                parent_span_id: None,
                root_span_id: span_id,
            },
            call_stack: vec![],
            timestamp: web_time::UNIX_EPOCH + Duration::from_millis(0),
            event: EventKind::Log(bex_events::LogEvent {
                level: "info".into(),
                data: bex_external_types::BexExternalValue::String("first\nsecond\tthird".into()),
                source: None,
            }),
        };

        let line = format_log_event_for_stderr(&event).expect("log events should render");
        assert!(
            !line.contains('\n'),
            "stderr log line must not contain raw newlines: {line:?}"
        );
        assert!(
            line.contains("\\n") && line.contains("\\t"),
            "newline/tab should be escaped in output: {line:?}"
        );
    }
}
