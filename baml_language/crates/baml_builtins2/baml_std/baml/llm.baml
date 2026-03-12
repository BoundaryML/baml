class OrchestrationStep {
  primitive_client PrimitiveClient
  delay_ms int
}

class ExecutionResult {
  ok bool
  value unknown
}

class ExecutionContext {
  jinja_string string
  args map<string, unknown>
  function_name string
}

class PlannerState {
  rr_client_names string[]

  function rr_local_offset(self, client_name: string) -> int {
    let offset = 0
    for (let seen_name in self.rr_client_names) {
      if (seen_name == client_name) {
        offset += 1
      }
    }
    offset
  }

  function rr_pick_index(self, client_name: string, sub_client_count: int) -> int {
    let local_offset = self.rr_local_offset(client_name)
    self.rr_client_names.push(client_name)
    let base = round_robin_peek(client_name)
    (base + local_offset) % sub_client_count
  }
}

function build_attempt(llm_client: Client) -> OrchestrationStep[] {
  let planner_state = PlannerState { rr_client_names: [] }
  build_attempt_with_state(llm_client, planner_state)
}

function build_attempt_with_state(
  llm_client: Client,
  planner_state: PlannerState,
) -> OrchestrationStep[] {
  match (llm_client.client_type) {
    ClientType.Primitive => {
      let resolve_fn = resolve_client(llm_client.name)
      let primitive = resolve_fn()
      [OrchestrationStep { primitive_client: primitive, delay_ms: 0 }]
    }

    ClientType.Fallback => {
      let steps: OrchestrationStep[] = []
      for (let sub in llm_client.sub_clients) {
        for (let step in build_plan_with_state(sub, planner_state)) {
          steps.push(step)
        }
      }
      steps
    }

    ClientType.RoundRobin => {
      if (llm_client.sub_clients.length() == 0) {
        []
      } else {
        let idx = planner_state.rr_pick_index(
          llm_client.name,
          llm_client.sub_clients.length(),
        )
        build_plan_with_state(
          llm_client.sub_clients.at(idx),
          planner_state,
        )
      }
    }
  }
}

function build_plan(llm_client: Client) -> OrchestrationStep[] {
  let planner_state = PlannerState { rr_client_names: [] }
  build_plan_with_state(llm_client, planner_state)
}

function build_plan_with_state(
  llm_client: Client,
  planner_state: PlannerState,
) -> OrchestrationStep[] {
  match (llm_client.retry) {
    null => build_attempt_with_state(llm_client, planner_state),
    r: RetryPolicy => {
      let result: OrchestrationStep[] = []
      let current_delay = r.initial_delay_ms + 0.0

      for (let attempt = 0; attempt <= r.max_retries; attempt += 1) {
        let delay = 0
        if (attempt > 0) {
          delay = baml.math.trunc(current_delay)
          let next = current_delay * r.multiplier
          if (next > r.max_delay_ms + 0.0) {
            current_delay = r.max_delay_ms + 0.0
          } else {
            current_delay = next
          }
        }

        let inner = build_attempt_with_state(llm_client, planner_state)
        for (let step in inner) {
          result.push(OrchestrationStep {
            primitive_client: step.primitive_client,
            delay_ms: delay,
          })
        }
      }

      result
    }
  }
}

function execute_client(
  llm_client: Client,
  context: ExecutionContext,
  inherited_delay_ms: int,
) -> ExecutionResult {
  match (llm_client.retry) {
    null => execute_client_once(
      llm_client,
      context,
      inherited_delay_ms,
    ),
    r: RetryPolicy => {
      let current_delay = r.initial_delay_ms + 0.0

      for (let attempt = 0; attempt <= r.max_retries; attempt += 1) {
        let attempt_delay = inherited_delay_ms
        if (attempt > 0) {
          attempt_delay = baml.math.trunc(current_delay)
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

        let result = execute_client_once(
          llm_client,
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

function execute_client_once(
  llm_client: Client,
  context: ExecutionContext,
  active_delay_ms: int,
) -> ExecutionResult {
  match (llm_client.client_type) {
    ClientType.Primitive => {
      let resolve_fn = resolve_client(llm_client.name)
      let primitive = resolve_fn()

      let prompt = primitive.render_prompt(context.jinja_string, context.args)
      let specialized = primitive.specialize_prompt(prompt)
      let http_request = primitive.build_request(specialized)
      let http_response = baml.http.send(http_request)

      if (http_response.ok()) {
        let body = http_response.text()
        let return_type = get_return_type(context.function_name)
        return ExecutionResult {
          ok: true,
          value: primitive.parse(body, return_type),
        }
      }

      if (active_delay_ms > 0) {
        baml.sys.sleep(active_delay_ms)
      }

      ExecutionResult { ok: false, value: null }
    }

    ClientType.Fallback => {
      for (let sub in llm_client.sub_clients) {
        let result = execute_client(
          sub,
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
      let idx = round_robin_next(llm_client.name) % llm_client.sub_clients.length()
      execute_client(
        llm_client.sub_clients.at(idx),
        context,
        active_delay_ms,
      )
    }
  }
}

function resolve_primitive(function_name: string) -> PrimitiveClient {
  let llm_client = get_client(function_name)
  let resolve_fn = resolve_client(llm_client.name)
  resolve_fn()
}

function render_prompt(function_name: string, args: map<string, unknown>) -> PromptAst {
  let jinja_string = get_jinja_template(function_name)
  let primitive_client = resolve_primitive(function_name)
  let prompt = primitive_client.render_prompt(jinja_string, args)

  return primitive_client.specialize_prompt(prompt)
}

function build_request(function_name: string, args: map<string, unknown>) -> baml.http.Request {
  let primitive_client = resolve_primitive(function_name)
  let specialized_prompt = render_prompt(function_name, args)

  return primitive_client.build_request(specialized_prompt)
}

function call_llm_function(function_name: string, args: map<string, unknown>) -> unknown {
  let jinja_string = get_jinja_template(function_name)
  let llm_client = get_client(function_name)

  let context = ExecutionContext {
    jinja_string: jinja_string,
    args: args,
    function_name: function_name,
  }

  let result = execute_client(llm_client, context, 0)

  if (result.ok) {
    return result.value
  }

  throw baml.errors.DevOther { message: "All orchestration steps failed" }
}
