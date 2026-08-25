// The client boundary: one operation. A client renders the input into its
// wire format, makes one call, and returns canonical content. It never
// writes the journal, executes a tool, or parses the output type.
// Mirrors _planv2/pages/02_guides/03_clients/02_the_client_interface.md.

class ModelTurnInput {
    // The bound prompt template. The client supplies the output-format schema
    // handle and receives one structural prompt.
    prompt: (OutputFormat) -> Prompt throws never,
    journal: Journal,
    toolbox: root.tools.Toolbox,
    output_type: reflect.Type,
}

class ModelTurn {
    content: root.content.Block[],
    stop_reason: root.content.StopReason,
    usage: root.events.Usage?,

    // The terminal Text block — the final candidate the runner parses.
    function terminal_text(self) -> string? {
        let out: string? = null;
        for (let b in self.content) {
            if let t: root.content.Text = b {
                out = t.text;
            }
        }
        out
    }

    // The media payloads the turn produced, in order. A media-shaped output
    // type is built from these instead of from the terminal text.
    function media(self) -> MediaPart[] {
        let out: MediaPart[] = [];
        for (let b in self.content) {
            if let m: root.content.Media = b {
                out.push(m.media);
            }
        }
        out
    }

    // The ToolUse blocks of the turn, in order.
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
    function id(self) -> string throws never
    // Render the provider request shape without performing I/O. Reliability
    // wrappers use this as a stable request-shape preview: they do not sleep,
    // rotate, or try fallback members while rendering.
    function render(self, input: ModelTurnInput) -> baml.http.Request throws root.errors.InvokeError
    function invoke(self, input: ModelTurnInput) -> ModelTurn throws root.errors.InvokeError
}
