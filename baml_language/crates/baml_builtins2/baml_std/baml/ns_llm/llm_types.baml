class PromptAst {
  _data $rust_type
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
  initial_delay_ms int
  multiplier float
  max_delay_ms int
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
  ) -> () -> PrimitiveClient throws root.errors.InvalidArgument {
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
        let current_delay = r.initial_delay_ms + 0.0;

        for (let attempt = 0; attempt <= r.max_retries; attempt += 1) {
          let delay = 0;
          if (attempt > 0) {
            delay = root.math.trunc(current_delay)
            let next = current_delay * r.multiplier;
            if (next > r.max_delay_ms + 0.0) {
              current_delay = r.max_delay_ms + 0.0;
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
        let current_delay = r.initial_delay_ms + 0.0

        for (let attempt = 0; attempt <= r.max_retries; attempt += 1) {
          let attempt_delay = inherited_delay_ms
          if (attempt > 0) {
            attempt_delay = root.math.trunc(current_delay)
            let next = current_delay * r.multiplier
            if (next > r.max_delay_ms + 0.0) {
              current_delay = r.max_delay_ms + 0.0
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
        let return_type = reflect.type_of<TFinal>();

        let prompt = primitive.render_prompt(context.jinja_string, context.args, return_type);
        let specialized = primitive.specialize_prompt(prompt);

        let http_request = primitive.build_request_stream(specialized, return_type);
        let sse = root.http.fetch_sse(http_request);
        let stream: Stream<TStream, TFinal> = self.__make_stream(sse, context.function_name);
        return stream;
      }

      ClientType.Fallback => {
        for (let sub in self.sub_clients) {
          let result2: Stream<TStream, TFinal> = sub.execute_stream<TStream, TFinal>(
            context,
            active_delay_ms,
          ) catch (e) {
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
  function __make_stream<TStream, TFinal>(self, sse: root.http.SseStream, function_name: string) -> Stream<TStream, TFinal> {
    let primitive = self.to_primitive_client()
    let return_type = get_return_type(function_name)
    let stream_return_type = get_stream_return_type(function_name)
    let acc = primitive.new_stream_accumulator()
    let cache: StreamCache<TStream, TFinal> = StreamCache.new(return_type, stream_return_type)
    Stream { _client: primitive, _acc: acc, _sse: sse, _cache: cache }
  }

  function execute_oneshot<T>(
    self,
    context: ExecutionContext,
    inherited_delay_ms: int,
  ) -> T {
    match (self.retry) {
      let r: RetryPolicy => {
        let current_delay = r.initial_delay_ms + 0.0

        for (let attempt = 0; attempt <= r.max_retries; attempt += 1) {
          let attempt_delay = inherited_delay_ms
          if (attempt > 0) {
            attempt_delay = root.math.trunc(current_delay)
            let next = current_delay * r.multiplier
            if (next > r.max_delay_ms + 0.0) {
              current_delay = r.max_delay_ms + 0.0
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

        let prompt = primitive.render_prompt(context.jinja_string, context.args, return_type)
        let specialized = primitive.specialize_prompt(prompt)
        let http_request = primitive.build_request(specialized, return_type)
        let http_response = root.http.send(http_request)

        if (http_response.ok()) {
          let body = http_response.text()
          let result: T = primitive.parse<T>(body) catch (e) {
            _ => {
              if (active_delay_ms > 0) {
                root.sys.sleep(active_delay_ms)
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
            root.sys.sleep(active_delay_ms)
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
  function new(target: type, streaming: type) -> StreamCache<TStream, TFinal> {
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
        throw root.errors.LlmClientError { message: "Streaming finished without finish_reason" };
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

function get_return_type(function_name: string) -> type throws root.errors.InvalidArgument {
  $rust_io_function
}

/// Returns the stream-expanded return type for an LLM function.
///
/// For a function returning `MyClass`, this returns the type that SAP uses for
/// partial streaming (e.g. `null | MyClass$stream`). Used by `__make_stream`
/// to construct a `StreamCache` with the correct streaming type schema.
function get_stream_return_type(function_name: string) -> type throws root.errors.InvalidArgument {
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
