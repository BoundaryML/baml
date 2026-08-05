// A policy is pure logic: (state, journal, event) -> commands.
// The runner (in session.baml) executes commands and performs all IO.
// Mirrors _plan/pages/02_guides/09_policies.md.
//
// The event vocabulary is the policy's associated type `Ev` (default:
// the built-in Event union). A policy widens it by binding
// `type Ev = <wider union>` — typed end to end, no escape hatches.

class CallModel {
}

class RunTool {
    call_id: string,
    tool: string,
    args_json: string,
}

class RetryTool {
    call_id: string,
    after_ms: int,
}

class MountTools {
    names: string[],
}

class UnmountTools {
    names: string[],
}

class SpawnChild {
    child_id: string,
    goal: string,
}

class AwaitInput {
    note: string?,
}

class FinishTurn {
    result_json: string,
}

class CancelAll {
    reason: string,
}

// Fail a tool call without running it (e.g. an approval was denied). The
// runner appends ToolFailed and folds it through the policy like any tool
// outcome — so the inner loop clears the pending call and recalls the
// model, live and on refold alike. Policies must use this rather than
// appending ToolFailed themselves.
class FailTool {
    call_id: string,
    error: string,
}

type Command = CallModel
    | RunTool
    | RetryTool
    | FailTool
    | MountTools
    | UnmountTools
    | SpawnChild
    | AwaitInput
    | FinishTurn
    | CancelAll;

class SessionState {
    steps: int,
    pending_tools: string[],
    queued_msgs: string[],
}

function new_state() -> SessionState {
    SessionState { steps: 0, pending_tools: [], queued_msgs: [] }
}

// Pure: no IO in update — no clock, no randomness, no network, no spawn.
// The runner performs all effects. update must not write the journal
// either: j.record(e) is the one sanctioned write (a journal-only custom
// decision record, refold-safe by construction), and built-in events are
// produced only by the runner, on command (RunTool results, FailTool).
// update MAY throw to abort the live run (budgets); refold swallows the
// throw, because the abort already happened.
interface Policy {
    type Ev = Event

    function init(self) -> SessionState throws never { new_state() }
    function update(self, st: SessionState, j: Journal<Self.Ev>, e: Self.Ev) -> Command[] throws unknown
}

//# The default policy: the standard tool loop, generic over the session's

//# event union. task_mode = true: a reply is not a legal stop.

class ToolLoop<X> {
    max_steps: int,
    task_mode: bool,

    // The step budget counts model turns (01_agents.md): steps increments
    // each time the loop asks for another model call, whatever triggered
    // it — a tool batch completing or a reply-only turn in task mode.
    function next_call(self, st: SessionState) -> Command {
        st.steps += 1;
        if (st.steps > self.max_steps) {
            throw StepBudgetExceeded { message: `step budget exhausted after ${self.max_steps} steps`, steps: self.max_steps };
        }
        CallModel {  }
    }

    implements Policy {
        type Ev = Event | X

        function update(self, st: SessionState, j: Journal<Event | X>, e: Event | X) -> Command[] {
            match (e) {
                let m: UserMessage => {
                    st.steps = 0;
                    [self.next_call(st)]
                },
                let t: ToolRequested => {
                    st.pending_tools.push(t.call_id);
                    [RunTool { call_id: t.call_id, tool: t.tool, args_json: t.args_json }]
                },
                let t: ToolCompleted => {
                    st.pending_tools
                        = st.pending_tools.filter((id) -> {
                            id != t.call_id
                        });
                    if (st.pending_tools.length() == 0) {
                        [self.next_call(st)]
                    } else {
                        []
                    }
                },
                let t: ToolFailed => {
                    st.pending_tools
                        = st.pending_tools.filter((id) -> {
                            id != t.call_id
                        });
                    if (st.pending_tools.length() == 0) {
                        [self.next_call(st)]
                    } else {
                        []
                    }
                },
                let f: FinalProduced => [FinishTurn { result_json: f.result_json }],
                let a: AssistantMessage => {
                    if (st.pending_tools.length() > 0) {
                        []
                    } else if (self.task_mode) {
                        //# a reply is not an exit in task mode, and it costs a step
                        return [self.next_call(st)];
                    } else {
                        [AwaitInput { note: null }]
                    }
                },
                // cancel running work, then let the model react (04_steering.md)
                let i: Interrupted => [CancelAll { reason: i.reason }, self.next_call(st)],
                _ => [],
            }
        }
    }
}

