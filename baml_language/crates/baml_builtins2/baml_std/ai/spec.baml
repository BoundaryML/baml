// Prompt and FunctionSpec. A spec is a recipe: identity, bound arguments,
// the prompt template, the initial toolbox, and the resolved default client.
// No journal, no conversation, no wire state.
// Mirrors _planv2/pages/02_guides/02_specs_and_runners/01_specs.md.

/// A client credential (or endpoint) option: either the literal value, or a
/// LATE-BOUND `baml.env.Ref` naming the environment variable that holds it.
///
/// Every provider client's `api_key` / `base_url` is `Credential?`, so all
/// three shapes are accepted positionally:
///
///     api_key = "sk-..."                  // literal
///     api_key = env.OPENAI_API_KEY        // late-bound ref, read per request
///     api_key = null                      // fall back to the canonical env var
///
/// A `Ref` carries the variable's NAME only, never its value: a constructed
/// client never holds the secret, and the read happens at request time — after
/// `$init` (which cannot do io) and after a host has loaded its secrets.
/// `ai.wire.resolve_credential` collapses the union to `string?`.
type Credential = string | baml.env.Ref;

/// What a `client:` position accepts. Defined in `ai/ns_clients/clients.baml`
/// next to `ai.clients.resolve`, which consumes it; re-exported here so the
/// package-root name `ai.ClientSelector` reads alongside `ai.Client`.
type ClientSelector = root.clients.ClientSelector;

/// Provider-neutral media retained inside a rendered prompt.
type MediaPart = baml.media.Image | baml.media.Audio | baml.media.Video | baml.media.Pdf;

/// One structural part of a rendered prompt message.
type PromptPart = string | MediaPart;

/// A single chat message in a rendered prompt.
class PromptMessage {
    /// The chat role, e.g. "system", "user", or "assistant". Empty for
    /// role-less content.
    role: string,
    /// The message's rendered text. Media parts use a readable placeholder.
    content: string,
    /// Ordered structural content. Provider clients use this instead of the
    /// readable `content` projection so media survives prompt rendering.
    parts: PromptPart[],
    /// Per-message provider directives, carried verbatim from the
    /// `root.Role.metadata` of the `${role(...)}` marker that opened
    /// this message — Anthropic `cache_control`, for example. Empty when the
    /// message carried none.
    metadata: map<string, baml.json.json>,
}

/// The rendered, provider-neutral prompt passed to clients. Roles split the
/// prompt into messages and media remains structural until a provider lowers
/// it to its wire format.
class Prompt {
    _data: $rust_type,

    /// Render the prompt as readable text. Message roles are emitted as headers.
    //baml:vm
    function text(self) -> string throws never {
        $rust_function
    }

    /// Return the ordered chat messages used by provider request builders.
    //baml:mut_vm
    function messages(self) -> PromptMessage[] throws never {
        $rust_function
    }

    implements baml.ToString {
        function to_string(self) -> string throws never {
            self.text()
        }
    }
}

/// Standalone tagged prompt constructor. The returned closure renders a
/// structural `ai.Prompt` for the supplied low-level prompt context.
//baml:tagged_string
function prompt(
    // No `throws` clause: the callback's effect is inferred per call site
    // (rule 5); whatever a template expression throws is wrapped below.
    body: (
        role: (string) -> root.Role throws never,
        ctx: root.Context,
    ) -> baml.TaggedString,
) -> (root.Context) -> Prompt throws root.errors.PromptRenderError {
    let render = (ctx: root.Context) -> {
        let tagged = body(root.internal.make_role, ctx) catch_all (e) {
            _ => throw root.errors.PromptRenderError {
                message: "prompt template rendering threw",
                cause: e,
            }
        };
        root.internal.assemble_prompt(tagged.parts, tagged.values)
    };
    render
}

// One unit of model work, bound and unrun. `MyFunc@spec(args)` projects the
// authored LLM function's attached private recipe; it does not name or create
// another declaration. Streaming is a separate `MyFunc@stream(args)`
// projection whose return type carries the existing `Out$stream` schema.
class FunctionSpec<Out> {
    spec_name: string,
    args: map<string, unknown>,
    prompt_template: (OutputFormat) -> Prompt throws root.errors.PromptRenderError,
    toolbox: root.tools.Toolbox,
    default_client: Client,

    // Execute this bound spec through the default Agent. Null uses the bound
    // client and an optional event callback observes the same journal events
    // as a direct LLM-function call.
    function call(
        self,
        client: Client? = null,
        on_event: ((root.events.Event) -> void)? = null,
    ) -> Out {
        Agent.new(client = client, on_event = on_event).run(self).value
    }

    // Parse an existing model reply using this spec's realized output type.
    function parse(self, json: string) -> Out {
        baml.sap.parse<Out>(json)
    }

    // Build the provider HTTP request shape for this spec without invoking the model.
    // A supplied client overrides the spec's default; null uses the default.
    function build_request(self, client: Client? = null) -> baml.http.Request {
        let selected = client ?? self.default_client;
        selected
            .render(
                ModelTurnInput {
                    prompt: self.prompt_template,
                    journal: Journal.new(self),
                    toolbox: self.tools(),
                    output_type: self.output_type(),
                },
            )
    }

    function name(self) -> string {
        self.spec_name
    }

    function arguments(self) -> map<string, unknown> {
        self.args
    }

    function output_type(self) -> reflect.Type throws reflect.errors.CompilationError {
        let output_type = reflect.Type.of<Out>();
        root.internal.validate_output_type(output_type);
        output_type
    }

    function prompt(self) -> Prompt {
        self.prompt_template(root.internal.build_output_format(self.output_type()))
    }

    function tools(self) -> root.tools.Toolbox {
        self.toolbox
    }

    function client_id(self) -> string {
        self.default_client.id()
    }

    // NOTE: `client` is a keyword, so the default client is read as the
    // `default_client` field rather than a `client()` getter. Holding an
    // eager value is safe because client construction is pure — credentials
    // resolve from the environment at request time, never in `new()`.
}
