// The journal: an append-only log of typed events. One per session.
// Mirrors _plan/pages/02_guides/10_journal.md.

class SessionStarted {
    function_name: string,
    args_json: string,
}

class UserMessage {
    content: string,
}

class AssistantMessage {
    content: string,
    raw_json: string?,
    provider: string,
}

class ToolRequested {
    call_id: string,
    tool: string,
    args_json: string,
}

class ToolCompleted {
    call_id: string,
    result_json: string,
}

class ToolFailed {
    call_id: string,
    error: string,
}

class FinalProduced {
    result_json: string,
}

class ToolsChanged {
    names: string[],
}

class StepCompleted {
    call_id: string,
    step: string,
    result_json: string,
}

class ChildSpawned {
    child_id: string,
    goal: string,
}

class ChildFinished {
    child_id: string,
    result_json: string,
}

class Interrupted {
    reason: string,
}

class Usage {
    input_tokens: int,
    output_tokens: int,
}

class Compacted {
    summary: string,
    through_seq: int,
}

type Event = SessionStarted
    | UserMessage
    | AssistantMessage
    | ToolRequested
    | ToolCompleted
    | ToolFailed
    | FinalProduced
    | ToolsChanged
    | StepCompleted
    | ChildSpawned
    | ChildFinished
    | Interrupted
    | Usage
    | Compacted;

class Entry<E> {
    seq: int,
    event: E,
}

class Journal<E> {
    entries: Entry<E>[],

    function append(self, e: E) -> void {
        self.entries.push(Entry { seq: self.entries.length(), event: e });
    }

    function read_from(self, seq: int) -> Entry<E>[] {
        // .filter() infers Entry<never>[] here (see reference notes); build manually
        let out: Entry<E>[] = [];
        for (let en in self.entries) {
            if (en.seq >= seq) {
                out.push(en);
            }
        }
        out
    }

    function last_seq(self) -> int {
        self.entries.length() - 1
    }

    function snapshot(self) -> string {
        baml.json.to_string(self)
    }
}

function new_journal<E>() -> Journal<E> {
    Journal<E> { entries: [] }
}

function resume_journal<E>(snap: string) -> Journal<E> {
    baml.json.from_string<Journal<E>>(snap)
}

//# The transcript is a rendering of the journal, not storage.

// The real feature (${ctx.transcript}) lowers provider-natively; this
// library fold renders to text.
function render_transcript<E>(j: Journal<E>) -> string {
    j.entries
        .map((en) -> {
            match (en.event) {
                let s: SessionStarted => `[briefing] ${s.args_json}`,
                let u: UserMessage => `[user] ${u.content}`,
                let a: AssistantMessage => `[assistant] ${a.content}`,
                let t: ToolRequested => `[calling] ${t.tool}(${t.args_json})`,
                let t: ToolCompleted => `[result] ${t.result_json}`,
                let t: ToolFailed => `[tool error] ${t.error}`,
                let c: Compacted => `[earlier, summarized] ${c.summary}`,
                _ => "",
            }
        })
        .filter((s) -> {
            s != ""
        })
        .join("\n")
}

// Event factories — the library form of `baml.session.user(...)`.
// Also the workaround for qualified struct literals inside array literals
// (see _plan/reference_notes.md).
function user(content: string) -> Event {
    UserMessage { content: content }
}

function assistant(content: string, provider: string = "test", raw_json: string? = null) -> Event {
    AssistantMessage { content: content, raw_json: raw_json, provider: provider }
}

function tool_requested(call_id: string, tool: string, args_json: string) -> Event {
    ToolRequested { call_id: call_id, tool: tool, args_json: args_json }
}

function tool_completed(call_id: string, result_json: string) -> Event {
    ToolCompleted { call_id: call_id, result_json: result_json }
}

function final_produced(result_json: string) -> Event {
    FinalProduced { result_json: result_json }
}

function usage_of(input_tokens: int, output_tokens: int) -> Event {
    Usage { input_tokens: input_tokens, output_tokens: output_tokens }
}
