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
/// The Jinja template to render for `function_name`. New-mode (BEP-049 M5)
/// functions have no template registered — their compiled `prompt`…`` closure
/// renders the prompt — so only look one up for the legacy `#"..."#` path.
function prompt_template_for(function_name: string, prompt_closure: ((Context) -> PromptAst)?) -> string throws root.errors.InvalidArgument {
  match (prompt_closure) {
    null => get_jinja_template(function_name),
    _ => "",
  }
}

/// Unwrap the `LegacyClient` bridge back to the legacy `Client` config. The
/// injected LLM-function `client` param is the new-model existential
/// `root.ai.Provider` (DCP §1.1); the declared client arrives as a
/// `LegacyClient`-wrapped default. Paths that genuinely need the legacy
/// config (the Jinja/companion pipeline) throw typed `Unsupported` for a
/// native override — those routes go native in the Phase C desugar.
function legacy_client_of(client: root.ai.Provider) -> Client throws root.errors.Unsupported {
  // The leading `let` keeps the parser from sniffing `match (client)` as an
  // LLM declarative body (`client` is a keyword token).
  let provider = client;
  match (provider) {
    let lc: Client => lc,
    _ => throw root.errors.Unsupported {
      message: "this path needs a legacy client config; a native provider override is not supported here",
    },
  }
}

function render_prompt(client: root.ai.Provider, function_name: string, args: map<string, unknown>, prompt_closure: ((Context) -> PromptAst)? = null) -> PromptAst {
  let return_type = get_return_type(function_name);
  // Legacy clients render with their own primitive config; a NATIVE provider
  // has no legacy config, so render with a neutral primitive (roles/media
  // preserved; wire shaping is the native provider's job). This is the
  // Phase C prompt hook — provider-specific specialization comes later.
  let provider = client;
  let primitive_client = match (provider) {
    let lc: Client => lc.to_primitive_client(),
    _ => from_shorthand("openai/gpt-4o"),
  };
  let template = prompt_template_for(function_name, prompt_closure);
  // `render_specialized_prompt` is the same render the orchestrator uses, so the
  // playground preview/cURL render byte-identically to execution.
  primitive_client.render_specialized_prompt(template, args, return_type, prompt_closure)
}

function build_request(client: root.ai.Provider, function_name: string, args: map<string, unknown>, prompt_closure: ((Context) -> PromptAst)? = null) -> root.http.Request {
  let return_type = get_return_type(function_name);
  let primitive_client = legacy_client_of(client).to_primitive_client();
  let template = prompt_template_for(function_name, prompt_closure);
  let specialized_prompt = primitive_client.render_specialized_prompt(template, args, return_type, prompt_closure);

  primitive_client.build_request(specialized_prompt, return_type)
}

function build_request_stream(client: root.ai.Provider, function_name: string, args: map<string, unknown>, prompt_closure: ((Context) -> PromptAst)? = null) -> root.http.Request {
  let return_type = get_return_type(function_name);
  let primitive_client = legacy_client_of(client).to_primitive_client();
  let template = prompt_template_for(function_name, prompt_closure);
  let specialized_prompt = primitive_client.render_specialized_prompt(template, args, return_type, prompt_closure);

  primitive_client.build_request_stream(specialized_prompt, return_type)
}

/// UNSAFE: do not call from user code
///
/// Used by LLM function `$parse` companions, which pass
/// `<STREAM_EXPANDED, ORIGINAL>` as explicit type args.
/// Only parses full responses, not partials.
function parse<TStream, TFinal>(json: string) -> TFinal {
  let cache: StreamCache<TStream, TFinal> = StreamCache.new(reflect.type_of<TStream>(), reflect.type_of<TFinal>());
  __sap_parse_final<TStream, TFinal>(json, cache)
}

/// UNSAFE: do not call from user code
///
/// Call an LLM function and return its full result.
function call_llm_function<T>(client: root.ai.Provider, function_name: string, args: map<string, unknown>, prompt_closure: ((Context) -> PromptAst)? = null) -> T {
  // New-mode (backtick) functions have no Jinja template registered; the
  // closure renders the prompt, so only look one up for the legacy path.
  let jinja_string = match (prompt_closure) {
    null => get_jinja_template(function_name),
    _ => "",
  };

  match (client) {
    // Declared/legacy client (or a legacy call-site override): today's
    // orchestrator path, unchanged. This concrete arm precedes capability
    // negotiation, so legacy strategy configs keep their strategy loops.
    let lc: Client => {
      let context = ExecutionContext {
        jinja_string: jinja_string,
        args: args,
        function_name: function_name,
        prompt_closure: prompt_closure,
      };
      let result: T = lc.execute_oneshot<T>(context, 0);
      result
    },
    // Native override (`Foo(x, client = Anthropic { … })`): render with a
    // neutral legacy renderer (roles/media preserved), then hand native
    // messages to the provider. The renderer's provider-specific
    // specialization is generic role/wrapping only; wire shaping is the
    // native provider's job.
    let h: root.ai.HttpProvider => {
      let renderer = from_shorthand("openai/gpt-4o");
      let return_type = get_return_type(function_name);
      let specialized = renderer.render_specialized_prompt(jinja_string, args, return_type, prompt_closure);
      let messages = root.ai.prompt_to_messages(specialized);
      h.call_messages<T>(messages)
    },
    _ => throw root.errors.Unsupported {
      message: "client override supports neither the legacy bridge nor HttpProvider",
    },
  }
}

/// UNSAFE: do not call from user code
///
/// The streaming counterpart of `call_llm_function`.
///
/// Sends the request and returns a `Stream<TStream, TFinal>` which can be used to consume the data as it arrives.
function stream_llm_function<TStream, TFinal>(client: root.ai.Provider, function_name: string, args: map<string, unknown>, prompt_closure: ((Context) -> PromptAst)? = null) -> Stream<TStream, TFinal> {
    // New-mode (backtick) functions have no Jinja template registered; the
    // closure renders the prompt, so only look one up for the legacy path.
    let jinja_string = match (prompt_closure) {
        null => get_jinja_template(function_name),
        _ => "",
    };

    match (client) {
        // Declared/legacy client (or a legacy override): today's stream
        // orchestrator, unchanged.
        let lc: Client => {
            let context = ExecutionContext {
                jinja_string: jinja_string,
                args: args,
                function_name: function_name,
                prompt_closure: prompt_closure,
            };
            let result: Stream<TStream, TFinal> = lc.execute_stream<TStream, TFinal>(context, 0);
            result
        },
        // Native override: render neutrally, stream through the provider's
        // Streaming capability.
        let s: root.ai.Streaming => {
            let renderer = from_shorthand("openai/gpt-4o");
            let return_type = get_return_type(function_name);
            let specialized = renderer.render_specialized_prompt(jinja_string, args, return_type, prompt_closure);
            let messages = root.ai.prompt_to_messages(specialized);
            s.stream_messages<TStream, TFinal>(messages)
        },
        _ => throw root.errors.Unsupported {
            message: "client override supports neither the legacy bridge nor Streaming",
        },
    }
}

/// UNSAFE: do not call from user code
///
/// The `$parse_stream` companion seam: build a legacy `Stream` from an open
/// SSE handle. Unwraps the Provider-typed client param back to the legacy
/// `Client` (`Unsupported` for native overrides — `$parse_stream` is a
/// legacy-wire companion by definition).
function make_stream_for<TStream, TFinal>(client: root.ai.Provider, sse: root.http.SseStream) -> Stream<TStream, TFinal> {
  legacy_client_of(client).__make_stream<TStream, TFinal>(sse)
}
