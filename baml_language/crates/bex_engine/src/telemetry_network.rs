//! Network spans around the HTTP sys-ops.
//!
//! The VM thread writes every record; the IO side only timestamps what
//! happens to a request and queues it. Before `_send`, `_fetch` or
//! `_send_sse` the engine opens the request's span and hands the op a
//! `NetworkContext`. After every sys-op it drains the queue on the VM thread.
//!
//! A span ends when the program finishes reading its response (`text()`,
//! `bytes()`, the `next()` that finds a stream done), closes a stream, or when
//! the op fails or is cancelled: the span then ends with the thrown value, in
//! `inject_sysop_throw`. A response that is never read stays open.

use std::{any::Any, sync::Arc};

use bex_external_types::BexExternalValue;
use bex_heap::{ActiveHeapPermit, HeapPermit as _};
use bex_vm::{
    BexVm,
    telemetry::{ClockInstant, InvocationOutcome, TelemetryId},
};
use bex_vm_types::{Object, SysOp, Value};
use sys_types::network::{NetworkContext, NetworkEventKind, NetworkRequest, NetworkTraced};

use crate::{BexEngine, thread::BexThread};

/// What an HTTP sys-op does to a network span.
pub(crate) enum NetworkOp {
    /// `_send`, `_fetch` or `_send_sse` opened `span`; its IO side reports
    /// through `context`.
    Opened {
        span: TelemetryId,
        context: NetworkContext,
    },
    /// `Response.text`/`bytes` or `SseStream.next`/`close` on `traced` data.
    Reads {
        operation: SysOp,
        traced: Arc<dyn Any + Send + Sync>,
    },
}

impl NetworkOp {
    pub(crate) fn context(&self) -> Option<NetworkContext> {
        match self {
            Self::Opened { context, .. } => Some(context.clone()),
            Self::Reads { .. } => None,
        }
    }

    /// The span a failed or cancelled op ends, if it is still open.
    pub(crate) fn span_to_close(&self) -> Option<TelemetryId> {
        match self {
            Self::Opened { span, .. } => Some(*span),
            Self::Reads { traced, .. } => take_span(traced),
        }
    }
}

fn take_span(traced: &Arc<dyn Any + Send + Sync>) -> Option<TelemetryId> {
    NetworkTraced::of(traced.as_ref())?
        .take_span()
        .and_then(TelemetryId::from_raw)
}

fn now(vm: &BexVm) -> Option<ClockInstant> {
    vm.telemetry_clock().map(|clock| clock.read())
}

impl BexEngine {
    /// Before a sys-op: open the span of a request about to be sent, or find
    /// the span a read may end on its receiver. `None` for every other op,
    /// and when telemetry is off or the auto level is `low`.
    pub(crate) fn network_before(
        &self,
        thread: &mut ActiveHeapPermit<BexThread>,
        operation: SysOp,
        args: &[Value],
    ) -> Option<NetworkOp> {
        let telemetry = self.telemetry.as_ref()?;
        match operation {
            SysOp::BamlHttpSend | SysOp::BamlHttpFetch | SysOp::BamlHttpSendSse => {
                let arg = self.vm_value_to_owned(thread.proof(), *args.first()?);
                let request = request(operation, &arg)?;
                let clock = Arc::clone(thread.vm.telemetry_clock()?);
                let span = thread.vm.open_network_span(&request)?;
                Some(NetworkOp::Opened {
                    span,
                    context: NetworkContext {
                        span: span.get(),
                        now: Arc::new(move || clock.read().get()),
                        sink: Arc::clone(&telemetry.network),
                    },
                })
            }
            SysOp::BamlHttpResponseText | SysOp::BamlHttpResponseBytes => {
                traced_field(&thread.vm, *args.first()?, "_body")
                    .map(|traced| NetworkOp::Reads { operation, traced })
            }
            SysOp::BamlHttpSseStreamNext | SysOp::BamlHttpSseStreamClose => {
                traced_field(&thread.vm, *args.first()?, "_handle")
                    .map(|traced| NetworkOp::Reads { operation, traced })
            }
            _ => None,
        }
    }

