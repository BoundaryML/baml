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
    /// `_send`, `_fetch` or `_send_sse` opened `span` for a request to
    /// `url`, as sent; its IO side reports through `context`.
    Opened {
        span: TelemetryId,
        context: NetworkContext,
        url: String,
    },
    /// `Response.text`/`bytes` or `SseStream.next`/`close` on `traced` data,
    /// whose response came from `url`.
    Reads {
        operation: SysOp,
        traced: Arc<dyn Any + Send + Sync>,
        url: Option<String>,
    },
}

/// A span that ends with the error being thrown. Its error is captured
/// without the raw `urls` it may quote: they become the sanitized URLs.
pub(crate) struct NetworkClose {
    span: TelemetryId,
    urls: Vec<String>,
}

impl NetworkOp {
    pub(crate) fn context(&self) -> Option<NetworkContext> {
        match self {
            Self::Opened { context, .. } => Some(context.clone()),
            Self::Reads { .. } => None,
        }
    }

    /// The span a failed or cancelled op ends, if it is still open.
    pub(crate) fn span_to_close(&self) -> Option<NetworkClose> {
        match self {
            Self::Opened { span, url, .. } => Some(NetworkClose {
                span: *span,
                urls: vec![url.clone()],
            }),
            Self::Reads { traced, url, .. } => Some(NetworkClose {
                span: take_span(traced)?,
                urls: url.iter().cloned().collect(),
            }),
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
                    url: request.url,
                })
            }
            SysOp::BamlHttpResponseText | SysOp::BamlHttpResponseBytes => {
                reads(&thread.vm, operation, *args.first()?, "_body")
            }
            SysOp::BamlHttpSseStreamNext | SysOp::BamlHttpSseStreamClose => {
                reads(&thread.vm, operation, *args.first()?, "_handle")
            }
            _ => None,
        }
    }

    /// Write the events the IO side queued, whatever thread's span they
    /// belong to. Called after every sys-op, before its result reaches the VM.
    /// Events still queued when the engine shuts down are not written: a
    /// last drain onto the root thread belongs in `shutdown`, with the GC
    /// `drop` hook (see `NetworkTraced`).
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
                    vm.close_network_span(*span, at, InvocationOutcome::Ok, None, &[]);
                }
            }
            NetworkOp::Reads {
                operation, traced, ..
            } => {
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
                    vm.close_network_span(span, at, InvocationOutcome::Ok, None, &[]);
                }
            }
        }
    }

    /// End the span of the sys-op whose error `value` is being thrown. The
    /// program keeps the error as it is; the capture does not quote the raw
    /// request URL.
    pub(crate) fn close_network_on_throw(thread: &mut BexThread, value: Value) {
        let Some(close) = thread.network_close.take() else {
            return;
        };
        let Some(at) = now(&thread.vm) else {
            return;
        };
        let outcome = thread.vm.telemetry_outcome_for_exception(value);
        let urls: Vec<&str> = close.urls.iter().map(String::as_str).collect();
        thread
            .vm
            .close_network_span(close.span, at, outcome, Some(value), &urls);
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

/// A read on a `Response` or `SseStream` whose `field` is traced data.
fn reads(vm: &BexVm, operation: SysOp, receiver: Value, field: &str) -> Option<NetworkOp> {
    let data = instance_field(vm, receiver, field)?.as_object_ptr()?;
    let Object::RustData(data) = vm.get_object(data) else {
        return None;
    };
    NetworkTraced::of(data.as_ref())?;
    let url = instance_field(vm, receiver, "url")
        .and_then(|url| vm.as_string(&url).ok())
        .map(|url| url.as_str().to_owned());
    Some(NetworkOp::Reads {
        operation,
        traced: Arc::clone(data),
        url,
    })
}

/// An instance's field, by name.
fn instance_field(vm: &BexVm, value: Value, name: &str) -> Option<Value> {
    let instance = vm.as_instance(&value).ok()?;
    let Object::Class(class) = vm.get_object(instance.class) else {
        return None;
    };
    let index = class.fields.iter().position(|field| field.name == name)?;
    instance.try_load_field(index)
}
