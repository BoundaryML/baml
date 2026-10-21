// The client boundary: one operation. A client renders the input into its
// wire format, makes one call, and returns canonical content. It never
// writes the journal, executes a tool, or parses the output type.
// Mirrors _planv2/pages/02_guides/03_clients/02_the_client_interface.md.

class ModelTurnInput {
    // The bound prompt template. The client supplies the output-format schema
    // handle and receives one structural prompt.
    prompt: (OutputFormat) -> Prompt throws root.errors.PromptRenderError,
    journal: Journal,
    toolbox: root.tools.Toolbox,
    output_type: reflect.Type,
}

/// A provider-decoded output. The wrapper distinguishes a decoded null from
/// absence. Runners check its runtime type before returning it to callers.
class ParsedOutput {
    value: unknown,
}

/// One attempt at a turn. A client returns one that succeeded; a wrapper's
/// `child_turns` also hold the attempts that failed (`error` set, no content,
/// `stop_reason` Failed).
class ModelTurn {
    parsed_output: ParsedOutput?,
    content: root.content.ModelBlock[],
    stop_reason: root.content.StopReason,
    // The model that answered, as its response named it; null when unknown.
    resolved_model: root.events.ModelKey?,
    // The answering client's `requested_model()`; null when unknown.
    requested_model: string?,
    // The calls this client made itself, oldest first. A wrapper makes none.
    calls: root.events.ModelCall[],
    // The attempts of the clients this one wraps, oldest first.
    child_turns: ModelTurn[],
    error: (root.errors.Failure | baml.errors.Timeout)?,

    /// Every call behind this turn: its own, then each child's, oldest first.
    function all_calls(self) -> root.events.ModelCall[] {
        let out: root.events.ModelCall[] = [];
        for (let call in self.calls) {
            out.push(call);
        }
        for (let child in self.child_turns) {
            for (let call in child.all_calls()) {
                out.push(call);
            }
        }
        out
    }

    /// The terminal Text block — the final candidate the runner parses.
    function terminal_text(self) -> string? {
        let out: string? = null;
        for (let b in self.content) {
            if let t: root.content.Text = b {
                out = t.text;
            }
        }
        out
    }

    /// The media payloads the turn produced, in order. A media-shaped output
    /// type is built from these instead of from the terminal text.
    function media(self) -> MediaPart[] {
        let out: MediaPart[] = [];
        for (let b in self.content) {
            if let m: root.content.Media = b {
                out.push(m.media);
            }
        }
        out
    }

    /// The ToolUse blocks of the turn, in order.
    function tool_uses(self) -> root.content.ToolUse[] {
        let out: root.content.ToolUse[] = [];
        for (let b in self.content) {
            if let u: root.content.ToolUse = b {
                out.push(u);
            }
        }
        out
    }
}

interface Client {
    /// The model this client asks for, as configured (`"claude-sonnet-4-5"`).
    /// A wrapper reports a member's.
    function requested_model(self) -> string throws never;
    /// The provider this client's calls bill (`"anthropic"`), as its
    /// `ModelKey`s name it. A wrapper reports a member's.
    function provider(self) -> string throws never;
    /// Render the provider request shape without performing I/O. Reliability
    /// wrappers use this as a stable request-shape preview: they do not sleep,
    /// rotate, or try fallback members while rendering.
    function render(
        self,
        input: ModelTurnInput,
    ) -> baml.http.Request throws root.errors.InvokeError;
    function invoke(self, input: ModelTurnInput) -> ModelTurn throws root.errors.InvokeError;
    /// The `args` this client uses for a `${cache()}` written with none (or
    /// with null): the object its wire's cache marker carries. Null when the
    /// client has no cache marker on its wire, or when the model it is
    /// configured for would reject one; a `${cache()}` then sends nothing.
    ///
    /// A client that sends cache markers passes this to
    /// `ai.wire.prompt_parts(message, default_cache_args)`.
    function default_cache_args(self) -> baml.json.json throws never {
        null
    }
}
