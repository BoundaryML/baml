/// A single chat message in a rendered prompt: a chat role and its text
/// content. Produced by `PromptAst.messages()` for offline inspection of a
/// rendered prompt (e.g. what `<Fn>$render_prompt` produced).
class PromptMessage {
  /// The chat role, e.g. "system", "user", "assistant". Empty for role-less
  /// content — a prompt with no `${role(...)}` markers that was never
  /// specialized to a client's default role.
  role string
  /// The message's rendered text content. Media parts render as a
  /// human-readable placeholder (e.g. `image::url(...)`).
  content string
}

/// The rendered prompt produced by `<Fn>$render_prompt` and the built-in
/// `prompt` tag: a structural chat-message tree. Use the accessors below to read
/// it as text or structured messages. To inspect the fully-resolved provider
/// request instead (URL, headers, body, remapped roles), use
/// `<Fn>$build_request`.
class PromptAst {
  _data $rust_type

  /// The rendered prompt as readable plain text: each chat message rendered as a
  /// `[role]` header line followed by its content, messages separated by a blank
  /// line (role-less content has no header). This is also what
  /// `string.from(prompt)`, string interpolation, and `to_string` yield.
  function text(self) -> string throws never {
    $rust_io_function
  }

  /// The rendered prompt as an ordered list of chat messages (role + text
  /// content). The primary structured accessor for offline prompt inspection.
  function messages(self) -> PromptMessage[] throws never {
    $rust_io_function
  }

  /// Render readably instead of dumping the opaque handle: `string.from(prompt)`,
  /// string interpolation, and `to_string` all yield `text()`.
  implements baml.ToString {
    function to_string(self) -> string throws never {
      self.text()
    }
  }
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
/// `ctx.client`. Flattened from the primitive client the orchestrator resolves
/// for the current attempt, so `${ctx.client.provider}` etc. read the chosen
/// client's fields directly.
class ContextClient {
  name string
  provider string
  default_role string
  allowed_roles string[]
}

/// BEP-049 §10. Per-render context passed to a `prompt` tag body as `ctx`:
/// per-attempt data (client info, runtime tags, output-format schema) resolved
/// AFTER the orchestrator picks the client for this attempt — preserving lazy
/// eval, exactly like the legacy Jinja runtime it replaced.
///
/// `output_format` is the default-rendered schema string for the call's return
/// type — the dominant `${ctx.output_format}` usage, byte-equivalent to the
/// legacy Jinja `{{ ctx.output_format }}`. `output_format_with(...)` re-renders
/// the schema with caller options (the rare `{{ ctx.output_format(...) }}`
/// form), backed by the opaque `_output_format` handle. The orchestrator
/// populates both once the client is chosen.
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

/// Configures automatic retry behavior for an LLM client.
class RetryPolicy {
  max_retries int
  // Delay fields are optional: a `retry_policy` block may omit them (or use a
  // `strategy { ... }` sub-block the config lowering does not yet expand into
  // these fields), in which case they arrive as `null` at runtime. Reading a
  // `null` in the backoff arithmetic below (`null + 0.0`) is a VM type error,
  // so the `*_or_default` accessors substitute the canonical defaults —
  // 200ms / 1.5x / 10000ms, matching the legacy engine — and return `float`,
  // keeping the retry-delay math well-typed. See B-488.
  initial_delay_ms int?
  multiplier float?
  max_delay_ms int?

  /// Initial backoff delay (ms) as a float; defaults to 200 when unset.
  function initial_delay_or_default(self) -> float throws never {
    (self.initial_delay_ms ?? 200) + 0.0
  }

  /// Backoff growth multiplier; defaults to 1.5 when unset.
  function multiplier_or_default(self) -> float throws never {
    self.multiplier ?? 1.5
  }

  /// Maximum backoff delay (ms) as a float; defaults to 10000 when unset.
  function max_delay_or_default(self) -> float throws never {
    (self.max_delay_ms ?? 10000) + 0.0
  }
}

/// An LLM client (primitive, fallback, or round-robin).
/// Compiled as a global variable from `client<llm>` declarations.
class Client {
  name string
  client_type ClientType
  sub_clients Client[]
  retry RetryPolicy?
  counter int

