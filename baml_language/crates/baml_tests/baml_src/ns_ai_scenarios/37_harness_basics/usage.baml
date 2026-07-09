// scenario 37 — what a harness is + driving it: ../../../../../llm-provider/ideas/scenarios/37-harness-basics/README.md
//   usage.baml — APPLICATION-author code + runnable tests, on the REAL `baml.ai` surface.
//   (Also folds scenario 42 "harness abstraction": ONE driver over many runtimes.)
//
// The design's thesis: a harness is a `Provider`, so the `client -> function ... ->
// Provider` sugar covers it with NO special case. `sandbox`/`permission_mode`/`model` are
// bound config FIELDS; the live control channel is handed in at the `.live` call, never a
// client field. A harness has NO `.call` (provider-as-marker): the "fresh session, run to
// completion" one-shot is `Realtime.run` over a throwaway channel, and a STEERABLE session
// is the driver loop below.
//
// What ships (REALIZED.md §37): `baml.ai.ClaudeCode` / `baml.ai.PiAgent` EXIST as marker
// `Provider` + `Realtime` classes with bound config fields — but subprocess transport is
// P8-pending, so their `run` throws a typed `Unsupported`. That typed Unsupported is the
// honest current truth (tested below). The design's DRIVER FLOW runs offline against the
// scripted stand-in `s37_ScriptedHarness` (implementation.baml).
//
// The offline tier proves, on the real surface:
//   (1) CONFIG-construction variance — real ClaudeCode/PiAgent + scripted, statically + dynamically.
//   (2) CAPABILITY NEGOTIATION — every harness is `Realtime` (drivable); a chat model is not (42).
//   (3) the divergent CONTROL PLANES — Claude honors RewindFiles; Pi does not (supported_verbs).
//   (4) the GUARD — an unsupported control verb is rejected BEFORE the subprocess.
//   (5) the ONE-SHOT `.live` over a throwaway channel — RUNS against the scripted harness.
//   (6) the typed `Unsupported` from the REAL ClaudeCode/PiAgent transport (P8 BLOCKED).
//   (7) the FULL DRIVER — negotiate -> guard -> steer/rewind -> run -> transcript.
//   (8) HOST-OWNED tool dispatch (`run_tools`) — the scripted harness runs it; the real
//       ClaudeCode does not implement `Tools`, so it is a typed `Unsupported`.
//
// NO LIVE TIER (sanctioned): subprocess transport does not exist, so a real harness cannot
// be driven for real. See the report's `blocked`.


