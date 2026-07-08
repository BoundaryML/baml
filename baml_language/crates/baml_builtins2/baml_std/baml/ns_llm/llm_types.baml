class PromptAst {
  _data $rust_type
}

/// BEP-049 §10 (M5b.2). Opaque handle to a prebuilt output-format schema (the
/// rust `OutputFormatContent` for a return type). Carried on `Context` so
/// `ctx.output_format_with(...)` can re-render the schema with caller options
/// without rebuilding it. Built by `build_output_format(return_type)`.
class OutputFormat {
  _data $rust_type
}

/// BEP-049 §10. A typed chat-role marker produced by the in-template `role(...)`
/// constructor (e.g. `${role("system")}`). When a `prompt` tag assembles a
/// `PromptAst`, each `Role` value in the template's `values` array starts a new
/// message — replacing the magic-delimiter role mechanism of the Jinja path.
class Role {
  /// The role name, e.g. "system", "user", "assistant".
  name string
  /// Provider-specific per-message metadata (e.g. cache_control). Opaque to the
  /// type system; interpreted during `PromptAst` assembly / specialization.
  metadata map<string, unknown>
}

/// BEP-049 §10. The resolved-client view exposed to a `prompt` tag body as
/// `ctx.client`. Flattened to mirror the runtime `RenderContextClient` the
/// orchestrator already assembles for the current attempt (sys_llm), so
/// `${ctx.client.provider}` etc. read the chosen client's fields directly.
class ContextClient {
  name string
  provider string
  default_role string
  allowed_roles string[]
}

/// BEP-049 §10. Per-render context passed to a `prompt` tag body as `ctx`.
/// Carries the same per-attempt data BAML's Jinja runtime injects today
/// (client info, runtime tags, output-format schema), resolved AFTER the
/// orchestrator picks the client for this attempt — preserving lazy eval.
///
/// `output_format` is the default-rendered schema string for the call's return
/// type — the dominant `${ctx.output_format}` usage, byte-equivalent to Jinja's
/// `{{ ctx.output_format }}`. `output_format_with(...)` re-renders the schema
/// with caller options (the rare `{{ ctx.output_format(...) }}` form), backed by
/// the opaque `_output_format` handle. The orchestrator populates both once the
/// client is chosen.
class Context {
  /// The client chosen for this attempt.
  client ContextClient
  /// Runtime tags for this call.
  tags map<string, unknown>
  /// The return type's schema, rendered with default options.
  output_format string
  /// Opaque handle to the return type's prebuilt output-format schema, backing
  /// `output_format_with(...)`.
  _output_format OutputFormat

  /// BEP-049 §10 (M5b.2). Render the return type's schema with custom options —
  /// the parameterized form of `output_format`. Each option defaults to `null`
  /// (auto); a supplied value overrides that aspect of the rendering.
  function output_format_with(
    self,
    prefix: string? = null,
    or_splitter: string? = null,
    enum_value_prefix: string? = null,
    hoisted_class_prefix: string? = null,
    always_hoist_enums: bool? = null,
    quote_class_fields: bool? = null,
    hoist_classes: string[]? = null,
    map_style: string? = null,
    render_null_as: string? = null,
  ) -> string throws never {
    $rust_io_function
  }
}

/// The routing strategy of an LLM client.
enum ClientType {
  Primitive
  Fallback
  RoundRobin
}

/// Configures automatic retry behavior for an LLM client. A declared
/// `retry_policy` block lowers to this value, and a client referencing it is
/// wrapped in `baml.ai.Retry { max: max_retries, base_delay_ms: initial_delay_ms }`
/// (DCP Phase C). The delay knobs are optional: an omitted `initial_delay_ms`
/// falls back to `Retry`'s default (100ms, doubling); `multiplier` /
/// `max_delay_ms` are accepted for config compatibility but the combinator's
/// backoff is fixed at ×2 with no cap.
class RetryPolicy {
  max_retries int
  initial_delay_ms int?
  multiplier float?
  max_delay_ms int?
}

/// An LLM client (primitive, fallback, or round-robin).
/// Compiled as a global variable from `client<llm>` declarations.
class Client {
  name string
  client_type ClientType
  sub_clients Client[]
  retry RetryPolicy?
  counter int

  function to_primitive_client(self) -> PrimitiveClient {
    if (self.name.includes("/")) {
      from_shorthand(self.name)
    } else {
      self.get_constructor()()
    }
  }

  function get_constructor(
    self
  ) -> (() -> PrimitiveClient throws root.errors.InvalidArgument) throws never {
    $rust_io_function
  }

