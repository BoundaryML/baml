mod api_wrapper;
#[cfg(not(feature = "wasm"))]
mod threaded_tracer;
#[cfg(feature = "wasm")]
mod wasm_tracer;

use anyhow::Result;
use baml_types::BamlValue;
use indexmap::IndexMap;
use std::collections::HashMap;

use uuid::Uuid;

use crate::{FunctionResult, RuntimeContext, TestResponse};

use self::api_wrapper::{
    core_types::{EventChain, IOValue, LogSchema, LogSchemaContext, TypeSchema, IO},
    APIWrapper,
};
#[cfg(not(feature = "wasm"))]
use self::threaded_tracer::ThreadedTracer;

#[cfg(feature = "wasm")]
use self::wasm_tracer::NonThreadedTracer;

#[cfg(not(feature = "wasm"))]
type TracerImpl = ThreadedTracer;
#[cfg(feature = "wasm")]
type TracerImpl = NonThreadedTracer;

pub struct TracingSpan {
    span_id: Uuid,
    function_name: String,
    params: IndexMap<String, BamlValue>,
    parent_ids: Option<Vec<(String, Uuid)>>,
    start_time: web_time::SystemTime,
    #[allow(dead_code)]
    ctx: RuntimeContext,
}

pub struct BamlTracer {
    options: APIWrapper,
    enabled: bool,
    tracer: Option<TracerImpl>,
}

impl BamlTracer {
    pub fn new(options: Option<APIWrapper>, ctx: &RuntimeContext) -> Self {
        let options = options.unwrap_or_else(|| ctx.into());

        let tracer = BamlTracer {
            tracer: if options.enabled() {
                Some(TracerImpl::new(
                    &options,
                    if options.stage() == "test" { 1 } else { 20 },
                ))
            } else {
                None
            },
            enabled: options.enabled(),
            options,
        };
        tracer
    }

    pub(crate) fn flush(&self) -> Result<()> {
        if let Some(tracer) = &self.tracer {
            tracer.flush()
        } else {
            Ok(())
        }
    }

    pub(crate) fn start_span(
        &self,
        function_name: &str,
        ctx: &RuntimeContext,
        params: &IndexMap<String, BamlValue>,
        parent: Option<&TracingSpan>,
    ) -> Option<TracingSpan> {
        if !self.enabled {
            return None;
        }
        Some(TracingSpan {
            span_id: Uuid::new_v4(),
            function_name: function_name.to_string(),
            params: params.clone(),
            parent_ids: parent.map(|p| vec![(p.function_name.clone(), p.span_id)]),
            start_time: web_time::SystemTime::now(),
            ctx: ctx.clone(),
        })
    }

    pub(crate) async fn finish_span(
        &self,
        span: TracingSpan,
        response: Option<BamlValue>,
    ) -> Result<()> {
        if let Some(tracer) = &self.tracer {
            tracer.submit((&self.options, span, response).into()).await
        } else {
            Ok(())
        }
    }

    pub(crate) async fn finish_baml_span(
        &self,
        span: TracingSpan,
        response: &Result<FunctionResult>,
    ) -> Result<()> {
        if let Some(tracer) = &self.tracer {
            tracer.submit((&self.options, span, response).into()).await
        } else {
            Ok(())
        }
    }

    pub(crate) async fn finish_test_span(
        &self,
        span: TracingSpan,
        response: &Result<TestResponse>,
    ) -> Result<()> {
        if let Some(tracer) = &self.tracer {
            tracer.submit((&self.options, span, response).into()).await
        } else {
            Ok(())
        }
    }
}

// Function to convert web_time::SystemTime to ISO 8601 string
fn to_iso_string(web_time: &web_time::SystemTime) -> String {
    let time = web_time.duration_since(web_time::UNIX_EPOCH).unwrap();
    // Convert to ISO 8601 string
    chrono::DateTime::from_timestamp_millis(time.as_millis() as i64)
        .unwrap()
        .to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
}

