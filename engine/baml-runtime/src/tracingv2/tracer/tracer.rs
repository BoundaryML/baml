use baml_types::baml_value::BamlValue;
use baml_types::tracing::baml_context::BamlContext;
use baml_types::tracing::events::{
    BamlOptions, ContentId, FunctionEnd, FunctionId, FunctionStart, TraceData, TraceEvent,
    TraceLevel, TraceTags,
};

use std::sync::Arc;
use time::OffsetDateTime;

use crate::RuntimeContext;

#[cfg(not(target_arch = "wasm32"))]
pub use super::super::publisher::TracePublisher;
pub use tracing_core::Level;

pub fn log(
    verbosity: tracing_core::Level,
    callsite: String,
    msg: String,
    fields: serde_json::Value,
    ctx: &RuntimeContext,
) {
    // Try to grab the current trace context; if unavailable bail out.

    let mut tags = ctx.tags.clone();

    // Determine span ID based on the current instrumentation scope.
    let span_id = FunctionId(ctx.);
    let log_event = Arc::new(TraceEvent {
        span_id,
        event_id: ContentId("".to_string()),
        span_chain: Vec::new(),
        timestamp: OffsetDateTime::now_utc(),
        content: TraceData::LogMessage { msg },
        callsite: callsite,
        verbosity: TraceLevel::Info,
        // tags,
        tags: Default::default(),
    });

    // Send a clone of the Arc to the channel.
    // let _ = ctx.tx.send(Arc::clone(&log_event));

    #[cfg(not(target_arch = "wasm32"))]
    {
        // Because 'put' is synchronous yet locking is async, we use a runtime to block.
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _ = rt.block_on(async {
            let mut storage = crate::storage::storage::GLOBAL_TRACE_STORAGE.lock().await;
            storage.put(Arc::clone(&log_event));
        });
    }

    #[cfg(target_arch = "wasm32")]
    {
        // On WASM we can't block, so instead we spawn the task locally.
        wasm_bindgen_futures::spawn_local(async move {
            let mut storage = crate::storage::storage::GLOBAL_TRACE_STORAGE.lock().await;
            storage.put(Arc::clone(&log_event));
        });
    }
}

/// This macro instruments a synchronous function call by sending "span start" and "span end"
/// events using the new LogEvent type. Note that we now use `FunctionStart` and `FunctionEnd`
/// (which live in baml-types) instead of the old TraceSpanStart/TraceSpanEnd.
macro_rules! impl_trace_scope {
    ($new_ctx:ident, $verbosity:ident, $name:ident, $fields:ident, $unwrapped_fn:expr, $then:expr) => {{
        let name = $name.into();
        let start_time = OffsetDateTime::now_utc();

        // Send a span start event.
        let tags = $new_ctx.2.clone();

        let start_event = TraceEvent {
            span_id: $new_ctx.0.to_string(),
            content_span_id: ContentId("".to_string()),
            span_chain: Vec::new(),
            timestamp: start_time,
            content: TraceData::FunctionStart(FunctionStart {
                name: name.clone(),
                // No arguments are provided in this context.
                args: Vec::new(),
                // Default options; adjust if you want to pass extra data.
                options: BamlOptions {
                    type_builder: None,
                    client_registry: None,
                },
            }),
            tags: {
                let mut fields_map = $new_ctx.tags.clone();
                if let serde_json::Value::Object(o) = $fields {
                    fields_map.extend(o);
                }
                fields_map
            },
            callsite: name.clone(),
            verbosity: TraceLevel::Info,
        };
        let _ = ctx.tx.send(Arc::new(start_event));

        let retval = $wrapped_fn;

        // Send a span end event.
        let end_event = TraceEvent {
            span_id,
            content_span_id: ContentId("".to_string()),
            span_chain: Vec::new(),
            timestamp: OffsetDateTime::now_utc(),
            content: TraceData::FunctionEnd(FunctionEnd {
                // Because we cannot (in general) convert the return value to a BamlValue,
                // we use a placeholder. You might convert `retval` if you require this.
                result: Ok(BamlValue::String("".to_string())),
            }),
            tags: {
                let mut fields = tags;
                match $then(&retval) {
                    serde_json::Value::Object(o) => fields.extend(o),
                    _ => (),
                }
                fields
            },
            callsite: name.clone(),
            verbosity: TraceLevel::Info,
        };
        let _ = ctx.tx.send(Arc::new(end_event));
        retval
    }};
}

/// Instruments a synchronous function call with tracing.
pub fn btrace<F, R, G>(
    verbosity: tracing_core::Level,
    name: impl Into<String>,
    fields: serde_json::Value,
    f: F,
    then: G,
) -> R
where
    F: FnOnce() -> R,
    G: FnOnce(&R) -> serde_json::Value,
{
    impl_trace_scope!(new_ctx, verbosity, name, fields, f(), then)
}

/// A trait to add a trace–aware method to futures.
pub trait WithTraceContext: Sized + std::future::Future {
    #[allow(async_fn_in_trait)]
    async fn btrace<F>(
        self,
        verbosity: tracing_core::Level,
        name: impl Into<String>,
        fields: serde_json::Value,
        then: F,
    ) -> <Self as std::future::Future>::Output
    where
        F: FnOnce(&<Self as std::future::Future>::Output) -> serde_json::Value,
    {
        impl_trace_scope!(new_ctx, verbosity, name, fields, self.await, then)
    }
}

// Auto-implement the trait for all futures.
impl<F> WithTraceContext for F where F: std::future::Future {}
