class PromptAst {
  _data $rust_type
}

enum ClientType {
  Primitive
  Fallback
  RoundRobin
}

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
      throw root.errors.InvalidArgument { message: "Invalid short hand name: " + self.name };
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
        let resolve_fn = self.get_constructor();
        let primitive = resolve_fn();
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
      r: RetryPolicy => {
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

  function execute<T>(
    self,
    context: ExecutionContext,
    inherited_delay_ms: int,
  ) -> T {
    match (self.retry) {
      r: RetryPolicy => {
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

          let result2: T = self.execute_once(
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
        let result: T = self.execute_once(
          context,
          inherited_delay_ms,
        );
        return result;
      },
    }
  }

  function execute_once<T>(
    self,
    context: ExecutionContext,
    active_delay_ms: int,
  ) -> T {
    match (self.client_type) {
      ClientType.Primitive => {
        let resolve_fn = self.get_constructor()
        let primitive = resolve_fn()
        let return_type = get_return_type(context.function_name)

        let prompt = primitive.render_prompt(context.jinja_string, context.args, return_type)
        let specialized = primitive.specialize_prompt(prompt)
        let http_request = primitive.build_request(specialized)
        let http_response = root.http.send(http_request)

        if (http_response.ok()) {
          let body = http_response.text()
          let result: T = primitive.parse(body, return_type) catch (e) {
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
          let result2: T = sub.execute(
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
        let idx = self.counter % self.sub_clients.length()
        self.counter += 1
        let result3: T = self.sub_clients[idx].execute(
          context,
          active_delay_ms,
        );
        return result3;
      }
    }
  }
}

// ---------------------------------------------------------------------------
// RuntimeIo
// ---------------------------------------------------------------------------
// Every `$rust_io_function` in this file (and in the other ns_* .baml files)
// is used by `baml_builtins2_codegen` to generate the `RuntimeIo` trait
// (in `sys_types::runtime_io`). RuntimeIo is a flat, typed async interface
// to all sys-ops -- no VM plumbing (BexHeap, SysOpContext, CallId) in its
// signatures. Crates like `sys_llm` take `&dyn RuntimeIo` to call into the
// runtime IO layer (HTTP, env, filesystem, shell) without coupling to the VM.
//
// The generated `RuntimeIoAdapter` (in `sys_ops`) bridges the trait to the
// underlying `SysOpFn` pointers by marshaling typed args through
// `BexExternalValue`.
//
// The trait carries `UnwindSafe + RefUnwindSafe` bounds because the AWS SDK's
// `HttpConnector` trait requires them on provider objects that store
// `Arc<dyn RuntimeIo>`. The adapter has a manual `impl UnwindSafe` -- this is
// safe because it holds only `Arc` clones (no interior mutability of its own)
// and we never catch panics across the SysOpFn boundary.
// ---------------------------------------------------------------------------

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

// @providers-no-options: openai, openai-generic, openai-responses, ollama, openrouter, google-ai, fallback, round-robin

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
class AzureOpenAiOptions {
  resource_name string | null
  deployment_id string | null
  api_version string
  max_tokens int | null
}

// @providers: anthropic
class AnthropicOptions {
  max_tokens int | null
}

// @providers: vertex-ai
class VertexAiOptions {
  credentials string | null
  credentials_content string | null
  location string | null
  project_id string | null
}

// @providers: aws-bedrock
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
  ) -> root.http.Request throws root.errors.LlmClient {
    $rust_io_function
  }

  function parse<T>(
    self,
    http_response_body: string,
    type_def: type,
  ) -> T throws root.errors.LlmClient {
    $rust_io_function
  }
}

function get_jinja_template(function_name: string) -> string throws root.errors.InvalidArgument {
  $rust_io_function
}

function get_return_type(function_name: string) -> type throws root.errors.InvalidArgument {
  $rust_io_function
}

function __sap_parse<T>(json: string, ty: type) -> T throws root.errors.LlmClient {
    $rust_io_function
}
