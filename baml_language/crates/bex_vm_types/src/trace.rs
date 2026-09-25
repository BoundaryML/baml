use std::sync::atomic::{AtomicBool, Ordering};

use btel_types::{InvocationMode, RecordingId, TelemetryId, allocate_telemetry_id};

/// The recording key of a span, exposed to BAML only as an opaque handle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpanId {
    scope: RecordingId,
    local: TelemetryId,
}

impl SpanId {
    pub fn new(scope: RecordingId, local: TelemetryId) -> Self {
        Self { scope, local }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReservationError {
    WrongScope,
    AlreadyAttached,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TraceOptionsData {
    pub mode: Option<InvocationMode>,
    pub inputs: Option<bool>,
    pub output: Option<bool>,
    pub error: Option<bool>,
}

#[derive(Debug)]
pub struct ReservedSpanData {
    pub options: TraceOptionsData,
    pub id: SpanId,
    attached: AtomicBool,
}

impl ReservedSpanData {
    pub fn new(scope: RecordingId, mut options: TraceOptionsData) -> Self {
        options.mode = Some(InvocationMode::Span);
        Self {
            options,
            id: SpanId::new(scope, allocate_telemetry_id()),
            attached: AtomicBool::new(false),
        }
    }

    pub fn attach(&self, scope: RecordingId) -> Result<TelemetryId, ReservationError> {
        if self.id.scope != scope {
            return Err(ReservationError::WrongScope);
        }
        self.attached
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| self.id.local)
            .map_err(|_| ReservationError::AlreadyAttached)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn reservation_snapshots_options_and_forces_span() {
        let options = TraceOptionsData {
            mode: Some(InvocationMode::Hidden),
            inputs: Some(false),
            output: Some(true),
            error: None,
        };
        let scope = RecordingId::generate();
        let reserved = ReservedSpanData::new(scope, options);
        assert_eq!(options.mode, Some(InvocationMode::Hidden));
        assert_eq!(reserved.options.mode, Some(InvocationMode::Span));
        assert_eq!(reserved.options.inputs, Some(false));
        assert_eq!(reserved.options.output, Some(true));
        assert_eq!(reserved.options.error, None);
        let id = reserved.id;
        assert!(reserved.attach(scope).is_ok());
        assert_eq!(
            reserved.attach(scope),
            Err(ReservationError::AlreadyAttached)
        );
        assert_eq!(reserved.id, id);
        assert_ne!(ReservedSpanData::new(scope, options).id, id);
    }

    #[test]
    fn reservation_attachment_is_shared_across_threads() {
        let recording = RecordingId::generate();
        let reservation = Arc::new(ReservedSpanData::new(
            recording,
            TraceOptionsData::default(),
        ));
        let attached = std::thread::scope(|scope| {
            let threads: Vec<_> = (0..8)
                .map(|_| {
                    let alias = Arc::clone(&reservation);
                    scope.spawn(move || usize::from(alias.attach(recording).is_ok()))
                })
                .collect();
            threads
                .into_iter()
                .map(|thread| thread.join().unwrap())
                .sum::<usize>()
        });
        assert_eq!(attached, 1);
        assert_eq!(
            reservation.attach(recording),
            Err(ReservationError::AlreadyAttached)
        );
    }

    #[test]
    fn span_identity_includes_recording_scope_and_local_id() {
        let scope = RecordingId::generate();
        let other_scope = RecordingId::generate();
        let local = allocate_telemetry_id();
        let id = SpanId::new(scope, local);
        assert_eq!(id, SpanId::new(scope, local));
        assert_ne!(id, SpanId::new(other_scope, local));
        assert_ne!(id, SpanId::new(scope, allocate_telemetry_id()));
    }

    #[test]
    fn wrong_runtime_does_not_consume_reservation() {
        let scope = RecordingId::generate();
        let reservation = ReservedSpanData::new(scope, TraceOptionsData::default());
        assert_eq!(
            reservation.attach(RecordingId::generate()),
            Err(ReservationError::WrongScope)
        );
        let local = reservation.attach(scope).unwrap();
        assert_eq!(reservation.id, SpanId::new(scope, local));
        assert_eq!(
            reservation.attach(scope),
            Err(ReservationError::AlreadyAttached)
        );
    }
}