  /// DO NOT CALL FROM USER CODE
  function __make_stream<TStream, TFinal>(self, sse: root.http.SseStream) -> Stream<TStream, TFinal> {
    let primitive = self.to_primitive_client()
    let acc = primitive.new_stream_accumulator()
    let cache: StreamCache<TStream, TFinal> = StreamCache.new(reflect.type_of<TStream>(), reflect.type_of<TFinal>())
    Stream { _client: primitive, _acc: acc, _sse: sse, _cache: cache }
  }
}

// NOTE (DCP §1.3, Phase C.3): the legacy orchestrator that used to live here —
// `build_plan`/`build_attempt` (OrchestrationStep expansion), `PlannerState`,
// `ExecutionContext`, and the `execute_{oneshot,stream}` retry/fallback/
// round-robin loops — is retired. Declared strategy configs now lower to the
// new-model combinators (`baml.ai.Fallback`/`RoundRobin`/`Retry`), primitives
// are `baml.ai.Provider`s via the bridge implements blocks in
// `ns_ai/core/legacy.baml`, and all routing happens in the capability match
// inside the `baml.ai.drive_*` companion drivers.
// ---------------------------------------------------------------------------
// @providers annotations
// ---------------------------------------------------------------------------
// The `// @providers: <name>` comments below are NOT regular comments --
// they are parsed at build time by `baml_compiler2_ast/build.rs`
// (`extract_provider_configs`) to associate each provider name with its
// options class. The compiler uses this mapping during `lower_cst` to
// validate and type-check `client<llm>` declarations: when a user writes
// `provider "aws-bedrock"`, the compiler knows the options block should
// match `BedrockOptions`.
//
// `@providers-no-options` lists providers that accept `PrimitiveClientOptions`
// directly and have no dedicated options class.
//
// To add a new provider: add its name here (and a new options class + annotation
// if it has provider-specific options).
// ---------------------------------------------------------------------------

// @providers-no-options: openai, openai-generic, openai-responses, ai-gateway-images, ollama, openrouter, google-ai, fallback, round-robin

class MediaUrlHandler {
  image string | null
  audio string | null
  video string | null
  pdf string | null
}

class PrimitiveClientOptions {
  model string | null
  base_url string | null
  allowed_role_metadata unknown | null
  finish_reason_allow_list string[] | null
  finish_reason_deny_list string[] | null
  supports_streaming bool | null
  default_role string | null
  allowed_roles string[] | null
  remap_roles map<string, string> | null
  api_key string | null
  provider_options AnthropicOptions | AzureOpenAiOptions | BedrockOptions | VertexAiOptions | null
  media_url_handler MediaUrlHandler | null

  // request stuff here
  headers map<string, string>
  query_params map<string, string>
  request_body map<string, unknown>
}

// @providers: azure-openai
/// Provider-specific options for `azure-openai` clients.
class AzureOpenAiOptions {
  resource_name string | null
  deployment_id string | null
  api_version string
  max_tokens int | null
}

// @providers: anthropic
/// Provider-specific options for `anthropic` clients.
class AnthropicOptions {
  max_tokens int | null
}

// @providers: vertex-ai
/// Provider-specific options for `vertex-ai` clients.
class VertexAiOptions {
  credentials string | null
  credentials_content string | null
  location string | null
  project_id string | null
}

// @providers: aws-bedrock
/// Provider-specific options for `aws-bedrock` clients.
class BedrockOptions {
  region string | null
  endpoint_url string | null
  access_key_id string | null
  secret_access_key string | null
  session_token string | null
  profile string | null
  stop_sequences string[] | null
  max_tokens int | null
  temperature float | null
  top_p float | null
}

class StreamAccumulator {
  _handle $rust_type

  function add_events(self, events: string) -> null throws root.errors.LlmClient {
    $rust_io_function
  }

  function content(self) -> string throws root.errors.LlmClient {
    $rust_io_function
  }

  function is_done(self) -> bool throws root.errors.LlmClient {
    $rust_io_function
  }

  function model(self) -> string? throws root.errors.LlmClient {
    $rust_io_function
  }

  function finish_reason(self) -> string? throws root.errors.LlmClient {
    $rust_io_function
  }

  function input_tokens(self) -> int? throws root.errors.LlmClient {
    $rust_io_function
  }

  function output_tokens(self) -> int? throws root.errors.LlmClient {
    $rust_io_function
  }
}

/// DO NOT USE FROM USER CODE
/// Cached data for a stream. Contains type information, assert check cache, etc.
///
/// Intended for use in `baml.llm.Stream`
class StreamCache<TStream, TFinal> {
  _data $rust_type

  /// DO NOT CALL FROM USER CODE
  function new(t_stream: type, t_final: type) -> StreamCache<TStream, TFinal> throws never {
    $rust_io_function
  }
}

