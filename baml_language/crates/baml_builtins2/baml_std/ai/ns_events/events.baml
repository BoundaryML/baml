// The event catalog: the typed things that can appear in a root.Journal.
// Only the runner produces events — clients and tools never append.

class RunStarted {
    spec_name: string,
    arguments: map<string, unknown>,
}

class UserMessage {
    content: string,
}

/// A user turn carrying prompt parts: text and media, in order. A runner
/// appends it imperatively mid-run — a screenshot after a tool ran, a
/// document the host attaches — and every client lowers it to a user
/// message at its position in the journal. `UserMessage` stays text-only.
class UserContent {
    parts: root.PromptPart[],

    /// Build a turn from ordinary values: text, and the `image` / `audio` /
    /// `video` / `pdf` primitives `baml.media.*.from_base64` and friends
    /// return. Those primitives are distinct static types from the
    /// `baml.media.*` wrapper classes `root.PromptPart` is a union of, so a
    /// media value bridges through `root.internal.media_part` (exactly as
    /// `root.content.Media.new` does); text passes straight through. Parts
    /// that are already `root.PromptPart` — a rendered prompt's, a
    /// `root.content.Media`'s — go in the plain constructor instead.
    function new(parts: (string | image | audio | video | pdf)[]) -> UserContent {
        let out: root.PromptPart[] = [];
        for (let part in parts) {
            if let text: string = part {
                out.push(text);
            } else {
                out.push(root.internal.media_part(part));
            }
        }
        UserContent { parts: out }
    }
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


// ---------------------------------------------------------------------------
// Wire records (v0-Collector-shaped). One LLMCall per HTTP exchange,
// INCLUDING failed retry/fallback legs (selected: false).

class Timing {
    // Wall-clock UTC start of the exchange; from baml.time.Instant.now().
    // Null on failed legs, whose start was never measured — never fabricated.
    start_time_utc_ms: bigint?,
    // Total duration; null when the leg failed before it could be measured.
    duration_ms: bigint?,
    // Time to first data event; streams only.
    ttft_ms: bigint?,
}

// Response snapshot; baml.http.Response holds a native body handle and
// cannot ride events.
class HttpResponse {
    status: int,
    headers: map<string, string>,
    // Only when the client set capture_wire.
    body: string?,
}

// Raw SSE data field, streams only. Duplicates ai.stream.SseEvent's shape so
// ns_events does not depend on ns_stream.
class SseEventRecord {
    event: string?,
    data: string?,
    id: string?,
}

class LLMCall {
    client_name: string,
    provider: string,
    timing: Timing,
    // Reused as-is; only when capture_wire. Rendered requests carry no
    // credential headers (auth is applied at send); the URL is redacted.
    http_request: baml.http.Request?,
    // Null on transport failure.
    http_response: HttpResponse?,
    usage: Usage?,
    selected: bool,
    // Streams, only when capture_wire.
    sse_responses: SseEventRecord[]?,
}

type Event = RunStarted
    | UserMessage
    | UserContent
    | AssistantMessage
    | ToolRequested
    | ToolCompleted
    | ToolFailed
    | Usage
    | LLMCall
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
