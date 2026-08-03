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
    queued_events: (Event | X)[],// data lane: custom events
    cancel: baml.spawn.CancelToken,
    output_schema: string,

    // ---- data lane ----
    function send(self, msg: string) -> void { self.queued.push(msg); }
    function send_event(self, e: Event | X) -> void { self.queued_events.push(e); }

    // ---- control lane ----
    function interrupt(self, reason: string) -> void {
        self.cancel.cancel();
        self.queued_events.push(Interrupted { reason: reason });
    }

    // ---- configuration (the alternative to a `with` clause) ----
    function set_client(self, c: Client<Ev = Event | X>) -> void { self.client = c; }
    function set_policy(self, p: Policy<Ev = Event | X>) -> void {
        self.policy = p;
        self.st = p.init();
    }
    function register(self, t: Tool) -> void { self.registry.push(t); }

    function snapshot(self) -> string { self.journal.snapshot() }

    function mount(self, names: string[]) -> void {
        for (let n in names) {
            if let t: Tool = self.registry.find((r) -> { r.name == n }) {
                self.toolbox.entries.push(t);
            }
        }
        self.journal.append(ToolsChanged { names: self.toolbox.names() });
    }

    function unmount(self, names: string[]) -> void {
        self.toolbox.entries = self.toolbox.entries.filter((t) -> { !names.includes(t.name) });
        self.journal.append(ToolsChanged { names: self.toolbox.names() });
    }

    // The runner: append event -> ask policy -> execute commands -> repeat.
    function run(self) -> Done<T> | Replied {
        let inbox: (Event | X)[] = [];
        let cmds: Command[] = [];

        //# turn boundary: drain both queues
        while let m: string = self.queued.shift() {
            inbox.push(UserMessage { content: m });
        }
        while let qe: Event | X = self.queued_events.shift() {
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
                        let args = parse_args(c.args_json);
                        inbox.push(ToolCompleted {
                            call_id: c.call_id,
                            result_json: self.toolbox.call(c.tool, args),
                        });
                    },
                    let c: RetryTool => {
                        // sketch: no timer; re-run from the journaled request
                        if let orig: ToolRequested = find_request(self.journal, c.call_id) {
                            inbox.push(ToolCompleted {
                                call_id: orig.call_id,
                                result_json: self.toolbox.call(orig.tool, parse_args(orig.args_json)),
                            });
                        }
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
                        self.cancel.cancel();
                        throw SessionInterrupted { reason: c.reason };
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
        cancel: baml.spawn.CancelToken.new(),
        output_schema: output_schema,
    }
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
    let st = policy.init();
    //# rebuild the derived cache from history (commands are discarded)
    for (let en in j.entries) {
        let _ = policy.update(st, j, en.event);
    }
    Session {
        journal: j,
        toolbox: toolbox,
        registry: [],
        client: client,
        policy: policy,
        st: st,
        queued: [],
        queued_events: [],
        cancel: baml.spawn.CancelToken.new(),
        output_schema: output_schema,
    }
}
