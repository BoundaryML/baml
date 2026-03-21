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
/// Built by get_client from compiler metadata.
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
      null => self.build_attempt_with_state(planner_state),
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
    }
  }

  function execute(
    self,
    context: ExecutionContext,
    inherited_delay_ms: int,
  ) -> ExecutionResult {
    match (self.retry) {
      null => self.execute_once(
        context,
        inherited_delay_ms,
      ),
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

          let result = self.execute_once(
            context,
            attempt_delay,
          )

          if (result.ok) {
            return result
          }
        }

        ExecutionResult { ok: false, value: null }
      }
    }
  }

  function execute_once(
    self,
    context: ExecutionContext,
    active_delay_ms: int,
  ) -> ExecutionResult {
    match (self.client_type) {
      ClientType.Primitive => {
        let resolve_fn = self.get_constructor()
        let primitive = resolve_fn()

        let prompt = primitive.render_prompt(context.jinja_string, context.args)
        let specialized = primitive.specialize_prompt(prompt)
        let http_request = primitive.build_request(specialized)
        let http_response = root.http.send(http_request)

        if (http_response.ok()) {
          let body = http_response.text()
          let return_type = get_return_type(context.function_name)
          return ExecutionResult {
            ok: true,
            value: primitive.parse(body, return_type),
          }
        }

        if (active_delay_ms > 0) {
          root.sys.sleep(active_delay_ms)
        }

        ExecutionResult { ok: false, value: null }
      }

      ClientType.Fallback => {
        for (let sub in self.sub_clients) {
          let result = sub.execute(
            context,
            active_delay_ms,
          )
          if (result.ok) {
            return result
          }
        }
        ExecutionResult { ok: false, value: null }
      }

      ClientType.RoundRobin => {
        let idx = round_robin_next(self.name) % self.sub_clients.length()
        self.sub_clients[idx].execute(
          context,
          active_delay_ms,
        )
      }
    }
  }
}

class PrimitiveClientOptions {
  base_url string | null
  default_role string | null
  allowed_roles string[] | null
  remap_roles map<string, string> | null
  api_key string | null
  provider_options AnthropicOptions | AzureOpenAiOptions | null

  // request stuff here
  headers map<string, string>
  query_params map<string, string>
  request_body map<string, unknown>
}

class AzureOpenAiOptions {
  resource_name string
  api_version string
}

class AnthropicOptions {
  anthropic_version string | null
}

class PrimitiveClient {
  name string
  provider string
  options PrimitiveClientOptions

  function render_prompt(
    self,
    template: string,
    args: map<string, unknown>,
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

  function parse(
    self,
    http_response_body: string,
    type_def: type,
  ) -> unknown throws root.errors.LlmClient {
    $rust_io_function
  }
}

function get_jinja_template(function_name: string) -> string throws root.errors.InvalidArgument {
  $rust_io_function
}

function get_client(function_name: string) -> Client throws root.errors.InvalidArgument {
  $rust_io_function
}

function round_robin_next(client_name: string) -> int throws root.errors.InvalidArgument {
  $rust_io_function
}

function round_robin_peek(client_name: string) -> int throws root.errors.InvalidArgument {
  $rust_io_function
}

function get_return_type(function_name: string) -> type throws root.errors.InvalidArgument {
  $rust_io_function
}
