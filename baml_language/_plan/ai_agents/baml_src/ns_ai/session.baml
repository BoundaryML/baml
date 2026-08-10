// Session = Journal + Client + Toolbox + Policy + the runner.
// The runner (inside run()) is the only place IO happens.
// Mirrors _plan/pages/02_guides/02_sessions.md.
//
// T is the typed goal; X is the CUSTOM EVENT EXTENSION — the session's
// full event union is `Event | X`, so built-in events are members by
// construction and no lower-bounded generics are needed. A session with
// no custom events is Session<T, never>.
//
// Configuration is not an argument: a session is configured on the value
// (set_client / set_policy / mount) — the library form of the BEP's
// `with baml.session.options(...)` clause.

class Done<T> { result: T }
class Replied { message: string }
// NOTE: `type Turn<T> = Done<T> | Replied` — generic type aliases do not
// parse yet (see _plan/reference_notes.md); the union is written inline.

class Session<T, X> {
    journal: Journal<Event | X>,
    toolbox: Toolbox,
    registry: Tool[],            // known-but-unmounted tools (MountTools source)
    client: Client<Ev = Event | X>,
    policy: Policy<Ev = Event | X>,
    st: SessionState,
    queued: string[],            // data lane: user messages
    queued_events: X[],          // data lane: custom events
    interrupts: string[],        // control lane: never waits behind data
    cancel: baml.spawn.CancelToken,
    output_schema: string,

    // ---- data lane: strings and CUSTOM events only. Built-in events are
    // produced by the runner alone — a caller cannot forge model history.
    function send(self, m: string | X) -> void {
        match (m) {
            let s: string => { self.queued.push(s); },
            _ => { self.queued_events.push(m); },
        }
    }

    // ---- control lane ----
    function interrupt(self, reason: string) -> void {
        self.cancel.cancel();
        self.interrupts.push(reason);
    }

    // ---- configuration; every setter is journaled (03_configuration.md) ----
    function set_client(self, c: Client<Ev = Event | X>) -> void {
        self.client = c;
        self.journal.append(ClientChanged { client_id: c.id() });
    }
    function set_policy(self, p: Policy<Ev = Event | X>) -> void {
        self.policy = p;
        self.journal.append(PolicyChanged { kind: "policy_changed" });
        self.st = refold(p, self.journal);
    }
    function register(self, t: Tool) -> void { self.registry.push(t); }

    function snapshot(self) -> string { self.journal.snapshot() }

    function mount(self, names: string[]) -> void {
        for (let n in names) {
            if let t: Tool = self.registry.find((r) -> { r.name == n }) {
                self.toolbox.entries.push(t);
            }
        }
        // The registry holds known-but-UNMOUNTED tools, so a tool leaves it on
        // mount. Leaving it behind would let the same name mount twice.
        self.registry = self.registry.filter((r) -> { !names.includes(r.name) });
        self.journal.append(ToolsChanged { names: self.toolbox.names() });
    }

    function unmount(self, names: string[]) -> void {
        // Symmetrically, an unmounted tool returns to the registry. Dropping it
        // here instead would lose it for good and make MountTools unable to
        // ever bring it back.
        for (let t in self.toolbox.entries.filter((t) -> { names.includes(t.name) })) {
            self.registry.push(t);
        }
        self.toolbox.entries = self.toolbox.entries.filter((t) -> { !names.includes(t.name) });
        self.journal.append(ToolsChanged { names: self.toolbox.names() });
    }

