// Canonical assistant content: what a model turn produced, normalized away
// from any one provider's envelope. Clients build these; the journal records
// them inside root.events.AssistantMessage.
//
// Mirrors _planv2/pages/04_reference/01_api.md (the `ai.content` namespace).

// A model turn is structured blocks, never a bare string and never an HTTP
// envelope.
class Text {
    text: string,
}

class Reasoning {
    summary: string,
}

class ToolUse {
    id: string,
    name: string,
    args: map<string, unknown>,
}

type Block = Text | Reasoning | ToolUse;

// Why the model stopped. `ToolUse` means the turn ended with tool calls the
// runner must execute before the next turn.
enum StopReason {
    Complete,
    ToolUse,
    MaxTokens,
    Refused,
}
