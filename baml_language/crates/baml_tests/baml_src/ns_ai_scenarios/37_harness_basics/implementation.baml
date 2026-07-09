// scenario 37 — what a harness is + driving it: ../../../../../llm-provider/ideas/scenarios/37-harness-basics/README.md
//   implementation.baml — LIBRARY / std-lib-author code, on the REAL `baml.ai` surface.
//
// The design's thesis (implement.baml): a coding harness (Claude Code, Pi, Flue) is the
// PAYOFF of the provider-as-MARKER model. It has NO `call` (no `HttpProvider`, no
// `build_request`) because its transport is a *subprocess*, not HTTP — it is a `Provider`
// only in the marker sense (composable, the type a `client` returns), and what it can DO
// is per-capability: its real interaction is `Realtime.run(prompt, io)` (a long-lived
// STEERABLE session the caller DRIVES), plus `Tools` so the host can own tool dispatch.
// The whole control plane — interrupt / setModel / setPermissionMode / rewindFiles / steer
// / followUp / stopTask — is a closed OutEvent/InEvent union sent as DATA over the one
// duplex channel, never a method per verb.
//
// REAL-SURFACE MAPPING (what changed from the design's implement.baml, and why):
//   - The shipped harness classes `baml.ai.ClaudeCode` / `baml.ai.PiAgent`
//     (ns_ai/providers/harness.baml) EXIST and implement `Provider` + `Realtime`
//     exactly as the design demands (marker, no `call`). But their `run` bodies
//     `throw Unsupported`: subprocess transport is host surface pending (plan P8). So
//     DRIVING a real harness live is BLOCKED — the typed `Unsupported` IS the honest
//     current truth, tested in usage.baml.
//   - To make the design's DRIVER FLOW actually RUN offline, this file adds a scripted
//     stand-in `s37_ScriptedHarness` that BEHAVES like a harness: it implements the
//     harness's capability set (`Realtime` + `Tools`) and its `run` genuinely drives the
//     pass-in `Channel` (pumps session events UP, folds a durable `Transcript`), the way
//     `s26_RealtimeTransport` does for voice. It is what the real `ClaudeCode` becomes
//     once P8 lands.
//   - The real `baml.ai.Channel` is STRING-typed (`send(data: string)`), so the design's
//     typed `OutEvent`/`InEvent` unions ride the wire as encoded strings. The control-
//     plane VOCABULARY is still modeled as a closed union of typed classes (`s37_OutEvent`
//     below) — the driver constructs typed events; `s37_encode` serializes them to the
//     one string channel. DIVERGENCE flagged in the report: design's `type[]` verb set
//     collapses to a `string[]` of verb NAMES because the channel speaks strings.
//   - The design's `ControlPlane` refinement (advertise WHICH verbs a harness honors) is
//     net-new; realized here as the user interface `s37_ControlPlane requires Realtime`
//     (verb set = `string[]`), matched at runtime like every other capability.
//   - `GuardedChannel` (reject an unsupported verb BEFORE it reaches the subprocess) is
//     the design's driver combinator, on the real `Channel` seam.
//
// BLOCKED (P8, sanctioned): no live tier. Subprocess transport does not exist, so the
// real `ClaudeCode`/`PiAgent` cannot be driven for real. Everything below is the offline
// truth: config-construction variance, capability negotiation over the harness classes,
// the typed `Unsupported` from the real transport, and the full driver flow running
// against the scripted stand-in.
//
// Reuses (read-only, shared `ns_ai_scenarios` namespace): `NullDeadChannel` (this dir's
// usage.baml) as a throwaway channel; the scripted-Tools shape from scenario 10/38.


// ── 1. Config vocabulary — the design's typed sandbox / permission enums ───────
//
// The shipped `baml.ai.ClaudeCode` carries these as raw config STRINGS (P8: the harness
// IS its config — scenario 38). The design typed them; we keep the typed vocabulary for
// the scripted stand-in so config-construction variance is expressed richly, and note the
// enum->string collapse on the real class.

enum s37_Sandbox {
    Local            // shell runs directly on this machine (cannot reliably rewind fs)
    Container        // ephemeral container, fs scoped to a workdir
    Virtual          // fully virtualized fs+net
}

