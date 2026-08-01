# Example: a coding agent (mini Claude Code)

This example composes most of the system: an agent with file and shell
tools, message queuing, permission gates with custom events, subagents,
Esc-to-cancel, and a terminal UI driven by the journal tail.

## The agent

```baml
class Report {
    summary: string,
    files_changed: string[],
}

/// A coding agent that works in the current repository.
function CodeAgent(task: string) -> Report {
    client: "anthropic/claude-sonnet-5"
    tools: [read_file, write_file, run_bash, sub_task]
    prompt: `
        You are a coding agent. Work the task to completion.
        Task: ${task}
        ${ctx.transcript}
        ${ctx.output_format}
    `
}

/// Delegate a focused task to a subagent; returns its report.
function sub_task(goal: string) -> string {
    baml.json.to_string(CodeAgent(goal))    // child session; Esc cancels it via the tree
}
```

## Custom events

The permission flow needs events that are neither user messages nor
interrupts:

```baml
class PermissionRequested { call_id: string, tool: string, why: string }
class PermissionGranted   { call_id: string }
class PermissionDenied    { call_id: string }

type CCEvent = baml.session.Event
             | PermissionRequested | PermissionGranted | PermissionDenied
```

## The policy stack

Steering (queue messages, flush at turn boundaries, cancel on interrupt)
wraps the permission gate, which wraps the default loop:

```baml
class WithSteering {
    inner: baml.session.Policy,
    implements baml.session.Policy {
        type Ev = CCEvent
        function update(self, st: SessionState, j: Journal<CCEvent>, e: CCEvent) -> Command[] {
            match (e) {
                //# buffer messages instead of firing the model immediately
                let m: UserMessage => { st.queued_msgs.push(m.content); [] },
                //# turn boundary: flush the buffer, then continue
                let a: AssistantSaid => {
                    if (st.queued_msgs.length() > 0) {
                        while let q: string = st.queued_msgs.shift() {
                            j.append(UserMessage { content: q });
                        }
                        [CallModel {}]
                    } else { self.inner.update(st, j, e) }
                },
                //# esc: kill children and running tools, then let the model react
                let i: Interrupted => [CancelAll { reason: i.reason }, CallModel {}],
                _ => self.inner.update(st, j, e),
            }
        }
    }
}

let policy = WithSteering { inner:
             WithApproval { needs_ok: ["run_bash", "write_file"], held: {}, inner:
             baml.session.ToolLoop { max_steps: 100 } } };
```

(`WithApproval` is shown in full in `../02_guides/09_policies.md`.)

## The app

Three concerns, three lanes: render the journal tail, run turns, read the
human.

```baml
function main() -> null {
    let s = CodeAgent@session(task = "fix the failing tests", policy = policy);

    //# UI lane: tail the journal, render events as they land
    spawn {
        let seen = 0;
        while (true) {
            for (let en in s.journal().read_from(seen)) {
                seen = en.seq + 1;
                match (en.event) {
                    let a: AssistantSaid => print(`● ${a.content}`),
                    let t: ToolRequested => print(`⚒ ${t.tool}(${t.args_json})`),
                    let p: PermissionRequested => print(`? allow ${p.tool}? [y/n]`),
                    let c: ChildSpawned => print(`↳ subagent: ${c.goal}`),
                    _ => { },
                }
            }
            baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));
        }
    };

    //# agent lane: keep folding turns as input arrives
    spawn { while (true) { let _ = s.run_blocking(); } };

    //# human lane: stdin — say, approve, interrupt
    while (true) {
        let line = read_line();
        if (line == "y")          { s.emit(PermissionGranted { call_id: pending_permission(s) }); }
        else if (line == "n")     { s.emit(PermissionDenied  { call_id: pending_permission(s) }); }
        else if (line == "<esc>") { s.interrupt("user pressed esc"); }
        else if (line == "/quit") { return null; }
        else                      { s.send(line); }
    }
}

//# a fold over the journal: the most recent unanswered permission request
function pending_permission(s: Session<Report, CCEvent>) -> string {
    let answered: string[] = [];
    let pending = "";
    for (let en in s.journal().read_from(0)) {
        match (en.event) {
            let g: PermissionGranted => answered.push(g.call_id),
            let d: PermissionDenied  => answered.push(d.call_id),
            let p: PermissionRequested =>
                if (!answered.includes(p.call_id)) { pending = p.call_id; },
            _ => { },
        }
    }
    pending
}
```

## Tracing the flows

- **A normal message** takes the data lane: `send` → `WithSteering`
  buffers it → injected at the turn boundary → the model sees it in
  order.
- **Esc** takes the control lane: `interrupt` → cancel tokens stop
  `run_bash` and any subagents now → `Interrupted` is journaled →
  the model gets to react.
- **A `run_bash` attempt** round-trips through custom events:
  `ToolRequested` → the gate holds it, journals `PermissionRequested`,
  ends the turn → UI renders the question → `emit(PermissionGranted)` →
  the held command executes.
- **Kill the process** at any point: `snapshot()`, restart, `resume` —
  the pending permission request is still pending, because it is an
  event in the journal, not a variable in memory.

`pending_permission` shows the idiom for reading session state: fold the
journal. There is no second state store to query or keep consistent.