impl From<(&APIWrapper, &TracingSpan)> for LogSchemaContext {
    fn from((api, span): (&APIWrapper, &TracingSpan)) -> Self {
        let parents = &span.parent_ids.as_ref();
        let mut parent_chain = parents
            .map(|p| {
                p.iter()
                    .map(|(name, id)| EventChain {
                        function_name: name.clone(),
                        variant_name: Some(id.to_string()),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        parent_chain.push(EventChain {
            function_name: span.function_name.clone(),
            variant_name: None,
        });
        LogSchemaContext {
            hostname: api.host_name().to_string(),
            stage: Some(api.stage().to_string()),
            latency_ms: span
                .start_time
                .elapsed()
                .map(|d| d.as_millis() as i128)
                .unwrap_or(0),
            process_id: api.session_id().to_string(),
            tags: HashMap::new(),
            event_chain: parent_chain,
            start_time: to_iso_string(&span.start_time),
        }
    }
}

impl From<&IndexMap<String, BamlValue>> for IOValue {
    fn from(items: &IndexMap<String, BamlValue>) -> Self {
        IOValue {
            r#type: TypeSchema {
                name: api_wrapper::core_types::TypeSchemaName::Multi,
                fields: items
                    .iter()
                    // TODO: @hellovai do better types
                    .map(|(k, _v)| (k.clone(), "unknown".into()))
                    .collect::<IndexMap<_, _>>(),
            },
            value: api_wrapper::core_types::ValueType::List(
                items
                    .iter()
                    .map(|(_, v)| serde_json::to_string(v).unwrap())
                    .collect::<Vec<_>>(),
            ),
            r#override: None,
        }
    }
}

impl From<&BamlValue> for IOValue {
    fn from(value: &BamlValue) -> Self {
        match value {
            BamlValue::Map(obj) => {
                let fields = obj
                    .iter()
                    .map(|(k, _v)| (k.clone(), "unknown".into()))
                    .collect::<IndexMap<_, _>>();
                IOValue {
                    r#type: TypeSchema {
                        name: api_wrapper::core_types::TypeSchemaName::Multi,
                        fields,
                    },
                    value: api_wrapper::core_types::ValueType::List(
                        obj.iter()
                            .map(|(_, v)| {
                                serde_json::to_string(&serde_json::json!(v))
                                    .unwrap_or_else(|_| "<unknown>".to_string())
                            })
                            .collect::<Vec<_>>(),
                    ),
                    r#override: None,
                }
            }
            _ => IOValue {
                r#type: TypeSchema {
                    name: api_wrapper::core_types::TypeSchemaName::Single,
                    fields: [("value".into(), "unknown".into())].into(),
                },
                value: api_wrapper::core_types::ValueType::String(
                    serde_json::to_string(&serde_json::json!(value))
                        .unwrap_or_else(|_| "<unknown>".to_string()),
                ),
                r#override: None,
            },
        }
    }
}

impl From<(&APIWrapper, TracingSpan, Option<BamlValue>)> for LogSchema {
    fn from((api, span, result): (&APIWrapper, TracingSpan, Option<BamlValue>)) -> Self {
        let parent_ids = &span.parent_ids.as_ref();
        LogSchema {
            project_id: api.project_id().map(|s| s.to_string()),
            event_type: api_wrapper::core_types::EventType::FuncCode,
            root_event_id: parent_ids
                .and_then(|p| p.first().map(|(_, id)| *id))
                .unwrap_or(span.span_id)
                .to_string(),
            event_id: span.span_id.to_string(),
            parent_event_id: parent_ids.and_then(|p| p.last().map(|(_, id)| id.to_string())),
            context: (api, &span).into(),
            io: IO {
                input: Some((&span.params).into()),
                output: result.as_ref().map(|r| r.into()),
            },
            error: None,
            metadata: None,
        }
    }
}

fn error_from_result(result: &Result<FunctionResult>) -> Option<api_wrapper::core_types::Error> {
    match result {
        Ok(r) if r.parsed.is_some() => None,
        Ok(r) => Some(api_wrapper::core_types::Error {
            code: 2,
            message: r
                .parsed
                .as_ref()
                .and_then(|r| r.as_ref().err().map(|e| e.to_string()))
                .or_else(|| r.llm_response.content().err().map(|e| e.to_string()))
                .unwrap_or_else(|| "Unknown error".to_string()),
            traceback: None,
            r#override: None,
        }),
        Err(e) => Some(api_wrapper::core_types::Error {
            code: 2,
            message: e.to_string(),
            traceback: None,
            r#override: None,
        }),
    }
}

impl From<(&APIWrapper, TracingSpan, &Result<FunctionResult>)> for LogSchema {
    fn from((api, span, result): (&APIWrapper, TracingSpan, &Result<FunctionResult>)) -> Self {
        LogSchema {
            project_id: api.project_id().map(|s| s.to_string()),
            event_type: api_wrapper::core_types::EventType::FuncCode,
            root_event_id: span.span_id.to_string(),
            event_id: span.span_id.to_string(),
            parent_event_id: None,
            context: (api, &span).into(),
            io: IO {
                input: Some((&span.params).into()),
                output: result
                    .as_ref()
                    .ok()
                    .and_then(|r| r.parsed.as_ref())
                    .and_then(|r| r.as_ref().ok())
                    .map(|r| {
                        let v: BamlValue = r.into();
                        (&v).into()
                    }),
            },
            error: error_from_result(result),
            metadata: None,
        }
    }
}
impl From<(&APIWrapper, TracingSpan, &Result<TestResponse>)> for LogSchema {
    fn from((api, span, result): (&APIWrapper, TracingSpan, &Result<TestResponse>)) -> Self {
        match result {
            Ok(r) => (api, span, &r.function_response).into(),
            Err(e) => LogSchema {
                project_id: api.project_id().map(|s| s.to_string()),
                event_type: api_wrapper::core_types::EventType::FuncCode,
                root_event_id: span.span_id.to_string(),
                event_id: span.span_id.to_string(),
                parent_event_id: None,
                context: (api, &span).into(),
                io: IO {
                    input: Some((&span.params).into()),
                    output: None,
                },
                error: Some(api_wrapper::core_types::Error {
                    code: 2,
                    message: e.to_string(),
                    traceback: None,
                    r#override: None,
                }),
                metadata: None,
            },
        }
    }
}