    // The runner: append event -> ask policy -> execute commands -> repeat.
    function run(self) -> Done<T> | Replied {
        let inbox: (Event | X)[] = [];
        let cmds: Command[] = [];

        //# control first — an interrupt never waits behind queued data
        while let r: string = self.interrupts.shift() {
            inbox.push(Interrupted { reason: r });
        }
        //# then the data lane
        while let m: string = self.queued.shift() {
            inbox.push(UserMessage { content: m });
        }
        while let qe: X = self.queued_events.shift() {
            inbox.push(qe);
        }
        if (inbox.length() == 0) {
            cmds.push(CallModel { });    // first run: the briefing is in the journal
        }

        while (true) {
            //# events first: append, fold through the policy
            if let e: Event | X = inbox.shift() {
                self.journal.append(e);
                for (let c in self.policy.update(self.st, self.journal, e)) {
                    cmds.push(c);
                }
                continue;
            }
            //# then commands: the only IO in the system
            if let cmd: Command = cmds.shift() {
                match (cmd) {
                    let c: CallModel => {
                        // one model call covers every queued trigger: coalesce
                        cmds = cmds.filter((x) -> { !(x is CallModel) });
                        let req = self.client.render(self.journal, self.toolbox, self.output_schema);
                        let resp = self.client.invoke(req);
                        for (let e2 in self.client.ingest(resp)) { inbox.push(e2); }
                    },
                    let c: RunTool => {
                        //# a tool error becomes a ToolFailed event, not a crash —
                        //# the model sees it and adapts (01_errors_and_retries.md)
                        let ev: Event = ToolCompleted {
                            call_id: c.call_id,
                            result_json: self.toolbox.call(c.tool, parse_args(c.args_json)),
                        } catch_all (e) {
                            _ => ToolFailed { call_id: c.call_id, error: e.to_string() },
                        };
                        inbox.push(ev);
                    },
                    let c: RetryTool => {
                        // re-run from the journaled request. The runner owns the
                        // clock; this sync sketch ignores after_ms rather than sleep.
                        if let orig: ToolRequested = find_request(self.journal, c.call_id) {
                            let ev: Event = ToolCompleted {
                                call_id: orig.call_id,
                                result_json: self.toolbox.call(orig.tool, parse_args(orig.args_json)),
                            } catch_all (e) {
                                _ => ToolFailed { call_id: orig.call_id, error: e.to_string() },
                            };
                            inbox.push(ev);
                        }
                    },
                    let c: FailTool => {
                        // fail without running: appended AND folded like any
                        // tool outcome, so the inner loop sees it (live + refold)
                        inbox.push(ToolFailed { call_id: c.call_id, error: c.error });
                    },
                    let c: MountTools => { self.mount(c.names); },
                    let c: UnmountTools => { self.unmount(c.names); },
                    let c: SpawnChild => {
                        // sketch: child sessions not wired in the reference yet
                        self.journal.append(ChildSpawned { child_id: c.child_id, goal: c.goal });
                    },
                    let c: AwaitInput => {
                        return Replied { message: last_assistant(self.journal) };
                    },
                    let c: FinishTurn => {
                        // the typed goal: parse the final into T right here
                        return Done { result: baml.json.from_string<T>(c.result_json) };
                    },
                    let c: CancelAll => {
                        //# fire cancel tokens; NOT a turn-ender — only AwaitInput
                        //# and FinishTurn end a run (10_policies.md, runner spec)
                        self.cancel.cancel();
                        self.cancel = baml.spawn.CancelToken.new();
                    },
                }
                continue;
            }
            //# nothing to do: the session is waiting for input
            return Replied { message: last_assistant(self.journal) };
        }
        Replied { message: "" }    // unreachable; while(true) always returns
    }
}

function parse_args(args_json: string) -> map<string, unknown> {
    baml.json.from_string<map<string, unknown>>(args_json) catch_all (e) {
        _ => {
            let empty: map<string, unknown> = {};
            empty
        }
    }
}

function find_request<E>(j: Journal<E>, call_id: string) -> ToolRequested? {
    let found: ToolRequested? = null;
    for (let en in j.entries) {
        if let t: ToolRequested = en.event {
            if (t.call_id == call_id) { found = t; }
        }
    }
    found
}

function last_assistant<E>(j: Journal<E>) -> string {
    let msg = "";
    for (let en in j.entries) {
        if let a: AssistantMessage = en.event { msg = a.content; }
    }
    msg
}

//# Session factory. args_json lands in SessionStarted — args are session
//# constants, recorded in the journal (see 02_sessions.md).
function new_session<T, X>(
    function_name: string,
    args_json: string,
    client: Client<Ev = Event | X>,
    toolbox: Toolbox,
    policy: Policy<Ev = Event | X>,
    output_schema: string = "",
) -> Session<T, X> {
    let j = new_journal<Event | X>();
    j.append(SessionStarted { function_name: function_name, args_json: args_json });
    Session {
        journal: j,
        toolbox: toolbox,
        registry: [],
        client: client,
        policy: policy,
        st: policy.init(),
        queued: [],
        queued_events: [],
        interrupts: [],
        cancel: baml.spawn.CancelToken.new(),
        output_schema: output_schema,
    }
}

//# The refold: rebuild policy state by replaying the journal through
//# update. Three rules make replay equal the live run:
//#   1. commands are discarded — their effects already happened;
//#   2. recorded entries are skipped — they were never folded live
//#      (and record() is suppressed, so replay cannot re-append them);
//#   3. throws are swallowed per event — the abort already happened live.
function refold<X>(p: Policy<Ev = Event | X>, j: Journal<Event | X>) -> SessionState {
    let st = p.init();
    j.replaying = true;
    defer { j.replaying = false; }
    for (let en in j.entries) {
        if (!en.recorded) {
            let _ = (p.update(st, j, en.event) catch_all (e) { _ => [] });
        }
    }
    st
}

//# Resume: the journal is the state. Policy state is rebuilt by re-folding.
function resume_session<T, X>(
    snap: string,
    client: Client<Ev = Event | X>,
    toolbox: Toolbox,
    policy: Policy<Ev = Event | X>,
    output_schema: string = "",
) -> Session<T, X> {
    let j = resume_journal<Event | X>(snap);
    let st = refold(policy, j);
    Session {
        journal: j,
        toolbox: toolbox,
        registry: [],
        client: client,
        policy: policy,
        st: st,
        queued: [],
        queued_events: [],
        interrupts: [],
        cancel: baml.spawn.CancelToken.new(),
        output_schema: output_schema,
    }
}
