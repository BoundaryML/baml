// Prompt and FunctionSpec. A spec is a recipe: identity, bound arguments,
// the prompt template, the initial toolbox, and the resolved default client.
// No journal, no conversation, no wire state.
// Mirrors _planv2/pages/02_guides/02_specs_and_runners/01_specs.md.

// The function's prompt with arguments bound but nothing rendered. The
// template closure receives the output-format text to substitute at the
// ${ctx.output_format} position (empty when the contract goes on the wire).
class Prompt {
    template: (string) -> string throws never,

    function render_text(self, output_format: string) -> string {
        self.template(output_format)
    }
}

// One unit of model work, bound and unrun. Immutable; getters only.
// `MyFunc@spec(args)` desugars to a constructor like this: the compiler
// synthesizes a `MyFunc$spec` companion for every spec-mode LLM function
// (backtick prompt, "provider/model" client string, and a `tools` field —
// `tools []` opts a tool-less function in).
class FunctionSpec<Out> {
    spec_name: string,
    args: map<string, unknown>,
    prompt_template: Prompt,
    toolbox: Toolbox,
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

    function prompt(self) -> Prompt {
        self.prompt_template
    }

    function tools(self) -> Toolbox {
        self.toolbox
    }

    // NOTE: `client` is a keyword, so the default client is read as the
    // `default_client` field rather than a `client()` getter. Holding an
    // eager value is safe because client construction is pure — credentials
    // resolve from the environment at request time, never in `new()`.
}
