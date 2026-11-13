//! A `tracing` layer that records spans as BAML spans. The span's invocation
//! state lives in its registry extensions, so `tracing` itself carries it across
//! `.await` and into `Instrument`ed tasks.
use std::{any::TypeId, marker::PhantomData, sync::Mutex};

use serde_json::{Map, Value, json};
pub use tracing::Subscriber;
use tracing::{
    Dispatch, Event, Id,
    field::{Field, Visit},
    span::{Attributes, Record},
};
use tracing_subscriber::layer::Context;
pub use tracing_subscriber::{Layer, registry::LookupSpan};

const END_CLOSED: u32 = 0;
const END_OK: u32 = 1;
const END_ERRORED: u32 = 2;
const END_PANICKED: u32 = 4;

/// Records every span it sees, so filter it with `Layer::with_filter`.
pub struct BamlLayer<S> {
    with_state: WithState,
    subscriber: PhantomData<fn(S)>,
}

impl<S> BamlLayer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    /// The generated SDK's `trace::tracing_layer` initializes the runtime first.
    #[doc(hidden)]
    pub fn new() -> Self {
        Self {
            with_state: WithState(nearest_span::<S>),
            subscriber: PhantomData,
        }
    }
}

impl<S> Default for BamlLayer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Reaches the extensions from a `Dispatch`, which does not name `S`.
struct WithState(fn(&Dispatch, &Id) -> Option<SpanState>);

fn nearest_span<S>(dispatch: &Dispatch, id: &Id) -> Option<SpanState>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let subscriber = dispatch.downcast_ref::<S>()?;
    subscriber
        .span(id)?
        .scope()
        .find_map(|span| span.extensions().get::<SpanRecord>().map(SpanRecord::state))
}

#[derive(Clone, Copy)]
struct SpanState {
    state: u64,
    /// The callback frame the span opened in, 0 if none.
    opened_in_frame: u64,
}

impl SpanState {
    /// A span older than the running callback is not what the callback runs
    /// under: the engine's frame is.
    fn applies_in(self, frame: u64) -> Option<u64> {
        (self.opened_in_frame == frame).then_some(self.state)
    }
}

/// The current span's invocation state, if it applies inside `frame`.
pub(crate) fn current_span_state(frame: u64) -> Option<u64> {
    tracing::Span::current()
        .with_subscriber(|(id, dispatch)| {
            let with_state = dispatch.downcast_ref::<WithState>()?;
            (with_state.0)(dispatch, id)
        })
        .flatten()
        .and_then(|span| span.applies_in(frame))
}

struct SpanRecord {
    span: u64,
    state: u64,
    opened_in_frame: u64,
    observed: Mutex<Observed>,
}

impl SpanRecord {
    fn state(&self) -> SpanState {
        SpanState {
            state: self.state,
            opened_in_frame: self.opened_in_frame,
        }
    }
}

impl Drop for SpanRecord {
    fn drop(&mut self) {
        crate::handle::release_engine_key(self.state);
        crate::handle::release_engine_key(self.span);
    }
}

/// What the span's own body reported: `#[instrument(ret, err)]` events and
/// fields recorded after it opened.
#[derive(Default)]
struct Observed {
    returned: Option<Value>,
    error: Option<Value>,
    late_fields: Vec<(String, Value)>,
}

impl Observed {
    /// The ABI's end code and tagged value. Without a report the span makes no
    /// claim, because a dropped future closes it exactly as a finished one does.
    /// Only an output has room for late fields: an error is its own value, and
    /// the recording format gives a span making no claim no value at all.
    fn end(self, panicking: bool) -> (u32, Option<Value>) {
        if panicking {
            return (END_PANICKED, None);
        }
        if let Some(error) = self.error {
            return (END_ERRORED, Some(error));
        }
        let Some(returned) = self.returned else {
            return (END_CLOSED, None);
        };
        if self.late_fields.is_empty() {
            return (END_OK, Some(returned));
        }
        let mut fields = self.late_fields;
        fields.push(("return".to_string(), returned));
        (END_OK, Some(tagged_map(&fields)))
    }
}

/// `HostValue`'s tagged JSON form, which `bridge_cffi::host_capture` decodes.
fn tagged(value: &Value) -> Value {
    match value {
        Value::Bool(value) => json!(["bool", value]),
        Value::Number(value) => json!(["number", value]),
        Value::String(value) => json!(["string", value]),
        _ => json!(["null"]),
    }
}

