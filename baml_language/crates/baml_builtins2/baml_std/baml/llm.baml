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
    let offset = 0;
    for (let seen_name in self.rr_client_names) {
      if (seen_name == client_name) {
        offset += 1;
      }
    }
    offset
  }

  function rr_pick_index(self, client_name: string, counter: int, sub_client_count: int) -> int {
    let local_offset = self.rr_local_offset(client_name);
    self.rr_client_names.push(client_name);
    (counter + local_offset) % sub_client_count
  }
}

function render_prompt(client: Client, function_name: string, args: map<string, unknown>) -> PromptAst {
  let jinja_string = get_jinja_template(function_name);
  let primitive_client = client.to_primitive_client();
  let prompt = primitive_client.render_prompt(jinja_string, args);

  primitive_client.specialize_prompt(prompt)
}

function build_request(client: Client, function_name: string, args: map<string, unknown>) -> root.http.Request {
  let primitive_client = client.to_primitive_client();
  let specialized_prompt = render_prompt(client, function_name, args);

  primitive_client.build_request(specialized_prompt)
}

function call_llm_function<T>(client: Client, function_name: string, args: map<string, unknown>) -> T {
  let jinja_string = get_jinja_template(function_name);

  let context = ExecutionContext {
    jinja_string: jinja_string,
    args: args,
    function_name: function_name,
  };

  let result: T = client.execute(context, 0);
  result
}
