pub enum SpanEvent {
  SetTags,
  InputOutput,
  LlmPromptTemplate,
  LlmRequestCacheHit,
  LlmRequestStart,
  LlmRequestError,
  LlmRequestArgs,
  LlmRequestEnd,
  Variant,
  Exception,
  Unknown,
}

impl From<&str> for SpanEvent {
  fn from(s: &str) -> Self {
    match s {
      "set_tags" => SpanEvent::SetTags,
      "io" => SpanEvent::InputOutput,
      "llm_prompt_template" => SpanEvent::LlmPromptTemplate,
      "llm_request_cache_hit" => SpanEvent::LlmRequestCacheHit,
      "llm_request_start" => SpanEvent::LlmRequestStart,
      "llm_request_error" => SpanEvent::LlmRequestError,
      "llm_request_args" => SpanEvent::LlmRequestArgs,
      "llm_request_end" => SpanEvent::LlmRequestEnd,
      "variant" => SpanEvent::Variant,
      "exception" => SpanEvent::Exception,
      _ => SpanEvent::Unknown,
    }
  }
}

impl From<SpanEvent> for &'static str {
  fn from(val: SpanEvent) -> Self {
    match val {
      SpanEvent::SetTags => "set_tags",
      SpanEvent::InputOutput => "io",
      SpanEvent::LlmPromptTemplate => "llm_prompt_template",
      SpanEvent::LlmRequestCacheHit => "llm_request_cache_hit",
      SpanEvent::LlmRequestStart => "llm_request_start",
      SpanEvent::LlmRequestError => "llm_request_error",
      SpanEvent::LlmRequestArgs => "llm_request_args",
      SpanEvent::LlmRequestEnd => "llm_request_end",
      SpanEvent::Variant => "variant",
      SpanEvent::Exception => "exception",
      SpanEvent::Unknown => "unknown",
    }
  }
}

pub(super) use tracing::event;

// Define a macro that wraps around event!
// This macro is used to record the event and apply it to the log schema
#[macro_export]
macro_rules! baml_event_def {
    ($event_type:ident, $($args:tt),*) => {
        {
            static EVENT_NAME: &str = match super::events::SpanEvent::$event_type {
                super::events::SpanEvent::SetTags => "baml_set_tags",
                super::events::SpanEvent::InputOutput => "baml_io",
                super::events::SpanEvent::LlmPromptTemplate => "baml_llm_prompt_template",
                super::events::SpanEvent::LlmRequestCacheHit => "baml_llm_request_cache_hit",
                super::events::SpanEvent::LlmRequestStart => "baml_llm_request_start",
                super::events::SpanEvent::LlmRequestError => "baml_llm_request_error",
                super::events::SpanEvent::LlmRequestArgs => "baml_llm_request_args",
                super::events::SpanEvent::LlmRequestEnd => "baml_llm_request_end",
                super::events::SpanEvent::Variant => "baml_variant",
                super::events::SpanEvent::Exception => "baml_exception",
                super::events::SpanEvent::Unknown => "baml_unknown",
            };
        super::events::event!(
            name: EVENT_NAME,
            tracing::Level::INFO,
            // Forwards the arguments to the event! macro with comma separated values
            $($args),*
        )
       }
    };
}

#[macro_export]
macro_rules! baml_event {
    ($event_type:ident, $($args:tt),*) => {
        $crate::otel::span_events::$event_type::event(
            $($args),*
        )
    };
}

#[macro_export]
macro_rules! baml_span {
  ($name:expr) => {
    tracing::info_span!("baml_event", function_name = $name)
  };
}
