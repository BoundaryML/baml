class OrchestrationStep {
  primitive_client PrimitiveClient
  delay_ms int
}

class ExecutionContext {
  jinja_string string
  args map<string, unknown>
  function_name string
  /// BEP-049 §10 (M5e/M5f). For new-mode (backtick) LLM functions, the compiled
  /// `prompt`…`` closure the orchestrator invokes per attempt with a built
  /// `Context`, replacing the Jinja render. `null` for legacy `#"..."#` functions.
  prompt_closure ((Context) -> PromptAst)?
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

/// Renders the Jinja prompt template for `function_name` with the given `args` using `client`'s configuration.
/// Returns the specialized `PromptAst` ready to be sent to the LLM provider.
function render_prompt(client: Client, function_name: string, args: map<string, unknown>) -> PromptAst {
  let jinja_string = get_jinja_template(function_name);
  let return_type = get_return_type(function_name);
  let primitive_client = client.to_primitive_client();
  let prompt = primitive_client.render_prompt(jinja_string, args, return_type);

  primitive_client.specialize_prompt(prompt)
}

function build_request(client: Client, function_name: string, args: map<string, unknown>) -> root.http.Request {
  let return_type = get_return_type(function_name);
  let primitive_client = client.to_primitive_client();
  let specialized_prompt = render_prompt(client, function_name, args);

  primitive_client.build_request(specialized_prompt, return_type)
}

function build_request_stream(client: Client, function_name: string, args: map<string, unknown>) -> root.http.Request {
  let return_type = get_return_type(function_name);
  let primitive_client = client.to_primitive_client();
  let specialized_prompt = render_prompt(client, function_name, args);

  primitive_client.build_request_stream(specialized_prompt, return_type)
}

/// UNSAFE: do not call from user code
///
/// Used by LLM function companion functions.
/// Only parses full responses, not partials.
function parse<TStream, TFinal>(function_name: string, json: string) -> TFinal {
  let return_type = get_return_type(function_name);
  let stream_return_type = get_stream_return_type(function_name);
  let cache = StreamCache.new(return_type, stream_return_type);
  __sap_parse_final<TStream, TFinal>(json, cache)
}

/// UNSAFE: do not call from user code
///
/// Call an LLM function and return its full result.
function call_llm_function<T>(client: Client, function_name: string, args: map<string, unknown>, prompt_closure: ((Context) -> PromptAst)? = null) -> T {
  // New-mode (backtick) functions have no Jinja template registered; the
  // closure renders the prompt, so only look one up for the legacy path.
  let jinja_string = match (prompt_closure) {
    null => get_jinja_template(function_name),
    _ => "",
  };

  let context = ExecutionContext {
    jinja_string: jinja_string,
    args: args,
    function_name: function_name,
    prompt_closure: prompt_closure,
  };

  let result: T = client.execute_oneshot<T>(context, 0);
  result
}

/// UNSAFE: do not call from user code
///
/// The streaming counterpart of `call_llm_function`.
///
/// Sends the request and returns a `Stream<TStream, TFinal>` which can be used to consume the data as it arrives.
function stream_llm_function<TStream, TFinal>(client: Client, function_name: string, args: map<string, unknown>, prompt_closure: ((Context) -> PromptAst)? = null) -> Stream<TStream, TFinal> {
    // New-mode (backtick) functions have no Jinja template registered; the
    // closure renders the prompt, so only look one up for the legacy path.
    let jinja_string = match (prompt_closure) {
        null => get_jinja_template(function_name),
        _ => "",
    };

    let context = ExecutionContext {
        jinja_string: jinja_string,
        args: args,
        function_name: function_name,
        prompt_closure: prompt_closure,
    };

    let result: Stream<TStream, TFinal> = client.execute_stream<TStream, TFinal>(context, 0);
    result
}