  /// Build the primitive client this client resolves to.
  ///
  /// `lenient` controls how the generated constructor reads `env.X` client
  /// options: when true, a missing variable yields "" instead of panicking.
  /// Only the offline `render_prompt` path passes `true` — it needs the
  /// client's provider/role metadata to render a prompt but never its
  /// credentials. Every network path keeps the default `false`, so a missing
  /// `api_key` env var still panics for `$call` / `$build_request`.
  function to_primitive_client(self, lenient: bool = false) -> PrimitiveClient {
    if (self.name.includes("/")) {
      from_shorthand(self.name)
    } else {
      self.get_constructor()(lenient)
    }
  }

  function get_constructor(
    self
  ) -> ((bool) -> PrimitiveClient throws root.errors.InvalidArgument) throws never {
    $rust_io_function
  }

  function build_attempt(self) -> OrchestrationStep[] {
    let planner_state = PlannerState { rr_client_names: [] }
    self.build_attempt_with_state(planner_state)
  }

  function build_attempt_with_state(
    self,
    planner_state: PlannerState,
  ) -> OrchestrationStep[] {
    match (self.client_type) {
      ClientType.Primitive => {
        let primitive = self.to_primitive_client();
        [OrchestrationStep { primitive_client: primitive, delay_ms: 0 }]
      }

      ClientType.Fallback => {
        let steps: OrchestrationStep[] = []
        for (let sub in self.sub_clients) {
          for (let step in sub.build_plan_with_state(planner_state)) {
            steps.push(step)
          }
        }
        steps
      }

      ClientType.RoundRobin => {
        if (self.sub_clients.length() == 0) {
          []
        } else {
          let idx = planner_state.rr_pick_index(
            self.name,
            self.counter,
            self.sub_clients.length(),
          );
          self.sub_clients[idx].build_plan_with_state(planner_state)
        }
      }
    }
  }

  function build_plan(self) -> OrchestrationStep[] {
    let planner_state = PlannerState { rr_client_names: [] };
    self.build_plan_with_state(planner_state)
  }

  function build_plan_with_state(
    self,
    planner_state: PlannerState,
  ) -> OrchestrationStep[] {
    match (self.retry) {
      let r: RetryPolicy => {
        let result: OrchestrationStep[] = [];
        let current_delay = r.initial_delay_or_default();

        for (let attempt = 0; attempt <= r.max_retries; attempt += 1) {
          let delay = 0;
          if (attempt > 0) {
            delay = root._trunc_to_int(current_delay)
            let next = current_delay * r.multiplier_or_default();
            if (next > r.max_delay_or_default()) {
              current_delay = r.max_delay_or_default();
            } else {
              current_delay = next;
            }
          }

          let inner = self.build_attempt_with_state(planner_state);
          for (let step in inner) {
            result.push(OrchestrationStep {
              primitive_client: step.primitive_client,
              delay_ms: delay,
            });
          }
        }

        result
      }
      null => self.build_attempt_with_state(planner_state),
    }
  }

  function execute_stream<TStream, TFinal>(
    self,
    context: ExecutionContext,
    inherited_delay_ms: int,
  ) -> Stream<TStream, TFinal> {
    match (self.retry) {
      let r: RetryPolicy => {
        let current_delay = r.initial_delay_or_default()

        for (let attempt = 0; attempt <= r.max_retries; attempt += 1) {
          let attempt_delay = inherited_delay_ms
          if (attempt > 0) {
            attempt_delay = root._trunc_to_int(current_delay)
            let next = current_delay * r.multiplier_or_default()
            if (next > r.max_delay_or_default()) {
              current_delay = r.max_delay_or_default()
            } else {
              current_delay = next
            }
          }

          if (attempt == r.max_retries) {
            attempt_delay = inherited_delay_ms
          }

          let result2: Stream<TStream, TFinal> = self.execute_once_stream<TStream, TFinal>(
            context,
            attempt_delay,
          ) catch (e) {
            // Deterministic render failure (e.g. a legacy Jinja prompt after
            // B-1024): retrying or falling back cannot help, so surface it
            // instead of the generic "All orchestration steps failed".
            root.errors.RenderPrompt { message } => throw root.errors.RenderPrompt { message: message },
            _ => { continue; }
          };

          return result2;
        }

        throw root.errors.DevOther { message: "All orchestration steps failed" };
      }
      null => {
        let result: Stream<TStream, TFinal> = self.execute_once_stream<TStream, TFinal>(
          context,
          inherited_delay_ms,
        );
        return result;
      },
    }
  }