enum s37_PermissionMode {
    Default          // ask on risky actions
    AcceptEdits      // auto-approve edits, ask on the rest
    Plan             // read & propose, never act
    BypassPermissions// no prompts (trusted CI only)
}


// ── 2. The control-plane vocabulary — a CLOSED union of typed OutEvents ────────
//
// The load-bearing move (design §2): a steerable session has an open-ended, evolving
// verb set. Modeling each verb as a METHOD would grow `Realtime` a method per harness
// feature; modeling them as DATA over one duplex `send` keeps the capability fixed. The
// driver constructs these typed values; `s37_encode` puts them on the string wire.

class s37_Steer       { text: string }                              // immediate mid-turn injection
class s37_FollowUp    { text: string }                              // queued: runs after this turn
class s37_Interrupt   { at_boundary: bool }                         // stop the in-flight turn safely
class s37_SetModel    { model: string? }                            // hot-swap model, keep session
class s37_SetPermMode { mode: s37_PermissionMode }                  // tighten/loosen mid-run
class s37_RewindFiles { to_user_message_id: string, dry_run: bool } // undo file edits back to a turn
class s37_StopTask    { task_id: string }                           // kill a background sub-task
class s37_EndSession  { drain: bool }                               // graceful shutdown

type s37_OutEvent =
    s37_Steer
  | s37_FollowUp
  | s37_Interrupt
  | s37_SetModel
  | s37_SetPermMode
  | s37_RewindFiles
  | s37_StopTask
  | s37_EndSession

/// The verb NAME of an OutEvent (the design's `type` identity, as a string because the
/// real `Channel` speaks strings). Also the guard's negotiation key.
function s37_verb_name(e: s37_OutEvent) -> string throws never {
    match (e) {
        let s: s37_Steer => "Steer",
        let f: s37_FollowUp => "FollowUp",
        let i: s37_Interrupt => "Interrupt",
        let m: s37_SetModel => "SetModel",
        let pm: s37_SetPermMode => "SetPermMode",
        let r: s37_RewindFiles => "RewindFiles",
        let st: s37_StopTask => "StopTask",
        let es: s37_EndSession => "EndSession",
    }
}

/// The whole control vocabulary — every verb name a harness could advertise. The guard
/// uses this to tell a CONTROL frame (must be advertised) from a session UP-event frame
/// (`TextDelta:`/`TurnDone:` — not a control verb, always passes).
function s37_all_verbs() -> string[] throws never {
    ["Steer", "FollowUp", "Interrupt", "SetModel", "SetPermMode", "RewindFiles", "StopTask", "EndSession"]
}

function s37_permmode_name(m: s37_PermissionMode) -> string throws never {
    match (m) {
        s37_PermissionMode.Default => "default",
        s37_PermissionMode.AcceptEdits => "accept-edits",
        s37_PermissionMode.Plan => "plan",
        s37_PermissionMode.BypassPermissions => "bypass",
    }
}

/// Serialize a typed OutEvent onto the one string channel (`verb:payload`).
function s37_encode(e: s37_OutEvent) -> string throws never {
    match (e) {
        let s: s37_Steer => "Steer:" + s.text,
        let f: s37_FollowUp => "FollowUp:" + f.text,
        let i: s37_Interrupt => "Interrupt:",
        let m: s37_SetModel => "SetModel:" + (m.model ?? ""),
        let pm: s37_SetPermMode => "SetPermMode:" + s37_permmode_name(pm.mode),
        let r: s37_RewindFiles => "RewindFiles:" + r.to_user_message_id,
        let st: s37_StopTask => "StopTask:" + st.task_id,
        let es: s37_EndSession => "EndSession:",
    }
}


// ── 3. Concrete harness domain errors (design §2b) ─────────────────────────────
//
// A subprocess harness has its OWN failures that are neither the answer nor a foreign
// transport error, so they are concrete typed classes. Since a harness has NO `call`,
// there is NO CallError variant: a spawn failure surfaces on the driven session
// (RealtimeError) and the host-dispatch tool loop (ToolError) — the harness's only two
// interactions. `Failure` is mandatory on every capability channel.

