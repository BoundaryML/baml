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
  prompt_closure ((Context) -> PromptAst throws never)?
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
/// The Jinja template to render for `function_name`. New-mode (BEP-049 M5)
/// functions have no template registered — their compiled `prompt`…`` closure
/// renders the prompt — so only look one up for the legacy `#"..."#` path.
function prompt_template_for(function_name: string, prompt_closure: ((Context) -> PromptAst throws never)?) -> string throws root.errors.InvalidArgument {
  match (prompt_closure) {
    null => get_jinja_template(function_name),
    _ => "",
  }
}

function render_prompt(client: Client, function_name: string, args: map<string, unknown>, prompt_closure: ((Context) -> PromptAst throws never)? = null) -> PromptAst {
  let return_type = get_return_type(function_name);
  // Rendering is offline: it needs the client's provider/role metadata but no
  // credentials. Construct leniently so a missing `api_key` env var doesn't
  // panic — `<Fn>$render_prompt` must work without a key set. (`build_request`
  // below stays strict, so the network paths still require the key.)
  let primitive_client = client.to_primitive_client(lenient = true);
  let template = prompt_template_for(function_name, prompt_closure);
  // `render_specialized_prompt` is the same render the orchestrator uses, so the
  // playground preview/cURL render byte-identically to execution.
  primitive_client.render_specialized_prompt(template, args, return_type, prompt_closure)
}

function build_request(client: Client, function_name: string, args: map<string, unknown>, prompt_closure: ((Context) -> PromptAst throws never)? = null) -> root.http.Request {
  let return_type = get_return_type(function_name);
  let primitive_client = client.to_primitive_client();
  let template = prompt_template_for(function_name, prompt_closure);
  let specialized_prompt = primitive_client.render_specialized_prompt(template, args, return_type, prompt_closure);

  primitive_client.build_request(specialized_prompt, return_type)
}

function build_request_stream(client: Client, function_name: string, args: map<string, unknown>, prompt_closure: ((Context) -> PromptAst throws never)? = null) -> root.http.Request {
  let return_type = get_return_type(function_name);
  let primitive_client = client.to_primitive_client();
  let template = prompt_template_for(function_name, prompt_closure);
  let specialized_prompt = primitive_client.render_specialized_prompt(template, args, return_type, prompt_closure);

  primitive_client.build_request_stream(specialized_prompt, return_type)
}

/// UNSAFE: do not call from user code
///
/// Used by LLM function `$parse` companions, which pass
/// `<STREAM_EXPANDED, ORIGINAL>` as explicit type args.
/// Only parses full responses, not partials.
///
/// `$parse` is a purely local, network-free parse of an existing JSON string
/// into the function's return type, so a schema mismatch (e.g. an out-of-enum
/// value) is a *parse* failure — not an LLM/client failure. The underlying
/// schema-aligned parser reports failures as `LlmClient` because it is shared
/// with the network and streaming paths (whose `LlmClient` typing must stay
/// intact); remap it to the parse-flavored `ParseError` here so the surfaced
/// error name matches the operation and the documented
/// `catch (baml.errors.ParseError)` pattern is reachable (no E0063).
function parse<TStream, TFinal>(json: string) -> TFinal throws root.errors.ParseError {
  let cache: StreamCache<TStream, TFinal> = StreamCache.new(reflect.type_of<TStream>(), reflect.type_of<TFinal>());
  __sap_parse_final<TStream, TFinal>(json, cache) catch (_) {
    root.errors.LlmClient { message } => throw root.errors.ParseError { message: message },
  }
}

/// Internal bridge for callers outside `baml.llm` that need a typed stream
/// cache. Keeping the inference here avoids requiring an explicit static-method
/// owner at each call site.
function __new_stream_cache<TStream, TFinal>() -> StreamCache<TStream, TFinal> throws never {
  let cache: StreamCache<TStream, TFinal> = StreamCache.new(reflect.type_of<TStream>(), reflect.type_of<TFinal>());
  cache
}

/// UNSAFE: do not call from user code
///
/// Call an LLM function and return its full result.
function call_llm_function<T>(client: Client, function_name: string, args: map<string, unknown>, prompt_closure: ((Context) -> PromptAst throws never)? = null) -> T {
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

