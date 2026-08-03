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

type Command = CallModel
    | RunTool
    | RetryTool
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

// Pure: no IO in update. The runner performs all effects.
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

    implements Policy {
        type Ev = Event | X

        function update(self, st: SessionState, j: Journal<Event | X>, e: Event | X) -> Command[] {
            match (e) {
                let m: UserMessage => {
                    st.steps = 0;
                    [CallModel {  }]
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
                    st.steps += 1;
                    if (st.steps >= self.max_steps) {
                        throw StepBudgetExceeded { message: `step budget exhausted after ${st.steps} steps`, steps: st.steps };
                    }
                    if (st.pending_tools.length() == 0) {
                        [CallModel {  }]
                    } else {
                        []
                    }
                },
                let t: ToolFailed => {
                    st.pending_tools
                        = st.pending_tools.filter((id) -> {
                            id != t.call_id
                        });
                    st.steps += 1;
                    if (st.pending_tools.length() == 0) {
                        [CallModel {  }]
                    } else {
                        []
                    }
                },
                let f: FinalProduced => [FinishTurn { result_json: f.result_json }],
                let a: AssistantMessage => {
                    if (st.pending_tools.length() > 0) {
                        []
                    } else if (self.task_mode) {
                        st.steps += 1;
                        if (st.steps >= self.max_steps) {
                            throw StepBudgetExceeded { message: `step budget exhausted after ${st.steps} steps`, steps: st.steps };
                        }
                        return [CallModel {  }];
                    } else {
                        [AwaitInput { note: null }]
                    }
                },
                let i: Interrupted => [CancelAll { reason: i.reason }],
                _ => [],
            }
        }
    }
}

//# Middleware: buffer messages, inject at turn boundaries.

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
                        while let q: string = st.queued_msgs.shift() {
                            j.append(UserMessage { content: q });
                        }
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