fn tagged_map(fields: &[(String, Value)]) -> Value {
    json!(["map", fields])
}

/// Fields split by destination; `baml.`-prefixed names set the span's context.
#[derive(Default)]
struct Fields {
    inputs: Vec<(String, Value)>,
    distinct_id: Option<Value>,
    metadata: Map<String, Value>,
}

impl Fields {
    fn put(&mut self, name: &str, value: Value) {
        if name == "baml.distinct_id" {
            self.distinct_id = Some(value);
        } else if let Some(key) = name.strip_prefix("baml.metadata.") {
            self.metadata.insert(key.to_string(), value);
        } else {
            self.inputs.push((name.to_string(), tagged(&value)));
        }
    }
}

macro_rules! visit_primitives {
    () => {
        fn record_f64(&mut self, field: &Field, value: f64) {
            self.put(field.name(), json!(value));
        }
        fn record_i64(&mut self, field: &Field, value: i64) {
            self.put(field.name(), json!(value));
        }
        fn record_u64(&mut self, field: &Field, value: u64) {
            self.put(field.name(), json!(value));
        }
        fn record_bool(&mut self, field: &Field, value: bool) {
            self.put(field.name(), json!(value));
        }
        fn record_str(&mut self, field: &Field, value: &str) {
            self.put(field.name(), json!(value));
        }
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.put(field.name(), json!(format!("{value:?}")));
        }
    };
}

impl Visit for Fields {
    visit_primitives!();
}

/// The `return` and `error` fields `#[instrument(ret, err)]` emits, the same
/// `error` field `tracing-opentelemetry` maps to span status.
#[derive(Default)]
struct Report {
    returned: Option<Value>,
    error: Option<Value>,
}

impl Report {
    #[expect(
        clippy::needless_pass_by_value,
        reason = "`visit_primitives` calls this and `Fields::put`, which keeps its value"
    )]
    fn put(&mut self, name: &str, value: Value) {
        match name {
            "return" => self.returned = Some(tagged(&value)),
            "error" => self.error = Some(tagged(&value)),
            _ => {}
        }
    }
}

impl Visit for Report {
    visit_primitives!();
}

fn begin(attrs: &Attributes<'_>, inherited: u64, opened_in_frame: u64) -> Option<SpanRecord> {
    let api = crate::capi::loaded_api()?;
    let metadata = attrs.metadata();
    let mut fields = Fields::default();
    attrs.record(&mut fields);
    let inputs = (!fields.inputs.is_empty()).then(|| tagged_map(&fields.inputs).to_string());
    let request = json!({
        "module": metadata.module_path().unwrap_or_else(|| metadata.target()),
        "name": metadata.name(),
        "file": metadata.file().unwrap_or("<unknown>"),
        "line": metadata.line().unwrap_or(0),
        "inherited": inherited,
        "distinct_id": fields.distinct_id,
        "metadata": fields.metadata,
        "inputs": inputs,
    })
    .to_string();
    let (mut span, mut state) = (0, 0);
    // SAFETY: the request is read during the call; the outputs are writable.
    #[expect(unsafe_code)]
    let status = unsafe {
        (api.host_span_begin)(
            request.as_ptr(),
            request.len(),
            &raw mut span,
            &raw mut state,
        )
    };
    if status != 0 {
        log::debug!("BAML did not record tracing span {}", metadata.name());
        return None;
    }
    Some(SpanRecord {
        span,
        state,
        opened_in_frame,
        observed: Mutex::default(),
    })
}

