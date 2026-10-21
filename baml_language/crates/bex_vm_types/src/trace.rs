use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use btel_types::{
    InvocationMode, RecordingId, TelemetryId, allocate_telemetry_id, context::ContextPatch,
};

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

    /// Return the recording-local ID only when it belongs to `scope`.
    pub fn local_in(&self, scope: RecordingId) -> Option<TelemetryId> {
        (self.scope == scope).then_some(self.local)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReservationError {
    WrongScope,
    AlreadyAttached,
}

#[derive(Clone, Debug, Default)]
pub struct TraceOptionsData {
    pub mode: Option<InvocationMode>,
    pub inputs: Option<bool>,
    pub output: Option<bool>,
    pub error: Option<bool>,
    pub context: Option<Arc<ContextPatch>>,
}

#[derive(Debug)]
struct ReservationState {
    attached: AtomicBool,
    materialized: AtomicBool,
}

#[derive(Clone, Debug)]
pub struct ReservedSpanData {
    pub options: TraceOptionsData,
    pub id: SpanId,
    state: Arc<ReservationState>,
    // Only the runtime can derive this authority after a successful entry claim.
    // Raw aliases share state, but never gain this bit by observing attachment.
    forwarded_claim: bool,
}

impl ReservedSpanData {
    pub fn new(scope: RecordingId, mut options: TraceOptionsData) -> Self {
        options.mode = Some(InvocationMode::Span);
        Self {
            options,
            id: SpanId::new(scope, allocate_telemetry_id()),
            state: Arc::new(ReservationState {
                attached: AtomicBool::new(false),
                materialized: AtomicBool::new(false),
            }),
            forwarded_claim: false,
        }
    }

    pub fn attach(&self, scope: RecordingId) -> Result<TelemetryId, ReservationError> {
        if self.id.scope != scope {
            return Err(ReservationError::WrongScope);
        }
        self.state
            .attached
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| self.id.local)
            .map_err(|_| ReservationError::AlreadyAttached)
    }

    /// Explicit continuation of a successful claim. Does not mutate the user's
    /// reservation options and never duplicates attachment/materialization state.
    #[must_use]
    pub fn claimed_view(&self, options: TraceOptionsData) -> Self {
        assert!(
            self.state.attached.load(Ordering::Acquire),
            "claim must precede forwarding"
        );
        Self {
            options,
            id: self.id,
            state: self.state.clone(),
            forwarded_claim: true,
        }
    }

    pub fn is_forwarded_claim(&self) -> bool {
        self.forwarded_claim
    }

    /// Identity of the shared claim, not merely its externally visible span ID.
    pub fn shares_claim(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.state, &other.state)
    }

    /// Raw aliases still fail attach; only an explicit internal view continues it.
    pub fn enter(&self, scope: RecordingId) -> Result<TelemetryId, ReservationError> {
        if self.forwarded_claim {
            self.id.local_in(scope).ok_or(ReservationError::WrongScope)
        } else {
            self.attach(scope)
        }
    }

    /// Span creation consumes the existing claim's one creation opportunity,
    /// independently of the earlier public attachment boundary.
    pub fn materialize(&self, scope: RecordingId) -> Result<TelemetryId, &'static str> {
        let id = self
            .id
            .local_in(scope)
            .ok_or("reservation belongs to another runtime")?;
        if !self.forwarded_claim {
            return Err("span creation requires an explicitly forwarded reservation claim");
        }
        self.state
            .materialized
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| id)
            .map_err(|_| "reservation claim already created a span")
    }

    /// Preparation verifies ownership/liveness without claiming the span.
    pub fn validate(&self, scope: RecordingId) -> Result<(), ReservationError> {
        if self.id.scope != scope {
            return Err(ReservationError::WrongScope);
        }
        if !self.forwarded_claim && self.state.attached.load(Ordering::Acquire) {
            return Err(ReservationError::AlreadyAttached);
        }
        Ok(())
    }
}

impl crate::BexRustData for SpanId {
    fn measure(&self, _: &mut crate::Meter) {}
}

impl TraceOptionsData {
    /// What the options keep alive: a context patch, shared with every span
    /// that carries it.
    fn measure_into(&self, meter: &mut crate::Meter) {
        if let Some(context) = &self.context {
            meter.shared(context, |meter| {
                meter.bytes(context.distinct_id.as_ref().map_or(0, String::capacity));
                for (key, value) in &context.metadata {
                    meter.bytes(key.capacity() + size_of_val(value));
                }
            });
        }
    }
}

