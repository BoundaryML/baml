// The event catalog: the typed things that can appear in a root.Journal.
// Only the runner produces events — clients and tools never append.
//
// Mirrors _planv2/pages/04_reference/02_events.md.

class RunStarted {
    spec_name: string,
    arguments: map<string, unknown>,
}

class UserMessage {
    content: string,
}

class AssistantMessage {
    content: root.content.Block[],
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

// Wrap a listener so a listener error can never fail or alter a run.
// A null listener stays null; callers `if let` and skip emission entirely.
// `E` accepts fallible listeners (host callables included); the wrapper
// swallows whatever they throw, hence the `throws never` on the returned type.
function guard<E>(
    listener: ((Event) -> void throws E)?,
) -> ((Event) -> void throws never)? {
    if let cb: (Event) -> void throws E = listener {
        (e: Event) -> void {
            cb(e) catch_all (err) { _ => {} }
        }
    } else {
        null
    }
}