  function execute_once_stream<TStream, TFinal>(
    self,
    context: ExecutionContext,
    active_delay_ms: int,
  ) -> Stream<TStream, TFinal> {
    match (self.client_type) {
      ClientType.Primitive => {
        let primitive = self.to_primitive_client();
        // On the stream path `reflect.type_of<TFinal>()` is `BuiltinUnknown`:
        // the `$stream` companion calls this without explicit type-args and
        // inferred type-args don't reach MIR (see `__make_stream`'s registry
        // fallback). Resolve the real return type from the registry by name and
        // use it for the whole attempt — the closure's `${ctx.output_format}`
        // needs it, and `build_request_stream` keys provider config off it. (The
        // oneshot path instead uses `reflect.type_of<T>()`, which DOES reach
        // MIR there.) Caveat: for a *generic* streaming function the registry
        // holds the unspecialized return type, so `${ctx.output_format}` renders
        // the generic schema — the oneshot path renders the specialized one.
        let return_type = get_return_type(context.function_name);

        // Render + specialize via the shared `render_specialized_prompt` (same
        // path as oneshot and the static `build_request_stream` companion).
        let specialized = primitive.render_specialized_prompt(return_type, context.prompt_closure);

        let http_request = primitive.build_request_stream(specialized, return_type);
        let sse = root.http.fetch_sse(http_request);
        let stream: Stream<TStream, TFinal> = self.__make_stream<TStream, TFinal>(sse);
        return stream;
      }

      ClientType.Fallback => {
        for (let sub in self.sub_clients) {
          let result2: Stream<TStream, TFinal> = sub.execute_stream<TStream, TFinal>(
            context,
            active_delay_ms,
          ) catch (e) {
            // Deterministic render failure (e.g. a legacy Jinja prompt after
            // B-1024): retrying or falling back cannot help, so surface it
            // instead of the generic "All orchestration steps failed".
            root.errors.RenderPrompt { message } => throw root.errors.RenderPrompt { message: message },
            _ => { continue; }
          };
          return result2;
        }
        throw root.errors.DevOther { message: "All orchestration steps failed" };
      }

      ClientType.RoundRobin => {
        if (self.sub_clients.length() == 0) {
          throw root.errors.DevOther { message: "RoundRobin client has no sub-clients" };
        }
        let idx = self.counter % self.sub_clients.length()
        self.counter += 1
        let result3: Stream<TStream, TFinal> = self.sub_clients[idx].execute_stream<TStream, TFinal>(
          context,
          active_delay_ms,
        );
        return result3;
      }
    }
  }

  /// DO NOT CALL FROM USER CODE
  function __make_stream<TStream, TFinal>(self, sse: root.http.SseStream) -> Stream<TStream, TFinal> {
    let primitive = self.to_primitive_client()
    let acc = primitive.new_stream_accumulator()
    let cache: StreamCache<TStream, TFinal> = StreamCache.new(reflect.type_of<TStream>(), reflect.type_of<TFinal>())
    Stream { _client: primitive, _acc: acc, _sse: sse, _cache: cache }
  }