// ── config-only dead default; the harness client is supplied per call via `client =`. ──
client<llm> s37_Dead {
    provider "openai"
    options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

// THE ONE FUNCTION. `-> string` so the `$live` companion (drive_live) returns the session
// `Transcript`; the plain body is never called against a harness (a harness has no `call`).
function s37_Fix(task: string) -> string {
    client s37_Dead
    prompt #"Fix the bug: {{ task }}"#
}


// ── config-construction factories (the design's config-only `client` blocks) ───
// Real shipped harnesses: config is the whole surface (P8). The scripted stand-ins carry
// the design's typed config AND a runnable session.

function s37_real_claude() -> baml.ai.ClaudeCode throws never {
    baml.ai.ClaudeCode {
        model: "claude-sonnet-4-6",
        sandbox: "container",
        permission_mode: "accept-edits",
        allowed_tools: ["read", "edit", "bash", "grep", "glob"],
    }
}

function s37_real_pi() -> baml.ai.PiAgent throws never {
    baml.ai.PiAgent { endpoint: "http://127.0.0.1:9" }
}

// Dynamic selection — a plain function body, impossible with a static block (design's `Coder`).
function s37_coder(trust: string) -> baml.ai.ClaudeCode throws never {
    if (trust == "ci") {
        baml.ai.ClaudeCode {
            model: "claude-sonnet-4-6",
            sandbox: "danger-full-access",
            permission_mode: "bypass",   // no prompts in CI
            allowed_tools: [],           // all tools
        }
    } else {
        baml.ai.ClaudeCode {
            model: null,
            sandbox: "container",
            permission_mode: "accept-edits",
            allowed_tools: ["read", "edit", "bash"],
        }
    }
}

// A Claude-SHAPED scripted harness: full verb set INCLUDING RewindFiles (containerized fs
// can checkpoint). This is what the real ClaudeCode becomes once P8 lands.
function s37_scripted_claude() -> s37_ScriptedHarness throws never {
    s37_ScriptedHarness {
        model: "claude-sonnet-4-6",
        sandbox: s37_Sandbox.Container,
        permission_mode: s37_PermissionMode.AcceptEdits,
        allowed_tools: ["read", "edit", "bash"],
        verbs: ["Steer", "FollowUp", "Interrupt", "SetModel", "SetPermMode", "StopTask", "EndSession", "RewindFiles"],
        reply: "patched null-deref in auth.py",
        tool_calls: [baml.ai.ToolCall { id: "c1", name: "edit", args: "{\"path\": \"auth.py\"}" }],
        final_json: "{\"files_changed\": [\"auth.py\"], \"summary\": \"fixed\"}",
    }
}

// A Pi-SHAPED scripted harness: NARROWER control plane — steer + followUp + end, NO rewind,
// NO interrupt (design §5: same interfaces, smaller verb set proves the seam is portable).
function s37_scripted_pi() -> s37_ScriptedHarness throws never {
    s37_ScriptedHarness {
        model: "anthropic/claude-sonnet-4-6",
        sandbox: s37_Sandbox.Local,
        permission_mode: s37_PermissionMode.Default,
        allowed_tools: ["read", "edit"],
        verbs: ["Steer", "FollowUp", "EndSession"],
        reply: "pi finished the task",
        tool_calls: [],
        final_json: "{\"files_changed\": [], \"summary\": \"pi done\"}",
    }
}


// ── app helpers: negotiate the marker + the verb set ───────────────────────────

// (42) ONE abstraction over many runtimes: any Realtime-capable provider is a drivable
// harness; a chat model is not.
function s37_drivable(p: baml.ai.Provider) -> string throws never {
    match (p) {
        let r: baml.ai.Realtime => "drivable harness",
        _ => "not a harness",
    }
}

// The advertised control plane (empty when the provider advertises none).
function s37_verbs_of(p: baml.ai.Provider) -> string[] throws never {
    match (p) {
        let cp: s37_ControlPlane => cp.supported_verbs(),
        _ => [],
    }
}

function s37_honors(p: baml.ai.Provider, verb: string) -> bool throws never {
    s37_verbs_of(p).includes(verb)
}


// ── the FULL DRIVER (design's DriveBuild) — negotiate, guard, steer, rewind, run ──
// The app author's OWN code (not generated): the control plane is MESSAGES over the
// handed-in channel, plus capability negotiation so the guard only lets through verbs the
// chosen harness backs. Returns the durable transcript.
function s37_drive_build(spec: string, raw_io: baml.ai.Channel, p: baml.ai.Provider) -> baml.ai.Transcript
    throws baml.errors.RealtimeError | baml.errors.UnknownError {
    let picked = p;
    match (picked) {
        let r: baml.ai.Realtime => {
            // Negotiate: wrap the host channel so unsupported verbs are rejected locally.
            let io = s37_negotiate(raw_io, r);
            // Wire the event stream coming UP from the session.
            io.on((ev: string) -> void {
                log.info("37 driver up-event: " + ev);
            });
            // Drive it — each is a MESSAGE, not a method on the client. Steer/FollowUp/
            // SetPermMode are honored by every harness; RewindFiles only by one that
            // advertised it (a Pi-shaped harness rejects it AT THE GUARD, landed = false).
            let steered = s37_try_send(io, s37_Steer { text: "Use the existing logger, don't add a new one." });
            let followed = s37_try_send(io, s37_FollowUp { text: "When green, open a PR." });
            let tightened = s37_try_send(io, s37_SetPermMode { mode: s37_PermissionMode.Default });
            let rewound = s37_try_send(io, s37_RewindFiles { to_user_message_id: "msg_1", dry_run: false });
            let rewind_label = if (rewound) { "rewind-honored" } else { "rewind-rejected-at-guard" };
            log.info("37 driver: steer=" + steered.to_string() + " rewind=" + rewind_label);
            let ended = s37_try_send(io, s37_EndSession { drain: true });
            // `run` drives the duplex to settle, then returns the opaque transcript.
            r.run(spec, io)
        },
        _ => throw baml.errors.Unsupported { message: "provider is not a steerable harness" },
    }
}


// ── host-owned tool dispatch (design's BuildWithMyTools): the app owns the policy ──
// A no-op dispatcher that echoes each requested tool back as done (a real app gates it —
// scenario 38). Drives the harness's OWN `Tools` loop via `run_tools`.
function s37_echo_dispatch() -> (baml.ai.ToolCall[]) -> baml.ai.ToolResult[] throws never {
    (calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] {
        let out: baml.ai.ToolResult[] = [];
        for (let c in calls) {
            out.push(baml.ai.ToolResult { id: c.id, output: "did: " + c.name });
        }
        out
    }
}

class s37_Patch {
    files_changed: string[],
    summary: string,
}

// Runs the harness's host-owned tool loop; a provider without `Tools` is a typed
// Unsupported. Negotiates `Tools` on the existential marker, then drives `run_tools`
// (which speaks the string prompt form directly).
function s37_build_with_my_tools(spec: string, tools: baml.ai.Tool[], p: baml.ai.Provider) -> s37_Patch
    throws baml.errors.ToolError | baml.errors.UnknownError {
    let picked = p;
    match (picked) {
        let tp: baml.ai.Tools => {
            let outcome = tp.run_tools<s37_Patch>(spec, tools, s37_echo_dispatch());
            match (outcome) {
                let patch: s37_Patch => patch,
                let b: baml.ai.Budget<s37_Patch> => s37_Patch { files_changed: [], summary: "budget" },
                let h: baml.ai.HandoffRequest => s37_Patch { files_changed: [], summary: "handoff" },
            }
        },
        _ => throw baml.errors.Unsupported { message: "provider cannot host-dispatch tools" },
    }
}


// ============================================================================
// Offline tier — deterministic, no network, no keys.
// ============================================================================

// (1) CONFIG-construction variance: static, dynamic, and the scripted typed config.
test "37: a harness is config; construction varies statically and dynamically" {
    let claude = s37_real_claude();
    assert.equal(claude.sandbox, "container")
    assert.equal(claude.allowed_tools.length(), 5)
    // dynamic selection — impossible with a static block
    assert.equal(s37_coder("ci").permission_mode, "bypass")
    assert.equal(s37_coder("dev").permission_mode, "accept-edits")
    // the scripted stand-in carries the design's TYPED config
    let scripted = s37_scripted_claude();
    log.info("37: scripted sandbox = " + s37_sandbox_label(scripted.sandbox));
    assert.equal(scripted.allowed_tools.length(), 3)
}

// (2) CAPABILITY NEGOTIATION (42): every harness is a drivable `Realtime`; a chat model
// is not. The SAME `s37_drivable` abstracts over real ClaudeCode, real PiAgent, and the
// scripted stand-in — one driver, many runtimes.
test "42: ClaudeCode, PiAgent, and the scripted harness all negotiate as drivable" {
    assert.equal(s37_drivable(s37_real_claude()), "drivable harness")
    assert.equal(s37_drivable(s37_real_pi()), "drivable harness")
    assert.equal(s37_drivable(s37_scripted_claude()), "drivable harness")
    // a chat model is NOT a harness (no Realtime).
    assert.equal(s37_drivable(baml.ai.OpenAi { model: "m", api_key: "k", base_url: null }), "not a harness")
}

// (3) DIVERGENT CONTROL PLANES: same interfaces, different advertised verb sets. Claude
// honors RewindFiles; Pi does not (design §5). Read as data, negotiated at runtime.
test "37: harnesses advertise divergent control planes (Claude rewinds, Pi does not)" {
    let claude: baml.ai.Provider = s37_scripted_claude();
    let pi: baml.ai.Provider = s37_scripted_pi();
    log.info("37: claude verbs = " + s37_verbs_of(claude).join(",")
        + " | pi verbs = " + s37_verbs_of(pi).join(","));
    assert.is_true(s37_honors(claude, "RewindFiles"))
    assert.is_true(s37_honors(claude, "Steer"))
    assert.is_true(!s37_honors(pi, "RewindFiles"))   // Pi has no rewind
    assert.is_true(!s37_honors(pi, "Interrupt"))     // ...and no interrupt
    assert.is_true(s37_honors(pi, "Steer"))          // ...but steer + followUp + end
}

// (4) THE GUARD: an unsupported control verb is rejected BEFORE the subprocess. Sending
// RewindFiles through a Pi-negotiated guard throws Io locally; through a Claude-negotiated
// guard it lands. A session UP-event frame always passes (not a control verb).
test "37: GuardedChannel rejects an unsupported control verb before the harness" {
    let rec = s37_RecordingChannel { events: [] };
    let pi_guard = s37_negotiate(rec, s37_scripted_pi());
    // RewindFiles is NOT in Pi's verb set -> rejected at the guard (landed = false).
    let rewind_landed = s37_try_send(pi_guard, s37_RewindFiles { to_user_message_id: "msg_1", dry_run: false });
    // Steer IS in Pi's verb set -> passes.
    let steer_landed = s37_try_send(pi_guard, s37_Steer { text: "hi" });
    // A session up-event (not a control verb) always passes.
    pi_guard.send("TextDelta:hello") catch (e) { _ => null };
    log.info("37: pi-guard rewind_landed=" + rewind_landed.to_string() + " steer_landed=" + steer_landed.to_string()
        + " recorded=" + rec.events.join("|"));
    assert.is_true(!rewind_landed)
    assert.is_true(steer_landed)
    assert.is_true(rec.events.join("|").includes("Steer:hi"))
    assert.is_true(!rec.events.join("|").includes("RewindFiles"))   // never reached the channel
}

// (5) THE ONE-SHOT: `.live` over a throwaway channel — RUNS against the scripted harness.
// A harness has NO `.call`; the fresh-session-to-completion one-shot is `Realtime.run`
// over a channel we never steer. The `$live` companion (drive_live) negotiates Realtime.
test "37: the one-shot .live drives a fresh scripted session to a transcript" {
    let ch = s37_RecordingChannel { events: [] };
    let t = s37_Fix$live("auth.py null deref", ch, client = s37_scripted_claude()) catch (e) {
        _ => baml.ai.Transcript { text: "LIVE FAILED" },
    };
    log.info("37: one-shot transcript = " + t.text + " | frames = " + ch.events.length().to_string());
    assert.is_true(t.text.includes("patched null-deref"))    // the session's reply, folded in
    assert.is_true(t.text.includes("auth.py null deref"))    // seeded with the call-time prompt
    assert.is_true(ch.events.length() > 0)                   // the duplex was driven over the pass-in io
}

// (6) THE REAL TRANSPORT IS P8-BLOCKED: driving the shipped ClaudeCode/PiAgent yields a
// typed `Unsupported` — the honest current truth. Existential binding so the interface arm
// is a real runtime test (a concrete-typed match would make it irrefutable — gotchas).
test "37: driving the REAL ClaudeCode transport is a typed Unsupported (P8 pending)" {
    let h: baml.ai.Provider = s37_real_claude();
    let label = match (h) {
        let r: baml.ai.Realtime => {
            r.run("fix the bug", NullDeadChannel {}) catch (e) {
                let u: baml.errors.Unsupported => "unsupported",
                let re: baml.errors.RealtimeError => "other-realtime-error",
                _ => "wrong-channel",
            }
        },
        _ => "not-drivable",
    };
    log.info("37: real ClaudeCode.run -> " + label);
    assert.equal(label, "unsupported")
}

test "37: driving the REAL PiAgent transport is also a typed Unsupported (P8 pending)" {
    let h: baml.ai.Provider = s37_real_pi();
    let label = match (h) {
        let r: baml.ai.Realtime => r.run("do it", NullDeadChannel {}) catch (e) {
            let u: baml.errors.Unsupported => "unsupported",
            _ => "other",
        },
        _ => "not-drivable",
    };
    assert.equal(label, "unsupported")
}

// (7) THE FULL DRIVER against the scripted harness: negotiate -> guard -> steer/rewind ->
// run. A Claude-shaped harness lets RewindFiles land (recorded before the harness's own
// up-events); a Pi-shaped harness rejects it at the guard, yet the session still runs.
test "37: the full driver steers a Claude-shaped session and lands a rewind" {
    let rec = s37_RecordingChannel { events: [] };
    let t = s37_drive_build("Implement the spec, then run the tests", rec, s37_scripted_claude()) catch (e) {
        _ => baml.ai.Transcript { text: "DRIVE FAILED" },
    };
    let frames = rec.events.join("|");
    log.info("37: claude driver transcript = " + t.text + " | frames = " + frames);
    assert.is_true(t.text.includes("patched null-deref"))
    assert.is_true(frames.includes("Steer:Use the existing logger"))
    assert.is_true(frames.includes("RewindFiles:msg_1"))     // honored -> reached the channel
    assert.is_true(frames.includes("TextDelta:"))             // the session then drove its own up-events
}

test "37: the full driver runs a Pi-shaped session but its rewind never reaches the wire" {
    let rec = s37_RecordingChannel { events: [] };
    let t = s37_drive_build("ship it", rec, s37_scripted_pi()) catch (e) {
        _ => baml.ai.Transcript { text: "DRIVE FAILED" },
    };
    let frames = rec.events.join("|");
    log.info("37: pi driver transcript = " + t.text + " | frames = " + frames);
    assert.is_true(t.text.includes("pi finished the task"))  // the session still ran
    assert.is_true(frames.includes("Steer:"))                // steer landed
    assert.is_true(!frames.includes("RewindFiles"))          // rejected at the guard
}

// (8) HOST-OWNED tool dispatch: the app owns the loop policy via `run_tools`. The scripted
// harness implements `Tools` and runs it to a value; the REAL ClaudeCode does NOT implement
// `Tools` (only Realtime), so the SAME call is a typed `Unsupported`.
test "37: host-owned tool dispatch runs on the scripted harness" {
    let tools = [baml.ai.Tool { name: "edit", description: "edit a file", parameters: "{}", params_type: null, handoff: null }];
    let patch = s37_build_with_my_tools("fix the bug", tools, s37_scripted_claude()) catch (e) {
        _ => s37_Patch { files_changed: [], summary: "TOOLS FAILED" },
    };
    log.info("37: host-dispatch patch summary = " + patch.summary);
    assert.equal(patch.summary, "fixed")
    assert.equal(patch.files_changed.length(), 1)
}

test "37: the REAL ClaudeCode does not host-dispatch tools -> typed Unsupported" {
    let tools = [baml.ai.Tool { name: "edit", description: "edit", parameters: "{}", params_type: null, handoff: null }];
    let label = s37_build_with_my_tools("fix", tools, s37_real_claude()) catch (e) {
        let u: baml.errors.Unsupported => "unsupported",
        let te: baml.errors.ToolError => "other-tool-error",
        _ => "wrong",
    };
    match (label) {
        let s: s37_Patch => assert.is_true(false),   // should have thrown
        let msg: string => assert.equal(msg, "unsupported"),
    }
}


// ── shared helpers / offline channel stand-ins ─────────────────────────────────

function s37_sandbox_label(s: s37_Sandbox) -> string throws never {
    match (s) {
        s37_Sandbox.Local => "local",
        s37_Sandbox.Container => "container",
        s37_Sandbox.Virtual => "virtual",
    }
}

/// A recording `Channel`: captures every frame the driver / session sends (the app gets
/// one from its host I/O and hands it to `.live`; offline we record strings).
class s37_RecordingChannel {
    events: string[],

    implements baml.ai.Channel {
        function on(self, handler: (string) -> void) -> null throws never {
            null
        }
        function send(self, data: string) -> null throws baml.errors.Io {
            self.events.push(data);
            null
        }
        function close(self) -> null throws never {
            null
        }
    }
}

/// A do-nothing throwaway channel (the one-shot's io param; also reused by scenario 38's
/// P8-marker `run`). Kept for cross-scenario reuse.
class NullDeadChannel {
    implements baml.ai.Channel {
        function on(self, handler: (string) -> void) -> null throws never {
            null
        }
        function send(self, data: string) -> null throws baml.errors.Io {
            null
        }
        function close(self) -> null throws never {
            null
        }
    }
}