//# Middleware: hold arriving messages; call the model at turn boundaries.

// Arriving UserMessages are already journaled by the runner (the admission
// record); this middleware only controls WHEN the model is next called
// with them. The flush is a pure state transition — no journal writes —
// so refolding it is trivially safe.
class WithSteering<X> {
    inner: Policy<Ev = Event | X>,

    implements Policy {
        type Ev = Event | X

        function update(self, st: SessionState, j: Journal<Event | X>, e: Event | X) -> Command[] {
            match (e) {
                let m: UserMessage => {
                    st.queued_msgs.push(m.content);
                    []
                },
                let a: AssistantMessage => {
                    if (st.queued_msgs.length() > 0) {
                        st.queued_msgs = [];
                        [CallModel {  }]
                    } else {
                        // e is narrowed to AssistantMessage here; widen back so
                        // the inner policy's Ev unifies (see reference notes)
                        let ev: Event | X = a;
                        self.inner.update(st, j, ev)
                    }
                },
                _ => self.inner.update(st, j, e),
            }
        }
    }
}

//# Middleware: retry transient tool failures. Pure: no sleeping — the

//# RetryTool command carries the delay and the runner owns the clock.

class WithRetry<X> {
    inner: Policy<Ev = Event | X>,
    max_attempts: int,
    tools: string[], // which tools are safe to retry (idempotent)

    implements Policy {
        type Ev = Event | X

        function update(self, st: SessionState, j: Journal<Event | X>, e: Event | X) -> Command[] {
            match (e) {
                let f: ToolFailed => {
                    let tool = tool_of(j, f.call_id);
                    if (!self.tools.includes(tool)) {
                        let ev: Event | X = f;
                        return self.inner.update(st, j, ev);
                    }
                    //# attempts are counted from the journal, not from policy state
                    let n = count_failures(j, f.call_id);
                    if (n < self.max_attempts) {
                        [RetryTool { call_id: f.call_id, after_ms: 500 * n }]
                    } else {
                        let ev: Event | X = f;
                        self.inner.update(st, j, ev) // give up; let the model see it
                    }
                },
                _ => self.inner.update(st, j, e),
            }
        }
    }
}

function tool_of<E>(j: Journal<E>, call_id: string) -> string {
    let name = "";
    for (let en in j.entries) {
        if let t: ToolRequested = en.event {
            if (t.call_id == call_id) {
                name = t.tool;
            }
        }
    }
    name
}

function count_failures<E>(j: Journal<E>, call_id: string) -> int {
    let n = 0;
    for (let en in j.entries) {
        if let t: ToolFailed = en.event {
            if (t.call_id == call_id) {
                n += 1;
            }
        }
    }
    n
}

//# Middleware: stop the session at a cost cap (typed error, not a string).

class WithBudget<X> {
    inner: Policy<Ev = Event | X>,
    cap_usd: float,
    spent: float,

    implements Policy {
        type Ev = Event | X

        function update(self, st: SessionState, j: Journal<Event | X>, e: Event | X) -> Command[] {
            match (e) {
                let u: Usage => {
                    self.spent
                        += (u.input_tokens * 1.0) * 0.000002 + (u.output_tokens * 1.0) * 0.00001;
                    if (self.spent > self.cap_usd) {
                        throw CostBudgetExceeded { message: "cost budget exceeded", spent_usd: self.spent };
                    }
                    return [];
                },
                _ => self.inner.update(st, j, e),
            }
        }
    }
}
