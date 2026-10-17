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
    /// Merged verbatim into inference requests' `message` objects.
    metadata: map<string, baml.json.json>,

    function new(text: string) -> UserMessage {
        UserMessage { content: [root.content.Text { text: text }], metadata: {} }
    }

    /// A turn from ordinary values: text, the `image` / `audio` / `video` /
    /// `pdf` primitives, or blocks already built (see `ai.content.blocks`).
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
    // The answering client's; see `ModelTurn`.
    resolved_model: ModelKey?,
    requested_model: string?,
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
    /// `pdf` primitives, or blocks already built (see `ai.content.blocks`).
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

/// The tokens a model call reported. Every count is null when the provider
/// did not report it, which is not the same as reporting zero.
class Usage {
    input_tokens: int?,
    output_tokens: int?,
    cached_input_tokens: int?,
    reasoning_tokens: int?,
    /// Tokens written to the provider's prompt cache, billed above the
    /// input rate. Null when the provider reports none.
    cache_write_input_tokens: int?,
    /// Input tokens billed at the full rate: `input_tokens` without cache
    /// reads and writes, whichever way the provider counts them. Null when
    /// the provider cannot tell.
    uncached_input_tokens: int?,
}

/// Who answered and billed: the client's provider id (`"anthropic"`,
/// `"openrouter"`, ...) and the model the response reported, which can be
/// more specific than the one requested (a dated snapshot, the model a router
/// chose). Falls back to the requested model when the response names none;
/// null for a call no response answered.
class ModelKey {
    provider: string,
    model: string?,
}

// Usage lists stand in for `map<ModelKey, Usage>` until SDK generators and
// the bridge wire format support non-string map keys; they are string-keyed
// today, so a class-keyed map cannot reach host SDKs.

/// One model's usage: the counts its calls reported, summed.
class ModelUsage {
    resolved_model: ModelKey,
    usage: Usage,
}

/// The usage `calls` reported, summed per model, in the order models first
/// appear. A turn's total is `call_usage(turn.all_calls())`.
function call_usage(calls: ModelCall[]) -> ModelUsage[] {
    let out: ModelUsage[] = [];
    for (let call in calls) {
        if let u: Usage = call.usage {
            merge_usage(out, [ModelUsage { resolved_model: call.resolved_model, usage: u }]);
        }
    }
    out
}

/// The usage `usage` lists for `model`, or null when it lists none.
function usage_for(usage: ModelUsage[], model: ModelKey) -> Usage? {
    usage
        .find((entry: ModelUsage) -> bool {
            entry.resolved_model == model
        })
        ?.usage
}

/// Add every entry of `from` into `into`, summing the counts of a model both
/// list. A nullable count stays null only when both sides are null. `into`
/// keeps one entry per model, in the order models first appear.
function merge_usage(into: ModelUsage[], from: ModelUsage[]) -> void {
    for (let added in from) {
        let existing = into.find((entry: ModelUsage) -> bool {
            entry.resolved_model == added.resolved_model
        });
        match (existing) {
            let entry: ModelUsage => {
                entry.usage = _sum_usage(entry.usage, added.usage);
                null
            },
            // A copy, so summing into it later leaves `from` unchanged.
            null => {
                into.push(ModelUsage { resolved_model: added.resolved_model, usage: added.usage });
                null
            },
        };
    }
}

function _sum_usage(a: Usage, b: Usage) -> Usage {
    Usage {
        input_tokens: _sum_count(a.input_tokens, b.input_tokens),
        output_tokens: _sum_count(a.output_tokens, b.output_tokens),
        cached_input_tokens: _sum_count(a.cached_input_tokens, b.cached_input_tokens),
        reasoning_tokens: _sum_count(a.reasoning_tokens, b.reasoning_tokens),
        cache_write_input_tokens: _sum_count(a.cache_write_input_tokens, b.cache_write_input_tokens),
        uncached_input_tokens: _sum_count(a.uncached_input_tokens, b.uncached_input_tokens),
    }
}

function _sum_count(a: int?, b: int?) -> int? {
    if (a == null && b == null) {
        null
    } else {
        (a ?? 0) + (b ?? 0)
    }
}

class FinalProduced {
    value_json: string,
}

// ---------------------------------------------------------------------------
// Call records (v0-Collector-shaped). One ModelCall per call to a model,
// including the calls of failed retry/fallback attempts.

class Timing {
    // Wall-clock start of the call. Null for a failed attempt's call, whose
    // start was never measured — never fabricated.
    start_time: baml.time.Instant?,
    // Null when the attempt failed before it was measured.
    duration: baml.time.Duration?,
    // Time to first data event; streams only.
    ttft: baml.time.Duration?,
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

class ModelCall {
    // Who the call billed. The model is null when no response named one,
    // as for a failed attempt's call.
    resolved_model: ModelKey,
    // The tokens the call reported; null when the provider reported none.
    usage: Usage?,
    timing: Timing,
    // Reused as-is; only when capture_wire. Rendered requests carry no
    // credential headers (auth is applied at send); the URL is redacted.
    // Null for a call that is not an HTTP request.
    http_request: baml.http.Request?,
    // Null on transport failure and for a call that is not an HTTP request.
    http_response: HttpResponse?,
    // Streams, only when capture_wire.
    sse_responses: SseEventRecord[]?,
}

type Event = RunStarted
    | UserMessage
    | AssistantMessage
    | ToolRequested
    | ToolCompleted
    | ToolFailed
    | ModelCall
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
