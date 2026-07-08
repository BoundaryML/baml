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
  let template = prompt_template_for(function_name, prompt_closure);
  // Legacy clients render + specialize with their own primitive config
  // (`render_specialized_prompt` is the same render the orchestrator used, so
  // the playground preview/cURL render byte-identically to execution). A
  // NATIVE provider has no legacy config: render with a neutral primitive but
  // UNSPECIALIZED — the neutral specialize pass would assign its default
  // "system" role to bare prompts, and providers that hoist system messages
  // (Anthropic) would then see ZERO messages (400). Role-less content becomes
  // a "user" message in `prompt_to_messages`; wire shaping is the native
  // provider's job.
  let provider = client;
  match (provider) {
    let lc: Client => {
      lc.to_primitive_client().render_specialized_prompt(template, args, return_type, prompt_closure)
    },
    _ => {
      let renderer = from_shorthand("openai/gpt-4o");
      match (prompt_closure) {
        null => renderer.render_prompt(template, args, return_type),
        let closure: (Context) -> PromptAst => closure(build_prompt_context(renderer, return_type)),
      }
    },
  }
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
///
/// DCP §1.3 (Phase C.3): a thin delegator kept for direct callers (tooling,
/// tests). Desugared LLM function bodies call `baml.ai.drive_call` over their
/// `$render_prompt` companion directly and no longer route through here. All
/// routing — legacy-primitive codec, ported-primitive native classes, native
/// overrides, combinators — lives in the capability match inside `drive_call`
/// plus the `Client` bridge implements blocks (`ns_ai/core/legacy.baml`).
function call_llm_function<T>(client: root.ai.Provider, function_name: string, args: map<string, unknown>, prompt_closure: ((Context) -> PromptAst)? = null) -> T {
  root.ai.drive_call<T>(client, render_prompt(client, function_name, args, prompt_closure = prompt_closure))
}

/// UNSAFE: do not call from user code
///
/// The streaming counterpart of `call_llm_function` — same thin-delegator
/// status (see there).
function stream_llm_function<TStream, TFinal>(client: root.ai.Provider, function_name: string, args: map<string, unknown>, prompt_closure: ((Context) -> PromptAst)? = null) -> Stream<TStream, TFinal> {
  root.ai.drive_stream<TStream, TFinal>(client, render_prompt(client, function_name, args, prompt_closure = prompt_closure))
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
