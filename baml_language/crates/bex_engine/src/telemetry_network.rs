//! Network spans around the HTTP sys-ops.
//!
//! The VM opens spans and supplies IO with a `NetworkContext`. Native body
//! readers record through an engine-owned producer without needing a live VM;
//! other platforms drain owned metadata at VM-safe boundaries. Small fetch/send
//! bodies finish at EOF/error, and application reads independently emit await.
//! SSE keeps its existing read/close lifecycle. Unfinished responses end with
//! a drop and cancelled outcome on ownership loss or engine shutdown.

use std::sync::Arc;

use bex_external_types::BexExternalValue;
use bex_heap::{ActiveHeapPermit, HeapPermit as _};
use bex_vm::{
    BexVm,
    telemetry::{ClockInstant, InvocationOutcome, TelemetryId},
};
use bex_vm_types::{Object, SysOp, Value};
use sys_types::network::{NetworkContext, NetworkEventKind, NetworkRequest};

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
    /// `Response.text`/`bytes` or `SseStream.next`/`close` on a body or
    /// stream from the traced request `network`.
    Reads {
        operation: SysOp,
        network: NetworkContext,
    },
}

/// A span that ends with the error being thrown. Its error is captured
/// without the `urls` it may quote: they become the sanitized URLs.
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
            Self::Opened { span, context, url } => context.close().then(|| NetworkClose {
                span: *span,
                urls: quoted_forms(url),
            }),
            Self::Reads { network, .. } => Some(NetworkClose {
                span: take_span(network)?,
                urls: network.response_url().map(quoted_forms).unwrap_or_default(),
            }),
        }
    }
}

/// The ways an error can quote `url`: as the program wrote it, and as
/// `reqwest` prints it, parsed (scheme and host lowercased, default port
/// dropped, empty path made `/`, spaces and other characters
/// percent-encoded). Each one's query is rewritten too.
fn quoted_forms(url: &str) -> Vec<String> {
    let mut forms = vec![url.to_owned()];
    if let Ok(parsed) = url::Url::parse(url)
        && parsed.as_str() != url
    {
        forms.push(parsed.as_str().to_owned());
    }
    forms
}

