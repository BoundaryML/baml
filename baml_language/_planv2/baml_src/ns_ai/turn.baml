// The client boundary: one operation. A client renders the input into its
// wire format, makes one call, and returns canonical content. It never
// writes the journal, executes a tool, or parses the output type.
// Mirrors _planv2/pages/02_guides/03_clients/02_the_client_interface.md.

class ModelTurnInput {
    prompt: Prompt,
    journal: Journal,
    toolbox: Toolbox,
    output_type: type,
}

class ModelTurn {
    content: ContentBlock[],
    stop_reason: StopReason,
    usage: Usage?,

    // The terminal Text block — the final candidate the runner parses.
    function terminal_text(self) -> string? {
        let out: string? = null;
        for (let b in self.content) {
            if let t: Text = b {
                out = t.text;
            }
        }
        out
    }

    // The ToolUse blocks of the turn, in order.
    function tool_uses(self) -> ToolUse[] {
        let out: ToolUse[] = [];
        for (let b in self.content) {
            if let u: ToolUse = b {
                out.push(u);
            }
        }
        out
    }
}

interface Client {
    function id(self) -> string throws never
    function invoke(self, input: ModelTurnInput) -> ModelTurn throws unknown
}


