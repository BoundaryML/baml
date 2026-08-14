// Prompt and FunctionSpec. A spec is a recipe: identity, bound arguments,
// the prompt template, the initial toolbox, and the resolved default client.
// No journal, no conversation, no wire state.
// Mirrors _planv2/pages/02_guides/02_specs_and_runners/01_specs.md.

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
    /// `baml.prompt.Role.metadata` of the `${role(...)}` marker that opened
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
    body: (
        role: (string) -> baml.prompt.Role throws never,
        ctx: baml.prompt.Context,
    ) -> baml.TaggedString throws never,
) -> (baml.prompt.Context) -> Prompt throws never {
    let render = (ctx: baml.prompt.Context) -> {
        let tagged = body(baml.prompt.make_role, ctx);
        root.internal.assemble_prompt(tagged.parts, tagged.values)
    };
    render
}

// One unit of model work, bound and unrun. Immutable; getters only.
// `MyFunc@spec(args)` desugars to a constructor like this: the compiler
// synthesizes a `MyFunc$spec` companion for every spec-mode LLM function
// (backtick prompt, "provider/model" client string, and a `tools` field —
// `tools []` opts a tool-less function in).
class FunctionSpec<Out> {
    spec_name: string,
    args: map<string, unknown>,
    prompt_template: (string) -> Prompt throws never,
    toolbox: root.tools.Toolbox,
    default_client: Client,

    function name(self) -> string {
        self.spec_name
    }

    function arguments(self) -> map<string, unknown> {
        self.args
    }

    function output_type(self) -> type {
        reflect.type_of<Out>()
    }

    function prompt(self, output_format: string = "") -> Prompt {
        self.prompt_template(output_format)
    }

    function tools(self) -> root.tools.Toolbox {
        self.toolbox
    }

    // NOTE: `client` is a keyword, so the default client is read as the
    // `default_client` field rather than a `client()` getter. Holding an
    // eager value is safe because client construction is pure — credentials
    // resolve from the environment at request time, never in `new()`.
}
