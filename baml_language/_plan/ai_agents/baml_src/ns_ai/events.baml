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

// A setter changed the session's client (03_configuration.md). Journal-only.
class ClientChanged {
    client_id: string,
}

// A setter changed the session's policy. Journal-only. Records that the
// change happened, not the value: a policy is a runtime value (often
// closing over state), so resume takes the policy from the caller and
// this event is audit history (see reference notes, spec issues).
// The literal field is a wire discriminant: an EMPTY class in a journaled
// union structurally matches any JSON object on resume and swallows every
// variant after it (see reference notes).
class PolicyChanged {
    kind: "policy_changed",
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
    | ClientChanged
    | PolicyChanged
    | StepCompleted
    | ChildSpawned
    | ChildFinished
    | Interrupted
    | Usage
    | Compacted;

// Custom events are invisible to the model unless they implement this.
// Mirrors `baml.session.Promptable` in 11_journal.md.
interface Promptable {
    function to_prompt(self) -> string? throws never
}

class Entry<E> {
    seq: int,
    event: E,
    // true = written by a policy via record(): journal-only history that was
    // never folded live, so refold skips it too (see refold in session.baml)
    recorded: bool,
}

class Journal<E> {
    entries: Entry<E>[],
    replaying: bool, // set by refold; suppresses record()

    // runner-side: append an event that IS folded through the policy
    function append(self, e: E) -> void {
        self.entries.push(Entry { seq: self.entries.length(), event: e, recorded: false });
    }

    // policy-side: record a journal-only decision (e.g. PermissionRequested).
    // Never folded; suppressed during refold, so replay cannot duplicate it.
    // This is the ONLY journal write allowed inside Policy.update.
    function record(self, e: E) -> void {
        if (!self.replaying) {
            self.entries.push(Entry { seq: self.entries.length(), event: e, recorded: true });
        }
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
    Journal<E> { entries: [], replaying: false }
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
                let p: Promptable => p.to_prompt() ?? "", // custom events opt in
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