class s37_HarnessSpawnError {
    message: string,   // e.g. "claude binary not found", "sandbox denied"

    implements baml.errors.Failure {
        function is_retryable(self) -> bool { true }        // a spawn can be retried
        function is_effectful(self) -> bool { false }
        function is_policy_refusal(self) -> bool { false }
        function is_resumable(self) -> bool { false }
        function is_unsupported(self) -> bool { false }
    }
    implements baml.errors.RealtimeError {
        function is_network_error(self) -> bool { true }    // the subprocess bridge is the transport
    }
    implements baml.errors.ToolError {
        function is_network_error(self) -> bool { true }
    }
}

/// The harness ran but the model DECLINED the task (a 200-equivalent refusal in the
/// subprocess event log). Canonical `Refused` shape, on the realtime + tool channels.
class s37_HarnessRefused {
    category: string,
    message: string,

    implements baml.errors.Failure {
        function is_retryable(self) -> bool { false }
        function is_effectful(self) -> bool { false }
        function is_policy_refusal(self) -> bool { true }
        function is_resumable(self) -> bool { false }
        function is_unsupported(self) -> bool { false }
    }
    implements baml.errors.RealtimeError {
        function is_network_error(self) -> bool { false }
    }
    implements baml.errors.ToolError {
        function is_network_error(self) -> bool { false }
    }
}


// ── 4. NET-NEW capability: ControlPlane (design §3) ────────────────────────────
//
// A PLAIN `Realtime` provider only promises "I will drive this channel" — it says nothing
// about WHICH OutEvents it honors. A voice model and a coding harness are both `Realtime`,
// yet `RewindFiles` is nonsense for voice, and Pi honors no rewind at all. `ControlPlane`
// lets a harness ADVERTISE its verb set at runtime (so a UI greys out unbacked buttons),
// negotiated like every other capability — a property of the running provider, not a
// client field. `requires baml.ai.Realtime` so a `match` arm binds (a bare interface
// binds `never`).

interface s37_ControlPlane requires baml.ai.Realtime {
    // The design returns `type[]` via `reflect.type_of`; the real string-typed `Channel`
    // makes verb NAMES the honest identity, so this is `string[]`.
    function supported_verbs(self) -> string[] throws never
}


// ── 5. The scripted harness stand-in (what ClaudeCode BECOMES once P8 lands) ───
//
// Implements the harness capability set and — unlike the shipped `ClaudeCode.run`, which
// throws `Unsupported` — actually DRIVES the pass-in channel offline, so the design's
// one-shot + full-driver flows RUN. Config fields mirror the shipped class; `verbs` is
// the advertised control set (so ONE class expresses both a Claude-shaped full harness
// and a Pi-shaped narrow one — config-construction variance, the scenario's theme). The
// `tool_calls`/`final_json` seed the host-owned `Tools` loop.

class s37_LoopState {
    remaining: baml.ai.ToolCall[],
    final_json: string,
}

class s37_ScriptedHarness {
    model: string?,
    sandbox: s37_Sandbox,
    permission_mode: s37_PermissionMode,
    allowed_tools: string[],
    verbs: string[],                       // advertised control-plane verbs (supported_verbs)
    reply: string,                         // the line the session "produces"
    tool_calls: baml.ai.ToolCall[],        // scripted host-dispatch calls (Tools loop)
    final_json: string,                    // the Tools-loop final value (SAP into T)

    implements baml.ai.Provider {}

    // -- Realtime: the STATEFUL, driven session. The caller hands in `io`; the harness
    //    pumps events UP as string frames and folds them into the durable transcript. --
    implements baml.ai.Realtime {
        function run(
            self,
            prompt: string,
            io: baml.ai.Channel,
        ) -> baml.ai.Transcript throws baml.errors.RealtimeError | baml.errors.UnknownError {
            log.info("37 offline: scripted harness session start, prompt = " + prompt);
            // UP-events over the pass-in duplex (NOT control verbs -> pass any guard).
            io.send("TurnStart:") catch (e) { _ => null };
            io.send("TextDelta:" + self.reply) catch (e) { _ => null };
            io.send("TurnDone:msg_1") catch (e) { _ => null };
            log.info("37 offline: scripted harness session settled");
            baml.ai.Transcript { text: "[" + self.reply + "] <- " + prompt }
        }
    }