  function execute_oneshot<T>(
    self,
    context: ExecutionContext,
    inherited_delay_ms: int,
  ) -> T {
    match (self.retry) {
      let r: RetryPolicy => {
        let current_delay = r.initial_delay_or_default()

        for (let attempt = 0; attempt <= r.max_retries; attempt += 1) {
          let attempt_delay = inherited_delay_ms
          if (attempt > 0) {
            attempt_delay = root._trunc_to_int(current_delay)
            let next = current_delay * r.multiplier_or_default()
            if (next > r.max_delay_or_default()) {
              current_delay = r.max_delay_or_default()
            } else {
              current_delay = next
            }
          }

          if (attempt == r.max_retries) {
            attempt_delay = inherited_delay_ms
          }

          let result2: T = self.execute_once_oneshot<T>(
            context,
            attempt_delay,
          ) catch (e) {
            // Deterministic render failure (e.g. a legacy Jinja prompt after
            // B-1024): retrying or falling back cannot help, so surface it
            // instead of the generic "All orchestration steps failed".
            root.errors.RenderPrompt { message } => throw root.errors.RenderPrompt { message: message },
            _ => { continue; }
          };

          return result2;
        }

        throw root.errors.DevOther { message: "All orchestration steps failed" };
      }
      null => {
        let result: T = self.execute_once_oneshot<T>(
          context,
          inherited_delay_ms,
        );
        return result;
      },
    }
  }

  function execute_once_oneshot<T>(
    self,
    context: ExecutionContext,
    active_delay_ms: int,
  ) -> T {
    match (self.client_type) {
      ClientType.Primitive => {
        let primitive = self.to_primitive_client()
        let return_type = reflect.type_of<T>()

        // Render + specialize via the shared `render_specialized_prompt`, the
        // same path the static `build_request` companion uses — so execution
        // and preview never drift.
        let specialized = primitive.render_specialized_prompt(return_type, context.prompt_closure)
        let http_request = primitive.build_request(specialized, return_type)
        let http_response = root.http.send(http_request)

        if (http_response.ok()) {
          let body = http_response.text()
          let result: T = primitive.parse<T>(body) catch (e) {
            _ => {
              if (active_delay_ms > 0) {
                root.sys.sleep(root.time.Duration.from_nanoseconds(active_delay_ms * 1000000n))
              }
              throw e;
            }
          };
          result
        } else {
          let error_body = http_response.text() catch (e) {
            _ => { "" }
          };
          if (active_delay_ms > 0) {
            root.sys.sleep(root.time.Duration.from_nanoseconds(active_delay_ms * 1000000n))
          }
          throw root.errors.DevOther { message: "HTTP request failed: " + error_body };
        }
      }

      ClientType.Fallback => {
        for (let sub in self.sub_clients) {
          let result2: T = sub.execute_oneshot<T>(
            context,
            active_delay_ms,
          ) catch (e) {
            // Deterministic render failure (e.g. a legacy Jinja prompt after
            // B-1024): retrying or falling back cannot help, so surface it
            // instead of the generic "All orchestration steps failed".
            root.errors.RenderPrompt { message } => throw root.errors.RenderPrompt { message: message },
            _ => { continue; }
          };
          return result2;
        }
        throw root.errors.DevOther { message: "All orchestration steps failed" };
      }

      ClientType.RoundRobin => {
        if (self.sub_clients.length() == 0) {
          throw root.errors.DevOther { message: "RoundRobin client has no sub-clients" };
        }
        let idx = self.counter % self.sub_clients.length()
        self.counter += 1
        let result3: T = self.sub_clients[idx].execute_oneshot<T>(
          context,
          active_delay_ms,
        );
        return result3;
      }
    }
  }
}
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

// @providers-no-options: openai, openai-generic, openai-responses, ai-gateway-images, ollama, openrouter, fallback, round-robin

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
  provider_options AnthropicOptions | AzureOpenAiOptions | BedrockOptions | GoogleAiOptions | VertexAiOptions | null
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

// @providers: google-ai
/// Provider-specific options for `google-ai` clients.
///
/// The Google Cloud fields are ignored while the client uses the Gemini API.
/// When `enterprise` or a `GOOGLE_GENAI_USE_*` environment flag selects the
/// Vertex backend, they have exactly the same meaning as on `VertexAiOptions`.
class GoogleAiOptions {
  /// Route this client through the Gemini Enterprise backend, an alias for
  /// Vertex AI (google-genai's `GOOGLE_GENAI_USE_ENTERPRISE`). Uses Google
  /// Cloud credentials (ADC), and an unset location defaults to `global`.
  /// Equivalent to setting `GOOGLE_GENAI_USE_ENTERPRISE=true`.
  enterprise bool | null
  credentials string | null
  credentials_content string | null
  location string | null
  project_id string | null
}

// @providers: vertex-ai
/// Provider-specific options for `vertex-ai` clients.
class VertexAiOptions {
  /// Path to a Google credential JSON file (service account, authorized user,
  /// workload identity federation, or impersonated service account). Inline
  /// JSON is not supported here; use `credentials_content` for inline
  /// credentials. When neither is set, Application Default Credentials apply.
  credentials string | null
  /// Inline credential JSON string. Used when `credentials` is not set.
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