    /// Write the events the IO side queued, whatever thread's span they
    /// belong to. Called after every sys-op, before its result reaches the VM.
    pub(crate) fn drain_network_events(&self, vm: &mut BexVm) {
        let Some(telemetry) = &self.telemetry else {
            return;
        };
        for event in telemetry.network.drain() {
            if let Some(span) = TelemetryId::from_raw(event.span) {
                vm.network_event(span, ClockInstant::from_ticks(event.at_ticks), &event.kind);
            }
        }
    }

    /// After an HTTP sys-op succeeded: end the span a finished read or a
    /// `close()` ends. A response its IO side did not trace has no read to
    /// wait for, so its span ends now.
    pub(crate) fn network_after_ok(vm: &mut BexVm, op: &NetworkOp, result: &BexExternalValue) {
        let Some(at) = now(vm) else {
            return;
        };
        match op {
            NetworkOp::Opened { span, .. } => {
                if !carries(result, *span) {
                    vm.close_network_span(*span, at, InvocationOutcome::Ok, None);
                }
            }
            NetworkOp::Reads { operation, traced } => {
                let end = match operation {
                    SysOp::BamlHttpSseStreamClose => NetworkEventKind::Close,
                    // A stream is read to its end when `next()` finds it done.
                    SysOp::BamlHttpSseStreamNext if !matches!(result, BexExternalValue::Null) => {
                        return;
                    }
                    _ => NetworkEventKind::Await,
                };
                if let Some(span) = take_span(traced) {
                    vm.network_event(span, at, &end);
                    vm.close_network_span(span, at, InvocationOutcome::Ok, None);
                }
            }
        }
    }

    /// End the span of the sys-op whose error `value` is being thrown.
    pub(crate) fn close_network_on_throw(thread: &mut BexThread, value: Value) {
        let Some(span) = thread.network_close.take() else {
            return;
        };
        let Some(at) = now(&thread.vm) else {
            return;
        };
        let outcome = thread.vm.telemetry_outcome_for_exception(value);
        thread.vm.close_network_span(span, at, outcome, Some(value));
    }
}

/// The request a `_send`, `_fetch` or `_send_sse` call sends, from its first
/// argument: a `baml.http.Request`, or `_fetch`'s URL.
fn request(operation: SysOp, arg: &BexExternalValue) -> Option<NetworkRequest> {
    if operation == SysOp::BamlHttpFetch {
        return Some(NetworkRequest {
            method: "GET".to_owned(),
            url: text(arg)?,
            ..NetworkRequest::default()
        });
    }
    let BexExternalValue::Instance { fields, .. } = arg else {
        return None;
    };
    let headers = match fields.get("headers") {
        Some(BexExternalValue::Map { entries, .. }) => entries
            .iter()
            .filter_map(|(name, value)| Some((name.clone(), text(value)?)))
            .collect(),
        _ => Vec::new(),
    };
    Some(NetworkRequest {
        method: text(fields.get("method")?)?,
        url: text(fields.get("url")?)?,
        headers,
        body: text(fields.get("body")?)?.into(),
    })
}

fn text(value: &BexExternalValue) -> Option<String> {
    match value {
        BexExternalValue::String(text) => Some(text.as_str().to_owned()),
        _ => None,
    }
}

/// Whether a `Response` or `SseStream` carries `span`.
fn carries(result: &BexExternalValue, span: TelemetryId) -> bool {
    let BexExternalValue::Instance { fields, .. } = result else {
        return false;
    };
    fields.values().any(|field| {
        matches!(field, BexExternalValue::RustData(data)
            if NetworkTraced::of(data.as_ref()).is_some_and(|traced| traced.network().span == span.get()))
    })
}

/// A traced `$rust_type` field of an instance, by name.
fn traced_field(vm: &BexVm, value: Value, name: &str) -> Option<Arc<dyn Any + Send + Sync>> {
    let instance = vm.as_instance(&value).ok()?;
    let Object::Class(class) = vm.get_object(instance.class) else {
        return None;
    };
    let index = class.fields.iter().position(|field| field.name == name)?;
    let field = instance.try_load_field(index)?;
    let Object::RustData(data) = vm.get_object(field.as_object_ptr()?) else {
        return None;
    };
    NetworkTraced::of(data.as_ref()).map(|_| Arc::clone(data))
}
