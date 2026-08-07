// Content blocks, journal events, and the Journal itself.
// Mirrors _planv2/pages/02_guides/04_the_journal.md and 04_reference/02_events.md.

// Canonical assistant content. A model turn is structured blocks, never a
// bare string and never an HTTP envelope.
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

type ContentBlock = Text | Reasoning | ToolUse;

enum StopReason {
    Complete,
    ToolUse,
    MaxTokens,
    Refused,
}

// Built-in events. Only the runner produces events.
class RunStarted {
    spec_name: string,
    arguments: map<string, unknown>,
}

class UserMessage {
    content: string,
}

class AssistantMessage {
    content: ContentBlock[],
    client_id: string,
}

class ToolRequested {
    id: string,
    name: string,
    args: map<string, unknown>,
}

class ToolCompleted {
    id: string,
    output: string, // the tool's result, serialized as JSON text
}

class ToolFailed {
    id: string,
    message: string,
}

class Usage {
    input_tokens: int,
    output_tokens: int,
    cached_input_tokens: int?,
    reasoning_tokens: int?,
}

class FinalProduced {
    value_json: string,
}

type Event = RunStarted
    | UserMessage
    | AssistantMessage
    | ToolRequested
    | ToolCompleted
    | ToolFailed
    | Usage
    | FinalProduced;

// The append-only typed record of one run: the COMPLETE record. Repair
// attempts, feedback, failures — everything that happened is an event.
class Journal {
    log: Event[],

    function entries(self) -> Event[] {
        self.log
    }

    // A journal for one run of a spec, with RunStarted appended.
    function new<Out>(spec: FunctionSpec<Out>) -> Journal {
        Journal { log: [RunStarted { spec_name: spec.name(), arguments: spec.arguments() }] }
    }

    // The driving runner is the only writer; clients and tools never append.
    function append_all(self, events: Event[]) -> void {
        for (let e in events) {
            self.log.push(e);
        }
    }
}