  function specialize_prompt(
    self,
    prompt: PromptAst,
  ) -> PromptAst throws root.errors.RenderPrompt | root.errors.LlmClient {
    $rust_io_function
  }

  /// BEP-049 M5: turn a function's prompt into a request-ready (specialized)
  /// `PromptAst`. A `prompt` backtick body compiles to a `(Context) -> PromptAst`
  /// closure rendered against a per-attempt `Context`. A legacy `#"..."#`
  /// function (null closure) can no longer be rendered: its Jinja engine was
  /// removed (B-1024), so it throws with a migration hint. This is the SINGLE
  /// source of truth for prompt rendering: the orchestrator
  /// (`execute_once_oneshot` / `execute_stream`) and the static `render_prompt`
  /// / `build_request` companions both call it, so the playground preview/cURL
  /// and actual execution render byte-identically.
  function render_specialized_prompt(
    self,
    return_type: type,
    prompt_closure: ((Context) -> PromptAst throws never)?,
  ) -> PromptAst throws root.errors.RenderPrompt | root.errors.LlmClient {
    let prompt = match (prompt_closure) {
      null => throw root.errors.RenderPrompt {
        message: "this function's prompt uses the legacy Jinja template syntax (prompt #\"...\"#), which is no longer supported at runtime; rewrite it as a backtick prompt (prompt `...`)",
      },
      let closure: (Context) -> PromptAst throws never => closure(build_prompt_context(self, return_type)),
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

/// BEP-049 §10 (M5d). Assemble a `PromptAst` from a tagged template's flattened
/// `parts` plus each interpolation twice: `values` holds the raw value (so a
/// `Role` marker from `${role("system")}` can start a new chat message and a
/// media value can become a media content part, whatever its host
/// representation), `rendered` holds its `render_prompt_values` string form
/// (used as the text of every other interpolation). With no `Role` values the
/// whole template becomes a single `PromptAst::Simple`. This replaces the
/// magic-delimiter role mechanism of the Jinja path with direct structural
/// assembly.
function assemble_prompt_ast(parts: string[], values: unknown[], rendered: unknown[]) -> PromptAst throws never {
  $rust_io_function
}

/// BEP-049 §11. Coerce a tagged prompt's interpolated `values` for assembly so
/// composites render the same as they do in an ordinary backtick string. The
/// template desugaring pushes each `${expr}` into `values` *raw* (uncoerced),
/// which is the right contract for a general tagged template — but the built-in
/// `prompt` tag stringifies its content, so a class/array/map value reaching the
/// assembler unrendered would silently produce an empty string (B-563). Route
/// every value through `string.from` — BAML's implicit `to_string`, the exact
/// conversion an ordinary `${expr}` applies (honoring `baml.ToString` overrides)
/// — except a `Role` marker (from `${role(...)}`), which stays raw here (the
/// assembler reads roles and media off the separately-passed RAW values array;
/// stringifying a `Role` would be wasted work since its rendered slot is never
/// read). Media interpolations keep their string form here but the assembler
/// ignores it: host-injected media does not inhabit the `baml.media.*` classes,
/// so it cannot be exempted by a class match at this level.
function render_prompt_values(values: unknown[]) -> unknown[] throws never {
  let rendered: unknown[] = []
  for (let value in values) {
    rendered.push(match (value) {
      Role => value,
      _ => string.from(value),
    })
  }
  rendered
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
  body: (role: (string) -> Role throws never, ctx: Context) -> baml.TaggedString throws never,
) -> (Context) -> PromptAst throws never {
  let render = (ctx: Context) -> {
    let t = body(make_role, ctx)
    assemble_prompt_ast(t.parts, t.values, render_prompt_values(t.values))
  }
  render
}

function get_return_type(function_name: string) -> type throws root.errors.InvalidArgument {
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
