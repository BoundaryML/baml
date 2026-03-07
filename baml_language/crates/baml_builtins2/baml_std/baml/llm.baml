class OrchestrationStep {
  primitive_client baml.llm.PrimitiveClient
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
    let base = baml.llm.round_robin_peek(client_name)
    (base + local_offset) % sub_client_count
  }
}

function build_attempt(llm_client: baml.llm.Client) -> baml.llm.OrchestrationStep[] {
  let planner_state = baml.llm.PlannerState { rr_client_names: [] }
  baml.llm.build_attempt_with_state(llm_client, planner_state)
}

function build_attempt_with_state(
  llm_client: baml.llm.Client,
  planner_state: baml.llm.PlannerState,
) -> baml.llm.OrchestrationStep[] {
  match (llm_client.client_type) {
    baml.llm.ClientType.Primitive => {
      let resolve_fn = baml.llm.resolve_client(llm_client.name)
      let primitive = resolve_fn()
      [baml.llm.OrchestrationStep { primitive_client: primitive, delay_ms: 0 }]
    }

    baml.llm.ClientType.Fallback => {
      let steps: baml.llm.OrchestrationStep[] = []
      for (let sub in llm_client.sub_clients) {
        for (let step in baml.llm.build_plan_with_state(sub, planner_state)) {
          steps.push(step)
        }
      }
      steps
    }

    baml.llm.ClientType.RoundRobin => {
      if (llm_client.sub_clients.length() == 0) {
        []
      } else {
        let idx = planner_state.rr_pick_index(
          llm_client.name,
          llm_client.sub_clients.length(),
        )
        baml.llm.build_plan_with_state(
          llm_client.sub_clients.at(idx),
          planner_state,
        )
      }
    }
  }
}

function build_plan(llm_client: baml.llm.Client) -> baml.llm.OrchestrationStep[] {
  let planner_state = baml.llm.PlannerState { rr_client_names: [] }
  baml.llm.build_plan_with_state(llm_client, planner_state)
}

function build_plan_with_state(
  llm_client: baml.llm.Client,
  planner_state: baml.llm.PlannerState,
) -> baml.llm.OrchestrationStep[] {
  match (llm_client.retry) {
    null => baml.llm.build_attempt_with_state(llm_client, planner_state),
    r: baml.llm.RetryPolicy => {
      let result: baml.llm.OrchestrationStep[] = []
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

        let inner = baml.llm.build_attempt_with_state(llm_client, planner_state)
        for (let step in inner) {
          result.push(baml.llm.OrchestrationStep {
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
  llm_client: baml.llm.Client,
  context: baml.llm.ExecutionContext,
  inherited_delay_ms: int,
) -> baml.llm.ExecutionResult {
  match (llm_client.retry) {
    null => baml.llm.execute_client_once(
      llm_client,
      context,
      inherited_delay_ms,
    ),
    r: baml.llm.RetryPolicy => {
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

        let result = baml.llm.execute_client_once(
          llm_client,
          context,
          attempt_delay,
        )

        if (result.ok) {
          return result
        }
      }

      baml.llm.ExecutionResult { ok: false, value: null }
    }
  }
}

function execute_client_once(
  llm_client: baml.llm.Client,
  context: baml.llm.ExecutionContext,
  active_delay_ms: int,
) -> baml.llm.ExecutionResult {
  match (llm_client.client_type) {
    baml.llm.ClientType.Primitive => {
      let resolve_fn = baml.llm.resolve_client(llm_client.name)
      let primitive = resolve_fn()

      let prompt = primitive.render_prompt(context.jinja_string, context.args)
      let specialized = primitive.specialize_prompt(prompt)
      let http_request = primitive.build_request(specialized)
      let http_response = baml.http.send(http_request)

      if (http_response.ok()) {
        let body = http_response.text()
        let return_type = baml.llm.get_return_type(context.function_name)
        return baml.llm.ExecutionResult {
          ok: true,
          value: primitive.parse(body, return_type),
        }
      }

      if (active_delay_ms > 0) {
        baml.sys.sleep(active_delay_ms)
      }

      baml.llm.ExecutionResult { ok: false, value: null }
    }

    baml.llm.ClientType.Fallback => {
      for (let sub in llm_client.sub_clients) {
        let result = baml.llm.execute_client(
          sub,
          context,
          active_delay_ms,
        )
        if (result.ok) {
          return result
        }
      }
      baml.llm.ExecutionResult { ok: false, value: null }
    }

    baml.llm.ClientType.RoundRobin => {
      let idx = baml.llm.round_robin_next(llm_client.name) % llm_client.sub_clients.length()
      baml.llm.execute_client(
        llm_client.sub_clients.at(idx),
        context,
        active_delay_ms,
      )
    }
  }
}

function resolve_primitive(function_name: string) -> baml.llm.PrimitiveClient {
  let llm_client = baml.llm.get_client(function_name)
  let resolve_fn = baml.llm.resolve_client(llm_client.name)
  resolve_fn()
}

function render_prompt(function_name: string, args: map<string, unknown>) -> baml.llm.PromptAst {
  let jinja_string = baml.llm.get_jinja_template(function_name)
  let primitive_client = baml.llm.resolve_primitive(function_name)
  let prompt = primitive_client.render_prompt(jinja_string, args)

  return primitive_client.specialize_prompt(prompt)
}

function build_request(function_name: string, args: map<string, unknown>) -> baml.http.Request {
  let primitive_client = baml.llm.resolve_primitive(function_name)
  let specialized_prompt = baml.llm.render_prompt(function_name, args)

  return primitive_client.build_request(specialized_prompt)
}

function call_llm_function(function_name: string, args: map<string, unknown>) -> unknown {
  let jinja_string = baml.llm.get_jinja_template(function_name)
  let llm_client = baml.llm.get_client(function_name)

  let context = baml.llm.ExecutionContext {
    jinja_string: jinja_string,
    args: args,
    function_name: function_name,
  }

  let result = baml.llm.execute_client(llm_client, context, 0)

  if (result.ok) {
    return result.value
  }

  throw baml.errors.DevOther { message: "All orchestration steps failed" }
}
