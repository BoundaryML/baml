// The event catalog: the typed things that can appear in a root.Journal.
// Only the runner produces events — clients and tools never append.

class RunStarted {
    spec_name: string,
    arguments: map<string, unknown>,
}

/// A user turn: text and media, in order. The runner records the opening
/// turn; a runner that observes its environment appends more mid-run (a
/// screenshot after a tool ran, a document the host attaches), and every
/// client lowers the turn at its position in the journal.
class UserMessage {
    content: root.content.Block[],
    /// Per-message provider directives, carried verbatim like
    /// `root.PromptMessage.metadata` (Anthropic `cache_control`, for example).
    metadata: map<string, baml.json.json>,

    function new(text: string) -> UserMessage {
        UserMessage { content: [root.content.Text { text: text }], metadata: {} }
    }

    /// A turn from ordinary values: text, the `image` / `audio` / `video` /
    /// `pdf` primitives, or blocks already built (see `root.content.blocks`).
    function of(
        items: (string | image | audio | video | pdf | root.content.Text | root.content.Media)[],
        metadata: map<string, baml.json.json> = {},
    ) -> UserMessage {
        UserMessage { content: root.content.blocks(items), metadata: metadata }
    }

    /// The `Text` blocks joined; a media block contributes a `[image]`-style
    /// placeholder.
    function text(self) -> string {
        root.content.text(self.content)
    }
}

class AssistantMessage {
    content: root.content.ModelBlock[],
    client_id: string,
}

class ToolRequested {
    id: string,
    name: string,
    args: map<string, unknown>,
}

/// A tool's result: its output text, plus any media the tool produced (a
/// screenshot from a computer-use tool, a rendered chart).
class ToolCompleted {
    id: string,
    content: root.content.Block[],

    /// `output` is the tool's result serialized as JSON text.
    function new(id: string, output: string) -> ToolCompleted {
        ToolCompleted { id: id, content: [root.content.Text { text: output }] }
    }

    /// A result from ordinary values: text, the `image` / `audio` / `video` /
    /// `pdf` primitives, or blocks already built (see `root.content.blocks`).
    function of(
        id: string,
        items: (string | image | audio | video | pdf | root.content.Text | root.content.Media)[],
    ) -> ToolCompleted {
        ToolCompleted { id: id, content: root.content.blocks(items) }
    }

    /// The `Text` blocks joined — the tool's text output; a media block
    /// contributes a `[image]`-style placeholder.
    function text(self) -> string {
        root.content.text(self.content)
    }
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

/// Response snapshot; baml.http.Response holds a native body handle and
/// cannot ride events.
class HttpResponse {
    status: int,
    headers: map<string, string>,
    // Only when the client set capture_wire.
    body: string?,
}

/// Raw SSE data field, streams only. Duplicates ai.stream.SseEvent's shape so
/// ns_events does not depend on ns_stream.
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
    | AssistantMessage
    | ToolRequested
    | ToolCompleted
    | ToolFailed
    | Usage
    | LLMCall
    | FinalProduced;

/// Wrap a listener so a listener error can never fail or alter a run.
/// A null listener stays null; callers `if let` and skip emission entirely.
/// `E` accepts fallible listeners (host callables included); the wrapper
/// swallows whatever they throw, hence the `throws never` on the returned type.
function guard<E>(listener: ((Event) -> void throws E)?) -> ((Event) -> void throws never)? {
    if let cb: (Event) -> void throws E = listener {
        (e: Event) -> void {
            cb(e) catch_all (err) {
                _ => {}
            }
        }
    } else {
        null
    }
}
