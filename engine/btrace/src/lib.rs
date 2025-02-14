use baml_types::tracing::{
    SpanId, TraceEvent, TraceLog, TraceMetadata, TraceSpanEnd, TraceSpanStart, TraceTags,
};
use time::OffsetDateTime;
#[cfg(not(target_arch = "wasm32"))]
mod tracer_thread;

#[cfg(not(target_arch = "wasm32"))]
pub use tracer_thread::TracerThread;
pub use tracing_core::Level;

#[derive(Clone, Debug)]
pub enum InstrumentationScope {
    Root,
    Child { parent_span_id: SpanId },
}

#[derive(Clone)]
pub struct TraceContext {
    /// The scope used for all spans/logs within this context.
    pub scope: InstrumentationScope,
    /// The channel used to send trace events to the trace agent.
    pub tx: tokio::sync::mpsc::UnboundedSender<TraceEvent>,
    pub tags: TraceTags,
}

impl TraceContext {
    fn child_ctx(&self) -> (Self, SpanId) {
        let new_uuid = format!("span_{}", uuid::Uuid::now_v7());
        let span_id = match &self.scope {
            InstrumentationScope::Root => SpanId(vec![new_uuid]),
            InstrumentationScope::Child { parent_span_id } => {
                let mut parent_span_id = parent_span_id.clone();
                parent_span_id.0.push(new_uuid);
                parent_span_id
            }
        };
        (
            Self {
                scope: InstrumentationScope::Child {
                    parent_span_id: span_id.clone(),
                },
                tx: self.tx.clone(),
                tags: self.tags.clone(),
            },
            span_id,
        )
    }
}

#[cfg(not(target_arch = "wasm32"))]
tokio::task_local! {
  pub static BAML_TRACE_CTX: TraceContext;
}
#[cfg(target_arch = "wasm32")]
thread_local! {
  pub static BAML_TRACE_CTX: TraceContext = TraceContext {
    scope: InstrumentationScope::Root,
    tx: tokio::sync::mpsc::unbounded_channel().0,
    tags: serde_json::Map::new(),
  };
}
// -------------------------------------------------------------------------------------------------

// impl TraceSpanStart {
//     pub fn new(
//         verbosity: tracing_core::Level,
//         callsite: String,
//         fields: serde_json::Value,
//     ) -> Self {
//         Self {
//             span_id: SpanId(vec![format!("span_{}", uuid::Uuid::now_v7())]),
//             start_time: web_time::Instant::now(),
//             meta: TraceMetadata {
//                 callsite,
//                 verbosity,
//             },
//             fields: match fields {
//                 serde_json::Value::Object(o) => o,
//                 _ => serde_json::Map::new(),
//             },
//         }
//     }
// }

pub fn log(
    verbosity: tracing_core::Level,
    callsite: String,
    msg: String,
    fields: serde_json::Value,
) {
    let Ok(ctx) = BAML_TRACE_CTX.try_with(|ctx| ctx.clone()) else {
        return;
    };
    let mut tags = ctx.tags.clone();
    match fields {
        serde_json::Value::Object(o) => tags.extend(o),
        _ => (),
    }
    let _ = ctx.tx.send(TraceEvent::Log(TraceLog {
        span_id: match ctx.scope {
            InstrumentationScope::Root => SpanId(vec![]),
            InstrumentationScope::Child { parent_span_id } => parent_span_id,
        },
        log_id: format!("log_{}", uuid::Uuid::now_v7()),
        start_time: OffsetDateTime::now_utc(),
        msg,
        meta: TraceMetadata {
            callsite,
            verbosity: verbosity.into(),
        },
        fields: tags,
    }));
}

macro_rules! impl_trace_scope {
    ($new_ctx:ident, $verbosity:ident, $name:ident, $fields:ident, $wrapped_fn:expr, $unwrapped_fn:expr, $then:expr) => {{
        let curr_ctx = BAML_TRACE_CTX.try_with(|ctx| ctx.clone());

        match curr_ctx {
            Ok(ctx) => {
                let ($new_ctx, span_id) = ctx.child_ctx();

                let name = $name.into();
                let start_time = OffsetDateTime::now_utc();
                let meta = TraceMetadata {
                    callsite: name,
                    verbosity: $verbosity.into(),
                };
                let tags = $new_ctx.tags.clone();
                let span = TraceSpanStart {
                    span_id: span_id.clone(),
                    start_time,
                    meta: meta.clone(),
                    fields: {
                        let mut fields = $new_ctx.tags.clone();
                        match $fields {
                            serde_json::Value::Object(o) => fields.extend(o),
                            _ => (),
                        }
                        fields
                    },
                };
                let _ = ctx.tx.send(TraceEvent::SpanStart(span));

                let retval = $wrapped_fn;

                let span = TraceSpanEnd {
                    span_id,
                    meta,
                    start_time,
                    end_time: OffsetDateTime::now_utc(),
                    fields: {
                        let mut fields = tags;
                        match $then(&retval) {
                            serde_json::Value::Object(o) => fields.extend(o),
                            _ => (),
                        }
                        fields
                    },
                };
                let _ = ctx.tx.send(TraceEvent::SpanEnd(span));
                retval
            }
            Err(_) => $unwrapped_fn,
        }
    }};
}

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
    impl_trace_scope!(
        new_ctx,
        verbosity,
        name,
        fields,
        BAML_TRACE_CTX.sync_scope(new_ctx, f),
        f(),
        then
    )
}

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
        impl_trace_scope!(
            new_ctx,
            verbosity,
            name,
            fields,
            BAML_TRACE_CTX.scope(new_ctx, self).await,
            self.await,
            then
        )
    }
}

// Auto-implement the trait for all futures
impl<F> WithTraceContext for F where F: std::future::Future {}
