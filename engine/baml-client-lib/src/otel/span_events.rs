mod events;
mod exception;
mod io;
mod llm_cache_hit;
mod llm_prompt_template;
mod llm_request_args;
mod llm_request_end;
mod llm_request_error;
mod llm_request_start;
mod partial_types;
mod set_tags;
mod variant;

use anyhow::Result;

use tracing::Subscriber;
use tracing_subscriber::layer::Layer;

use crate::api_wrapper::core_types::{EventChain, LogSchemaContext};

use self::events::SpanEvent;
use self::exception::Exception;
use self::llm_prompt_template::LlmPromptTemplate;
use self::llm_request_args::LlmRequestArgs;
use self::llm_request_end::LlmRequestEnd;
use self::llm_request_error::LlmRequestError;
use self::partial_types::Apply;

pub(crate) use self::{
    io::IOEvent, llm_cache_hit::LlmRequestCacheHit, llm_request_start::LlmRequestStart,
    set_tags::SetTags, variant::Variant,
};

use super::event_thread::BatchProcessor;

// Macro to record and apply the event to the log schema
macro_rules! record_and_apply {
    ($event_type:ident, $event:ident, $span:ident) => {{
        let mut visitor = $event_type::default();
        $event.record(&mut visitor);
        let mut extension = $span.extensions_mut();
        match extension.get_mut::<partial_types::PartialLogSchema>() {
            Some(metadata) => metadata.apply(visitor, $span),
            None => panic!("No partial log schema found for span"),
        }
    }};
}

fn parse_event<S>(
    event: &tracing::Event<'_>,
    span: &tracing_subscriber::registry::SpanRef<'_, S>,
) -> Result<()>
where
    S: Subscriber,
    S: for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    let event_name = event.metadata().name();
    match SpanEvent::from(event_name) {
        SpanEvent::SetTags => record_and_apply!(SetTags, event, span),
        SpanEvent::InputOutput => record_and_apply!(IOEvent, event, span),
        SpanEvent::LlmPromptTemplate => record_and_apply!(LlmPromptTemplate, event, span),
        SpanEvent::LlmRequestCacheHit => record_and_apply!(LlmRequestCacheHit, event, span),
        SpanEvent::LlmRequestStart => record_and_apply!(LlmRequestStart, event, span),
        SpanEvent::LlmRequestError => record_and_apply!(LlmRequestError, event, span),
        SpanEvent::LlmRequestArgs => record_and_apply!(LlmRequestArgs, event, span),
        SpanEvent::LlmRequestEnd => record_and_apply!(LlmRequestEnd, event, span),
        SpanEvent::Variant => record_and_apply!(Variant, event, span),
        SpanEvent::Exception => record_and_apply!(Exception, event, span),
        SpanEvent::Unknown => Err(anyhow::anyhow!("Unknown event type: {}", event_name))?,
    };

    Ok(())
}

pub(super) struct BamlEventSubscriber<'a> {
    config: &'a mut BatchProcessor,
}

impl<'a> BamlEventSubscriber<'a> {
    pub fn new(config: &'a mut BatchProcessor) -> Self {
        Self { config }
    }
}

#[derive(Default)]
struct FunctionName {
    function_name: Option<String>,
}

impl tracing::field::Visit for FunctionName {
    fn record_debug(&mut self, field: &tracing::field::Field, _value: &dyn std::fmt::Debug) {
        // By defaul invalid
        panic!("unexpected field name: {}", field.name());
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        match field.name() {
            "function_name" => {
                self.function_name = Some(value.to_string());
            }
            name => {
                panic!("unexpected field name: {}", name);
            }
        }
    }
}

impl<S> Layer<S> for BamlEventSubscriber<'static>
where
    S: Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        // Get all parents

        let span = ctx.span(id).unwrap();
        let mut parents = vec![];

        {
            let mut parent = span.parent();
            while let Some(p) = &parent {
                let name = {
                    let ext = p.extensions();
                    let val = ext.get::<partial_types::PartialLogSchema>();
                    match val {
                        Some(p) => p
                            .context
                            .event_chain
                            .last()
                            .map(|v| v.function_name.clone()),
                        None => None,
                    }
                };
                parents.push(name.unwrap_or("<unknown>".to_string()));
                parent = p.parent();
            }
        }
        let function_name = {
            let mut visitor = FunctionName::default();
            attrs.record(&mut visitor);
            visitor.function_name
        };

        let function_name = match function_name {
            Some(name) => name,
            None => return,
        };
        let event_id = uuid::Uuid::new_v4().to_string();
        let (parent_id, root_event_id, mut event_chain, tags) = span
            .parent()
            .map(|parent| {
                parent
                    .extensions()
                    .get::<partial_types::PartialLogSchema>()
                    .map(|p| {
                        (
                            Some(p.event_id.clone()),
                            p.root_event_id.clone(),
                            p.context.event_chain.clone(),
                            p.context.tags.clone(),
                        )
                    })
                    .unwrap_or((
                        None,
                        event_id.clone(),
                        vec![],
                        std::collections::HashMap::new(),
                    ))
            })
            .unwrap_or_default();

        event_chain.push(EventChain {
            function_name,
            variant_name: None,
        });

        // Create a new partial log schema for the span
        span.extensions_mut()
            .insert(partial_types::PartialLogSchema {
                project_id: self.config.api().project_id().map(|v| v.to_string()),
                event_id,
                root_event_id,
                parent_event_id: parent_id,
                context: LogSchemaContext {
                    hostname: self.config.api().host_name().to_string(),
                    stage: Some(self.config.api().stage().to_string()),
                    process_id: self.config.api().session_id().to_string(),
                    event_chain,
                    tags,
                    ..Default::default()
                },
                ..Default::default()
            });
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
        if let Some(span_id) = ctx.current_span().id() {
            if let Some(span) = ctx.span(span_id) {
                if let Err(e) = parse_event(event, &span) {
                    eprintln!("Error parsing event: {:?}", e);
                }
            }
        }
    }

    fn on_exit(&self, _id: &tracing::span::Id, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        // Do nothing
    }

    fn on_close(&self, id: tracing::span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        let span = ctx.span(&id).unwrap();
        let mut extension = span.extensions_mut();
        let schema = match extension.get_mut::<partial_types::PartialLogSchema>() {
            Some(metadata) => metadata,
            None => {
                return;
            }
        };
        match schema.to_final() {
            Ok(log_schema) => {
                for schema in log_schema {
                    // Submit to a background thread that will send the log schema to the server
                    match self.config.submit(schema) {
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("Error submitting log schema: {:?}", e);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Error converting to final log schema: {:?}", e);
            }
        }
    }
}

impl Drop for BamlEventSubscriber<'_> {
    fn drop(&mut self) {
        match self.config.stop() {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error BAML thread: {:?}", e);
            }
        }
    }
}