fn finish(record: &SpanRecord) {
    let Some(api) = crate::capi::loaded_api() else {
        return;
    };
    let observed = std::mem::take(
        &mut *record
            .observed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    let (end, value) = observed.end(std::thread::panicking());
    let value = value.map(|value| value.to_string());
    let (value, len) = value
        .as_deref()
        .map_or((std::ptr::null(), 0), |value| (value.as_ptr(), value.len()));
    // SAFETY: the span handle is live and `value` is read during the call.
    #[expect(unsafe_code)]
    unsafe {
        (api.host_span_finish)(record.span, end, value, len);
    }
}

impl<S> Layer<S> for BamlLayer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let frame = crate::host_value::current_frame_state();
        // A filter can hide the parent, so take the nearest recorded ancestor.
        let inherited = if attrs.is_root() {
            0
        } else {
            span.scope()
                .skip(1)
                .find_map(|ancestor| {
                    ancestor
                        .extensions()
                        .get::<SpanRecord>()
                        .map(SpanRecord::state)
                })
                .and_then(|ancestor| ancestor.applies_in(frame))
                .unwrap_or(frame)
        };
        // The FFI call stays outside the extensions lock.
        if let Some(record) = begin(attrs, inherited, frame) {
            span.extensions_mut().insert(record);
        }
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let extensions = span.extensions();
        let Some(record) = extensions.get::<SpanRecord>() else {
            return;
        };
        let mut fields = Fields::default();
        values.record(&mut fields);
        record
            .observed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .late_fields
            .append(&mut fields.inputs);
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let Some(span) = ctx.event_span(event) else {
            return;
        };
        let mut report = Report::default();
        event.record(&mut report);
        if report.returned.is_none() && report.error.is_none() {
            return;
        }
        let extensions = span.extensions();
        let Some(record) = extensions.get::<SpanRecord>() else {
            return;
        };
        let mut observed = record
            .observed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        observed.returned = report.returned.or(observed.returned.take());
        observed.error = report.error.or(observed.error.take());
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(&id) else {
            return;
        };
        let record = span.extensions_mut().remove::<SpanRecord>();
        if let Some(record) = record {
            finish(&record);
        }
    }

    // SAFETY: the pointers are to `self` and its field, as `Layer` requires.
    #[expect(unsafe_code)]
    unsafe fn downcast_raw(&self, id: TypeId) -> Option<*const ()> {
        if id == TypeId::of::<Self>() {
            Some(std::ptr::from_ref(self).cast())
        } else if id == TypeId::of::<WithState>() {
            Some(std::ptr::from_ref(&self.with_state).cast())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observed(returned: Option<&str>, error: Option<&str>, late: &[&str]) -> Observed {
        Observed {
            returned: returned.map(|value| tagged(&json!(value))),
            error: error.map(|value| tagged(&json!(value))),
            late_fields: late
                .iter()
                .map(|name| ((*name).to_string(), tagged(&json!(1))))
                .collect(),
        }
    }

    #[test]
    fn a_span_without_a_report_makes_no_claim() {
        assert_eq!(observed(None, None, &[]).end(false), (END_CLOSED, None));
    }

    #[test]
    fn a_returned_value_is_the_output() {
        assert_eq!(
            observed(Some("x"), None, &[]).end(false),
            (END_OK, Some(json!(["string", "x"])))
        );
    }

    #[test]
    fn an_error_wins_over_a_return_and_carries_only_itself() {
        assert_eq!(
            observed(Some("x"), Some("bad"), &["rows"]).end(false),
            (END_ERRORED, Some(json!(["string", "bad"])))
        );
    }

    #[test]
    fn late_fields_join_the_return_in_a_map() {
        assert_eq!(
            observed(Some("x"), None, &["rows"]).end(false),
            (
                END_OK,
                Some(json!([
                    "map",
                    [["rows", ["number", 1]], ["return", ["string", "x"]]]
                ]))
            )
        );
    }

    #[test]
    fn a_span_making_no_claim_carries_no_value() {
        assert_eq!(
            observed(None, None, &["rows"]).end(false),
            (END_CLOSED, None)
        );
    }

    #[test]
    fn panicking_overrides_a_reported_return() {
        assert_eq!(
            observed(Some("x"), None, &[]).end(true),
            (END_PANICKED, None)
        );
    }

    #[test]
    fn a_span_opened_outside_the_frame_does_not_apply_inside_it() {
        let span = SpanState {
            state: 7,
            opened_in_frame: 0,
        };
        assert_eq!(span.applies_in(0), Some(7));
        assert_eq!(span.applies_in(9), None);
    }

    #[test]
    fn context_fields_are_split_from_inputs() {
        let mut fields = Fields::default();
        fields.put("baml.distinct_id", json!("user"));
        fields.put("baml.metadata.stage", json!("load"));
        fields.put("rows", json!(3));
        assert_eq!(fields.distinct_id, Some(json!("user")));
        assert_eq!(fields.metadata["stage"], json!("load"));
        assert_eq!(
            fields.inputs,
            vec![("rows".to_string(), json!(["number", 3]))]
        );
    }
}