/// `Stream` provides a streaming interface for LLM calls.
///
/// It is a pull-based interface: the sse stream is only flushed into the accumulator
/// and the value is only parsed when `next()` or `final()` is called.
///
/// Created by `<LLM function>$stream` companion functions,
/// internally using `baml.llm.stream_llm_function`.
class Stream<TStream, TFinal> {
  _client: PrimitiveClient
  _acc: StreamAccumulator
  _sse: root.http.SseStream
  _cache: StreamCache<TStream, TFinal>

  /// Return the next partial value, or a `StreamFinished` value if the stream is done.
  /// Will only return partial values: if the stream is done, use `final()` to get the final value.
  /// Blocks until a partial value is available.
  function next(self) -> TStream | root.stream.StreamFinished {
    while (true) {
      let next = match (self._sse.next()) {
        null => {
          return root.stream.StreamFinished {};
        }
        let next: string => next,
      };
      self._acc.add_events(next);
      if (self._acc.is_done()) {
        self._sse.close();
        return root.stream.StreamFinished {}; // user should call final() to get the final value
      }

      let content = self._acc.content();
      let parsed: TStream | root.stream.StreamNoYield = __sap_parse_partial<TStream, TFinal>(content, self._cache);
      match (parsed) {
        root.stream.StreamNoYield => {
          continue;
        }
        let parsed: TStream => {
          return parsed;
        }
      }
    }
    root.sys.panic("Unreachable")
  }
  /// Return the final value of the stream.
  /// If the final value is not yet available, this will block until the stream is done.
  /// Calling this twice will return the same value.
  function final(self) -> TFinal {
    while (!self._acc.is_done()) {
      let next = match (self._sse.next()) {
        null => {
          break;
        }
        let next: string => next,
      };
      self._acc.add_events(next);
      if (self._acc.is_done()) {
        self._sse.close();
        break;
      }
    }
    match (self._acc.finish_reason()) {
      null => {
        throw root.errors.LlmClient { message: "Streaming finished without finish_reason" };
      },
      let reason: string => self._client.validate_finish_reason(reason),
    }

    let content = self._acc.content();
    __sap_parse_final<TStream, TFinal>(content, self._cache)
  }
}

class PrimitiveClient {
  name string
  provider string
  options PrimitiveClientOptions

  function render_prompt(
    self,
    template: string,
    args: map<string, unknown>,
    return_type: type,
  ) -> PromptAst throws root.errors.RenderPrompt {
    $rust_io_function
  }

  function specialize_prompt(
    self,
    prompt: PromptAst,
  ) -> PromptAst throws root.errors.RenderPrompt | root.errors.LlmClient {
    $rust_io_function
  }

  /// BEP-049 M5: turn a function's prompt into a request-ready (specialized)
  /// `PromptAst`, dispatching on mode. A new-mode `prompt`…`` compiles to a
  /// `(Context) -> PromptAst` closure rendered against a per-attempt `Context`;
  /// a legacy `#"..."#` function (null closure) renders the stored Jinja
  /// `template`. This is the SINGLE source of truth for prompt rendering: the
  /// orchestrator (`execute_once_oneshot` / `execute_stream`) and the static
  /// `render_prompt` / `build_request` companions both call it, so the playground
  /// preview/cURL and actual execution render byte-identically.
  function render_specialized_prompt(
    self,
    template: string,
    args: map<string, unknown>,
    return_type: type,
    prompt_closure: ((Context) -> PromptAst)?,
  ) -> PromptAst throws root.errors.RenderPrompt | root.errors.LlmClient {
    let prompt = match (prompt_closure) {
      null => self.render_prompt(template, args, return_type),
      let closure: (Context) -> PromptAst => closure(build_prompt_context(self, return_type)),
    };
    self.specialize_prompt(prompt)
  }

  function build_request(
    self,
    prompt: PromptAst,
    return_type: type,
  ) -> root.http.Request throws root.errors.LlmClient {
    $rust_io_function
  }

  function build_request_stream(
    self,
    prompt: PromptAst,
    return_type: type,
  ) -> root.http.Request throws root.errors.LlmClient {
    $rust_io_function
  }

  function new_stream_accumulator(self) -> StreamAccumulator throws root.errors.LlmClient {
    $rust_io_function
  }

  function validate_finish_reason(
    self,
    finish_reason: string,
  ) -> null throws root.errors.LlmClient {
    $rust_io_function
  }

  /// DO NOT CALL FROM USER CODE
  /// Requires the response to be done (no partials).
  ///
  /// The `type_def` parameter is automatically threaded by the compiler from
  /// the call-site type argument `<T>` — callers write `primitive.parse<T>(body)`
  /// without passing an explicit type value.
  function parse<T>(
    self,
    http_response_body: string,
  ) -> T throws root.errors.LlmClient {
    $rust_io_function
  }
}