impl crate::BexRustData for TraceOptionsData {
    fn measure(&self, meter: &mut crate::Meter) {
        self.measure_into(meter);
    }
}

impl crate::BexRustData for ReservedSpanData {
    fn measure(&self, meter: &mut crate::Meter) {
        self.options.measure_into(meter);
    }
}

impl crate::BexRustData for HostMarker {
    fn measure(&self, meter: &mut crate::Meter) {
        meter.bytes(self.definition.retained_bytes());
        self.options.measure_into(meter);
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
            context: None,
        };
        let scope = RecordingId::generate();
        let reserved = ReservedSpanData::new(scope, options.clone());
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
    fn forwarding_preserves_effective_options_and_claim_without_reclaiming() {
        let scope = RecordingId::generate();
        let raw = ReservedSpanData::new(scope, TraceOptionsData::default());
        let raw_alias = raw.clone();
        let id = raw.enter(scope).unwrap();
        let effective = TraceOptionsData {
            inputs: Some(true),
            ..raw.options.clone()
        };
        let forwarded = raw.claimed_view(effective);
        assert_eq!(raw.options.inputs, None);
        assert_eq!(forwarded.options.inputs, Some(true));
        assert_eq!(
            raw_alias.enter(scope),
            Err(ReservationError::AlreadyAttached)
        );
        assert_eq!(forwarded.enter(scope), Ok(id));
        assert_eq!(forwarded.enter(scope), Ok(id));
        assert!(raw.materialize(scope).is_err());
        assert_eq!(forwarded.materialize(scope), Ok(id));
        assert!(forwarded.materialize(scope).is_err());
    }

    #[test]
    fn forwarded_aliases_create_only_one_span() {
        let scope = RecordingId::generate();
        let raw = ReservedSpanData::new(scope, TraceOptionsData::default());
        raw.enter(scope).unwrap();
        let first = raw.claimed_view(raw.options.clone());
        let second = raw.claimed_view(raw.options.clone());
        let winners = std::thread::scope(|threads| {
            let a = threads.spawn(|| usize::from(first.materialize(scope).is_ok()));
            let b = threads.spawn(|| usize::from(second.materialize(scope).is_ok()));
            a.join().unwrap() + b.join().unwrap()
        });
        assert_eq!(winners, 1);
        assert_eq!(
            first.enter(RecordingId::generate()),
            Err(ReservationError::WrongScope)
        );
    }

    #[test]
    fn failed_setup_does_not_release_entry_claim() {
        let scope = RecordingId::generate();
        let raw = ReservedSpanData::new(scope, TraceOptionsData::default());
        raw.enter(scope).unwrap();
        drop(raw.claimed_view(raw.options.clone()));
        assert_eq!(raw.enter(scope), Err(ReservationError::AlreadyAttached));
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

/// Lexical definition and wrapper site, independent of request data and object
/// addresses. Adapters must distinguish explicitly stacked wrapper sites.
#[derive(Clone, Debug)]
pub struct HostDefinition {
    pub language: String,
    pub module: String,
    pub qualified_name: String,
    pub source_file: String,
    pub definition_line: u32,
    pub wrapper_line: u32,
    pub display_name: String,
}

pub type HostDefinitionKey = (String, String, String, String, u32, u32);

impl HostDefinition {
    /// Bytes the definition's names occupy outside the value.
    pub fn retained_bytes(&self) -> usize {
        self.language.capacity()
            + self.module.capacity()
            + self.qualified_name.capacity()
            + self.source_file.capacity()
            + self.display_name.capacity()
    }

    pub fn key(&self) -> HostDefinitionKey {
        (
            self.language.clone(),
            self.module.clone(),
            self.qualified_name.clone(),
            self.source_file.clone(),
            self.definition_line,
            self.wrapper_line,
        )
    }
}

/// A native source coordinate, interned independently of display names.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HostCallSite {
    pub source_file: String,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub struct HostMarker {
    pub definition: HostDefinition,
    pub options: TraceOptionsData,
}

#[derive(Clone, Debug, Default)]
pub struct HostCallOptions {
    pub options: TraceOptionsData,
    pub reserved_id: Option<TelemetryId>,
}