/// The span of `network`'s request, ending it: once, then `None`.
fn take_span(network: &NetworkContext) -> Option<TelemetryId> {
    network
        .close()
        .then_some(network.span)
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
                    context: NetworkContext::new(
                        span.get(),
                        Arc::new(move || clock.read().get()),
                        Arc::clone(&telemetry.network),
                    ),
                    url: request.url,
                })
            }
            SysOp::BamlHttpResponseBodyText
            | SysOp::BamlHttpResponseBodyBytes
            | SysOp::BamlHttpSseStateNext
            | SysOp::BamlHttpSseStateClose => reads(&thread.vm, operation, *args.first()?),
            _ => None,
        }
    }

    /// Write the events the IO side queued, whatever thread's span they
    /// belong to. Called after every sys-op, before its result reaches the VM,
    /// so a span's events come before the completion that op may write. The
    /// queue drops the events of a span that ended since they were queued.
    /// A terminal `drop` preserves earlier queued events, then completes the
    /// abandoned body as cancelled (not a transport error or a full read).
    pub(crate) fn drain_network_events(&self, vm: &mut BexVm) {
        let Some(telemetry) = &self.telemetry else {
            return;
        };
        for event in telemetry.network.drain() {
            if let Some(span) = TelemetryId::from_raw(event.span) {
                let at = ClockInstant::from_ticks(event.at_ticks);
                if let NetworkEventKind::Complete { error } = &event.kind {
                    vm.complete_network_io(span, at, error.as_ref());
                    continue;
                }
                if event.kind == NetworkEventKind::Cancelled {
                    vm.close_network_span(span, at, InvocationOutcome::Cancelled, None, &[]);
                    continue;
                }
                vm.network_event(span, at, &event.kind);
                if event.kind == NetworkEventKind::Drop {
                    vm.close_network_span(span, at, InvocationOutcome::Cancelled, None, &[]);
                }
            }
        }
    }

    /// Drain the final GC drops and abandon responses still rooted by host handles
    /// or globals after native body readers have stopped and joined.
    pub(crate) fn finish_network_spans(&self) {
        let Some(telemetry) = &self.telemetry else {
            return;
        };
        telemetry.network.abandon_open();
        let events = telemetry.network.drain();
        telemetry.network.seal();
        if events.is_empty() {
            return;
        }
        let Some(mut state) = crate::telemetry_state::EngineTelemetry::new_root(Some(telemetry))
        else {
            return;
        };
        telemetry.clock.validate(state.clock(), true);
        #[cfg(not(target_arch = "wasm32"))]
        let _scope = state.host_recording_scope();
        for event in events {
            if let Some(span) = TelemetryId::from_raw(event.span) {
                let at = ClockInstant::from_ticks(event.at_ticks);
                if let NetworkEventKind::Complete { error } = &event.kind {
                    state.complete_network_io(span, at, error.as_ref());
                    continue;
                }
                if event.kind == NetworkEventKind::Cancelled {
                    // SAFETY: owned cancellation contains no VM values.
                    unsafe {
                        state.close_network_span(span, at, InvocationOutcome::Cancelled, None, &[]);
                    }
                    continue;
                }
                state.network_event(span, at, &event.kind);
                if event.kind == NetworkEventKind::Drop {
                    // SAFETY: a dropped response has no heap-backed error to capture.
                    unsafe {
                        state.close_network_span(span, at, InvocationOutcome::Cancelled, None, &[]);
                    }
                }
            }
        }
        state.complete_thread(InvocationOutcome::Ok);
    }

    /// After an HTTP sys-op succeeded: end the span a finished read or a
    /// `close()` ends. A response its IO side did not trace has no read to
    /// wait for, so its span ends now.
    pub(crate) fn network_after_ok(vm: &mut BexVm, op: &NetworkOp, result: &sys_types::OpValue) {
        let Some(at) = now(vm) else {
            return;
        };
        match op {
            NetworkOp::Opened { span, context, .. } => {
                if !carries(result, *span) && context.close() {
                    vm.close_network_span(*span, at, InvocationOutcome::Ok, None, &[]);
                }
            }
            NetworkOp::Reads { operation, network } => {
                // An application read stays observable even when the native
                // reader already completed the transfer at EOF.
                if matches!(
                    operation,
                    SysOp::BamlHttpResponseBodyText | SysOp::BamlHttpResponseBodyBytes
                ) {
                    network.application_await();
                }
                let end = match operation {
                    SysOp::BamlHttpSseStateClose => NetworkEventKind::Close,
                    // A stream is read to its end when `next()` finds it done.
                    SysOp::BamlHttpSseStateNext
                        if !matches!(
                            result,
                            sys_types::OpValue::External(BexExternalValue::Null)
                        ) =>
                    {
                        return;
                    }
                    _ => NetworkEventKind::Await,
                };
                if let Some(span) = take_span(network) {
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

/// Whether a `Response` or `SseStream` carries `span`: the body or stream it
/// holds belongs to the request.
fn carries(result: &sys_types::OpValue, span: TelemetryId) -> bool {
    let sys_types::OpValue::Instance { fields, .. } = result else {
        return false;
    };
    fields.iter().any(|field| {
        matches!(field, sys_types::OpValue::RustInstance { payload, .. }
            if payload.network().is_some_and(|network| network.span == span.get()))
    })
}

/// A read on a body or stream that belongs to a traced request.
fn reads(vm: &BexVm, operation: SysOp, receiver: Value) -> Option<NetworkOp> {
    let Object::RustInstance(instance) = vm.get_object(receiver.as_object_ptr()?) else {
        return None;
    };
    Some(NetworkOp::Reads {
        operation,
        network: instance.payload.network()?.clone(),
    })
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::Duration,
    };

    use sys_native::SysOpsExt;

    use super::*;

    #[tokio::test]
    async fn native_reader_join_survives_cancelling_the_shutdown_caller() {
        let directory = tempfile::tempdir().unwrap();
        let engine = Arc::new(
            BexEngine::new_with_telemetry_recording(
                baml_test_support::compile_source("function main() -> int { 1 }"),
                Arc::new(sys_native::SysOps::native()),
                vec![],
                None,
                btel_clock::ClockMode::Monotonic,
                crate::TelemetryRecording::local_files_in(
                    directory.path(),
                    btel_recorder::RecordingConfig::default(),
                ),
            )
            .unwrap(),
        );
        let release = sys_types::CancellationToken::new();
        let observed = release.clone();
        let joined = Arc::new(AtomicBool::new(false));
        let finished = Arc::clone(&joined);
        assert!(engine.sys_op_ctx.http_readers.spawn(release, async move {
            observed.cancelled().await;
            tokio::time::sleep(Duration::from_millis(40)).await;
            finished.store(true, Ordering::Release);
        }));
        let started = engine.sys_op_ctx.http_readers.clone();
        // A second controlled reader exposes the exact join phase without
        // timing a GC or assuming a particular executor scheduling order.
        let entered = sys_types::CancellationToken::new();
        let signal = entered.clone();
        assert!(started.spawn(signal.clone(), async move {
            signal.cancelled().await;
        }));
        let closing = Arc::clone(&engine);
        let caller = tokio::spawn(async move {
            closing.shutdown().await;
        });
        tokio::time::timeout(Duration::from_secs(2), entered.cancelled())
            .await
            .unwrap();
        caller.abort();
        let _ = caller.await;
        tokio::time::timeout(Duration::from_secs(2), engine.shutdown())
            .await
            .unwrap();
        assert!(
            joined.load(Ordering::Acquire),
            "recorder cannot close before readers join"
        );
        assert_eq!(engine.telemetry_result(), Some(Ok(())));
    }
}