function from_shorthand(
  shorthand: string,
) -> PrimitiveClient throws root.errors.InvalidArgument {
  $rust_io_function
}

function get_jinja_template(function_name: string) -> string throws root.errors.InvalidArgument {
  $rust_io_function
}

/// BEP-049 §10 (M5d). Assemble a `PromptAst` from a tagged template's flattened
/// `parts`/`values`. A `Role` value (from `${role("system")}`) starts a new chat
/// message; strings — and other scalar values via their string form — accumulate
/// into the current message's content. With no `Role` values the whole template
/// becomes a single `PromptAst::Simple`. This replaces the magic-delimiter role
/// mechanism of the Jinja path with direct structural assembly.
function assemble_prompt_ast(parts: string[], values: unknown[]) -> PromptAst throws never {
  $rust_io_function
}

/// BEP-049 §10 (M5b). Render `return_type`'s schema with default options — the
/// string `ctx.output_format` exposes to a `prompt` body. The orchestrator
/// calls this with the call's return type to populate `Context.output_format`,
/// producing output byte-equivalent to the legacy Jinja `{{ ctx.output_format }}`.
function render_output_format(return_type: type) -> string throws never {
  $rust_io_function
}

/// BEP-049 §10 (M5b.2). Build the opaque output-format schema handle for
/// `return_type`, for `Context._output_format`. The orchestrator builds this
/// once per attempt; `ctx.output_format_with(...)` renders it with options.
function build_output_format(return_type: type) -> OutputFormat throws never {
  $rust_io_function
}

/// BEP-049 §10 (M5e). Assemble the per-attempt `Context` a `prompt`…`` closure
/// is invoked with: the chosen primitive client's view (`ctx.client.*`) plus the
/// return type's output-format schema. Built AFTER the orchestrator picks the
/// client for this attempt, so `${if (ctx.client.provider == ...)}` and
/// `${ctx.output_format}` see the live client/type. Shared by oneshot + stream.
function build_prompt_context(primitive: PrimitiveClient, return_type: type) -> Context throws never {
  Context {
    client: ContextClient {
      name: primitive.name,
      provider: primitive.provider,
      default_role: primitive.options.default_role ?? "system",
      allowed_roles: primitive.options.allowed_roles ?? [],
    },
    tags: {},
    output_format: render_output_format(return_type),
    _output_format: build_output_format(return_type),
  }
}

/// The `role(...)` constructor passed into a `prompt` body lambda. Builds a
/// typed `Role` marker; `prompt` threads this in as the body's `role` parameter.
function make_role(name: string) -> Role throws never {
  Role { name: name, metadata: {} }
}

/// BEP-049 §10. The built-in `prompt` tag: `` prompt`...` `` evaluates to a
/// `(Context) -> PromptAst` closure that the LLM orchestrator invokes after it
/// has resolved the client for the current attempt (lazy per-attempt eval).
///
/// The body lambda receives `role` (a `Role` constructor) and `ctx` (per-render
/// `Context`); `${role("user")}` markers split the rendered template into chat
/// messages. `ctx.output_format` schema injection lands in a later slice.
//baml:tagged_string
function prompt(
  body: (role: (string) -> Role, ctx: Context) -> baml.TaggedString throws never,
) -> (Context) -> PromptAst throws never {
  let render = (ctx: Context) -> {
    let t = body(make_role, ctx)
    assemble_prompt_ast(t.parts, t.values)
  }
  render
}

function get_return_type(function_name: string) -> type throws root.errors.InvalidArgument {
  $rust_io_function
}

/// Flatten a rendered `PromptAst` into a single plain-text string (message contents
/// concatenated, one per line). Used to bridge the rendered prompt into a BAML-authored
/// provider whose `call` takes a `string`. NOTE: role structure is flattened for now —
/// message roles (system/user/…) collapse into the concatenated text.
function prompt_to_text(prompt: PromptAst) -> string throws never {
  $rust_io_function
}

/// True iff the rendered prompt contains any media part (image/audio/pdf/video).
/// The string-taking BAML provider path cannot carry media, so callers route
/// media-bearing prompts down the legacy (media-aware) request builder instead.
function prompt_has_media(prompt: PromptAst) -> bool throws never {
  $rust_io_function
}


/// DO NOT CALL FROM USER CODE
function __sap_parse_final<TStream, TFinal>(json: string, cache: StreamCache<TStream, TFinal>) -> TFinal throws root.errors.LlmClient {
    $rust_io_function
}

/// DO NOT CALL FROM USER CODE
function __sap_parse_partial<TStream, TFinal>(json: string, cache: StreamCache<TStream, TFinal>) -> TStream | root.stream.StreamNoYield throws root.errors.LlmClient {
    $rust_io_function
}