    // -- ControlPlane: advertise the verbs THIS harness honors. ------------------
    implements s37_ControlPlane {
        function supported_verbs(self) -> string[] throws never {
            self.verbs
        }
    }

    // -- Tools: let the HOST own dispatch policy. Canonical begin/step/submit; the
    //    opaque Transcript is the subprocess loop state (scenario 10/38 shape). -----
    implements baml.ai.Tools {
        type Transcript = s37_LoopState

        function begin<T>(self, prompt: string, tools: baml.ai.Tool[]) -> s37_LoopState throws never {
            s37_LoopState { remaining: self.tool_calls, final_json: self.final_json }
        }
        function step<T>(self, t: s37_LoopState) -> T | baml.ai.ToolCalls
            throws baml.errors.UnknownError {
            if (t.remaining.length() > 0) {
                return baml.ai.ToolCalls { calls: [t.remaining[0]] };
            }
            baml.sap.parse<T>(t.final_json) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["scripted harness final parse failed"] },
            }
        }
        function submit(self, t: s37_LoopState, results: baml.ai.ToolResult[]) -> s37_LoopState throws never {
            s37_LoopState { remaining: t.remaining.slice(1, t.remaining.length()), final_json: t.final_json }
        }
        // run_tools<T> inherited from Tools default.
    }
}


// ── 6. GuardedChannel — reject an unsupported verb BEFORE the subprocess (design §6) ──
//
// Wraps the host `Channel` and the negotiated verb set. A CONTROL frame whose verb is not
// advertised throws `baml.errors.Io` at the guard (never touching the harness); a session
// UP-event frame (not in the control vocabulary) passes straight through. This is the same
// interface-membership idea the capability `match` uses, applied to a closed data union.

class s37_GuardedChannel {
    inner: baml.ai.Channel,
    allowed: string[],        // from provider.supported_verbs()

    implements baml.ai.Channel {
        function on(self, handler: (string) -> void) -> null throws never {
            // Forward transparently. Wrap so the handler's inferred callback-throws is
            // absorbed into `throws never` (the inner `on` demands a never-throwing handler).
            self.inner.on((s: string) -> void {
                handler(s) catch (e) {
                    _ => null,
                }
            })
        }
        function send(self, data: string) -> null throws baml.errors.Io {
            // Which control verb (if any) does this frame carry?
            let matched = "";
            for (let v in s37_all_verbs()) {
                if (data.starts_with(v + ":")) {
                    matched = v;
                }
            }
            // A recognized control verb that was NOT advertised is rejected locally.
            if (matched != "" && !self.allowed.includes(matched)) {
                throw baml.errors.Io { message: "harness does not support control verb: " + matched };
            }
            self.inner.send(data)
        }
        function close(self) -> null throws never {
            self.inner.close()
        }
    }
}

/// Build a guarded channel by negotiating with whatever `Realtime` provider this is: if
/// it also advertises a `ControlPlane` verb set, honor it; else pass the raw channel
/// through (best-effort — let the session itself reject). `p` is typed as the existential
/// `Realtime` so the `s37_ControlPlane` arm is a real runtime test (a concrete-typed match
/// would make it irrefutable — gotchas).
function s37_negotiate(io: baml.ai.Channel, p: baml.ai.Realtime) -> baml.ai.Channel throws never {
    match (p) {
        let cp: s37_ControlPlane => {
            log.info("37: negotiated verb set = " + cp.supported_verbs().join(","));
            s37_GuardedChannel { inner: io, allowed: cp.supported_verbs() }
        },
        _ => {
            log.info("37: provider advertises no verb set -> raw channel (best-effort)");
            io
        },
    }
}

/// Try to send a typed OutEvent over a (possibly guarded) channel. Returns whether the
/// verb LANDED — a guard rejection (`Io`) becomes `false`, so a driver can react to an
/// unsupported verb instead of aborting. This is the `throws never` seam DriveBuild uses.
function s37_try_send(io: baml.ai.Channel, e: s37_OutEvent) -> bool throws never {
    io.send(s37_encode(e)) catch (err) {
        _ => {
            return false;
        },
    };
    true
}
