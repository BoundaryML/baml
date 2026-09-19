//! Incremental, single-owner event building for one engine's marker stream.
//! Input ranges contain whole records. Arrival may interleave OS sources and
//! batches arbitrarily. No execution-ready signal or stack nesting is required.
//! Closed spans are emitted immediately; only unmatched halves are retained.
pub mod event;
pub mod open_span_store;

use btel_core::{
    marker::{self, DecodeError, Marker},
    stage::{EventBuilder, MarkerRange},
};
pub use event::{CallKey, ClosedSpan, OpenSpan, SpanEnd, SpanStart, TelemetryEvent};
pub use open_span_store::OpenSpanStore;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildErrorKind {
    Decode(DecodeError),
    DuplicateHalf(CallKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildError {
    pub source_id: u64,
    pub byte_offset: usize,
    pub kind: BuildErrorKind,
}

#[derive(Default)]
pub struct TelemetryEventBuilder {
    open: OpenSpanStore,
}

impl TelemetryEventBuilder {
    pub fn open_spans(&self) -> &OpenSpanStore {
        &self.open
    }

    /// End of an input session preserves unresolved facts; it never invents
    /// closing timestamps or waits for absent markers.
    pub fn into_open_spans(self) -> OpenSpanStore {
        self.open
    }

    /// Decode and emit incrementally. On error, the valid prefix has already
    /// been processed. The caller must report failure, not retry the whole range.
    pub fn try_build(
        &mut self,
        input: MarkerRange<'_>,
        mut emit: impl FnMut(TelemetryEvent),
    ) -> Result<(), BuildError> {
        let mut offset = 0;
        while offset < input.bytes.len() {
            let (marker, consumed) =
                marker::decode(&input.bytes[offset..]).map_err(|error| BuildError {
                    source_id: input.source_id,
                    byte_offset: offset,
                    kind: BuildErrorKind::Decode(error),
                })?;
            self.accept(input.source_id, marker, &mut emit)
                .map_err(|key| BuildError {
                    source_id: input.source_id,
                    byte_offset: offset,
                    kind: BuildErrorKind::DuplicateHalf(key),
                })?;
            offset += consumed;
        }
        Ok(())
    }

    fn accept(
        &mut self,
        source_id: u64,
        marker: Marker<'_>,
        mut emit: impl FnMut(TelemetryEvent),
    ) -> Result<(), CallKey> {
        let (key, half) = match marker {
            Marker::FunctionEnter {
                flags,
                thread_id,
                call_id,
                parent_call_id,
                function_id,
                call_site,
                ts_ticks,
            } => (
                CallKey { thread_id, call_id },
                OpenSpan::Start(SpanStart {
                    source_id,
                    flags,
                    parent_call_id,
                    function_id,
                    call_site,
                    ticks: ts_ticks,
                }),
            ),
            Marker::FunctionExit {
                status,
                thread_id,
                call_id,
                ts_ticks,
            } => (
                CallKey { thread_id, call_id },
                OpenSpan::End(SpanEnd {
                    source_id,
                    status,
                    ticks: ts_ticks,
                    await_ns: 0,
                    await_count: 0,
                }),
            ),
            Marker::FunctionExitAwaited {
                status,
                thread_id,
                call_id,
                ts_ticks,
                await_ns,
                await_count,
            } => (
                CallKey { thread_id, call_id },
                OpenSpan::End(SpanEnd {
                    source_id,
                    status,
                    ticks: ts_ticks,
                    await_ns,
                    await_count,
                }),
            ),
            Marker::BexThreadStart {
                flags,
                thread_id,
                parent_thread_id,
                parent_call_id,
                ts_ticks,
                name,
            } => {
                emit(TelemetryEvent::ThreadStarted {
                    source_id,
                    flags,
                    thread_id,
                    parent_thread_id,
                    parent_call_id,
                    ticks: ts_ticks,
                    spawn_site: None,
                    name: name.to_vec(),
                });
                return Ok(());
            }
            Marker::BexThreadStartSpawned {
                flags,
                thread_id,
                parent_thread_id,
                parent_call_id,
                ts_ticks,
                spawn_site,
                name,
            } => {
                emit(TelemetryEvent::ThreadStarted {
                    source_id,
                    flags,
                    thread_id,
                    parent_thread_id,
                    parent_call_id,
                    ticks: ts_ticks,
                    spawn_site: Some(spawn_site),
                    name: name.to_vec(),
                });
                return Ok(());
            }
            Marker::BexThreadEnd {
                status,
                thread_id,
                ts_ticks,
            } => {
                emit(TelemetryEvent::ThreadEnded {
                    source_id,
                    thread_id,
                    ticks: ts_ticks,
                    status,
                });
                return Ok(());
            }
            Marker::SetBoundaryLocalId {
                thread_id,
                call_id,
                id,
                ts_ticks,
            } => {
                emit(TelemetryEvent::BoundaryLocalId {
                    source_id,
                    key: CallKey { thread_id, call_id },
                    ticks: ts_ticks,
                    id,
                });
                return Ok(());
            }
        };
        if let Some(span) = self.open.insert(key, half)? {
            emit(TelemetryEvent::ClosedSpan(span));
        }
        Ok(())
    }
}

impl EventBuilder for TelemetryEventBuilder {
    type Event = TelemetryEvent;

    fn finish(&mut self, mut emit: impl FnMut(Self::Event)) {
        for (key, half) in self.open.drain() {
            emit(TelemetryEvent::UnmatchedSpan { key, half });
        }
    }

    /// Adapter for the infallible stage contract: corrupt committed input is
    /// fatal. Drivers that report errors without panicking use `try_build`.
    fn build(&mut self, input: MarkerRange<'_>, emit: impl FnMut(Self::Event)) {
        self.try_build(input, emit)
            .expect("invalid committed markers");
    }
}
