// scenario 38 — permissions & sandboxing: ../../../../../llm-provider/ideas/scenarios/38-permissions-sandbox/README.md
//   implementation.baml — LIBRARY / std-lib-author code.
//
// The design bounds a code-running harness across two fences:
//   * permissions — WHICH tools may run (permission modes + allow/deny lists +
//     a per-call approval callback);
//   * sandbox     — WHERE shell commands run (virtual / local / container).
//
// REAL-SURFACE MAPPING (what changed from the design's implement.baml):
//   - The design's `Sandbox` interface + `baml.errors.SandboxError` channel +
//     `ClaudeCode`/`FlueHarness` subprocess providers all need SUBPROCESS
//     TRANSPORT, which is P8-blocked (REALIZED.md #38: the harness IS its config).
//     So the real fence (sandbox posture) lives as CONFIG on the shipped
//     `baml.ai.ClaudeCode` (`sandbox` / `permission_mode` / `allowed_tools`) —
//     asserted here, its enforcement flagged P8-pending — and the *running*
//     enforcement is the PERMISSION MODE the design's per-call gate describes.
//   - The design's `PermissionedContext implements ExecutionContext` does not map:
//     the real `Tools.run_tools` / `ToolLoop` take a `dispatch: (ToolCall[]) ->
//     ToolResult[] throws never` CLOSURE (there is no `ExecutionContext`). So the
//     dispatcher becomes `s38_gate` (throws a typed refusal) + a `throws never`
//     `s38_permissioned_dispatch` factory that catches the refusal into a DENIED
//     `ToolResult` — a denied call is fed back to the model as DATA, not a crash
//     (exactly scenario 16's gate/gated_dispatch shape).
//   - `ToolCall.args` / `ToolResult.output` are `string` (JSON), so the checks are
//     pure BAML string ops — no `$rust_type`, no host shims (offline: no network).
//
// The design's THREE permission modes run offline through the gate:
//   * "read-only"  — a read tool runs; a write tool is a typed s38_PermissionDenied.
//   * "ask"        — a write tool consults the approval CALLBACK seam (allow/deny);
//                    no callback present ⇒ deny (an unattended run cannot block).
//   * "full-auto"  — read AND write both run.
// The permission_mode is pulled from the harness's OWN config (`ClaudeCode.
// permission_mode`), so the shipped class's posture field actually DRIVES the
// running enforcement — the design's semantics, on the real surface.
//
// Reuses (read-only, shared namespace): scenario 10's ScriptedTools pattern
// (`begin`/`step`/`submit` scripted Tools fake); scenario 37's NullDeadChannel
// (the P8-marker `run` param).


// ── 1. The typed permission refusal (the design's "denied" disposition) ────────
//
// A deliberate POLICY decision, not a transport failure: a concrete
// `baml.errors.ToolError` (all classifiers false) on the `Failure` axis as a
// non-retryable policy-refusal (re-driving replays the same forbidden call). The
// app catches it BY CONCRETE TYPE — a consumer matching only the interface would
// wrongly retry a denied write.

class s38_PermissionDenied {
    tool: string,
    mode: string,
    reason: string,

    implements baml.errors.Failure {
        function is_retryable(self) -> bool { false }
        function is_effectful(self) -> bool { false }
        function is_policy_refusal(self) -> bool { true }
        function is_resumable(self) -> bool { false }
        function is_unsupported(self) -> bool { false }
    }
    implements baml.errors.ToolError {
        function is_network_error(self) -> bool { false }
    }
}


// ── 2. The permission classifier — read vs write ───────────────────────────────
//
// "Where Bash runs" is the sandbox (P8); "may this tool mutate at all" is the
// permission mode. A write/edit/bash tool mutates; read/grep/glob observe.

function s38_is_write_tool(name: string) -> bool throws never {
    name == "write_file" || name == "edit" || name == "bash" || name == "write"
}


// ── 3. The approval CALLBACK seam ("ask" mode) ─────────────────────────────────
//
// The design's per-call `can_use_tool` hook, written in BAML as a closure the app
// hands in. It carries the LIVE per-call ToolCall and answers a yes/no. Two ready
// factories exercise both branches offline; a real app writes its own conditional
// policy (destructive Bash ⇒ deny, Edit ⇒ allow, …).

function s38_always_approve() -> (baml.ai.ToolCall) -> bool throws never {
    (c: baml.ai.ToolCall) -> bool { true }
}
function s38_never_approve() -> (baml.ai.ToolCall) -> bool throws never {
    (c: baml.ai.ToolCall) -> bool { false }
}


// ── 4. The gate: enforce the declared permission mode at the one chokepoint ─────
//
// Every tool call the loop dispatches passes through here. `log.info` records each
// permission decision (the observability seam). THROWS the typed refusal so a
// direct caller can catch it by concrete type; `s38_permissioned_dispatch` wraps
// this for the `throws never` ToolLoop seam.

function s38_gate(
    calls: baml.ai.ToolCall[],
    mode: string,
    approve: ((baml.ai.ToolCall) -> bool throws never)?,
) -> baml.ai.ToolResult[]
    throws s38_PermissionDenied {
    let out: baml.ai.ToolResult[] = [];
    for (let c in calls) {
        if (!s38_is_write_tool(c.name)) {
            // A read tool observes; every mode permits it.
            log.info("38 permit read tool [" + mode + "]: " + c.name);
            out.push(baml.ai.ToolResult { id: c.id, output: "read ok: " + c.name });
        } else if (mode == "full-auto") {
            log.info("38 full-auto permits write: " + c.name);
            out.push(baml.ai.ToolResult { id: c.id, output: "wrote: " + c.name });
        } else if (mode == "ask") {
            // The approval CALLBACK decides. No callback (unattended) ⇒ deny.
            let ok = match (approve) {
                let cb: (baml.ai.ToolCall) -> bool => cb(c),
                _ => false,
            };
            if (ok) {
                log.info("38 ask-mode approved write: " + c.name);
                out.push(baml.ai.ToolResult { id: c.id, output: "wrote (approved): " + c.name });
            } else {
                log.info("38 ask-mode denied write: " + c.name);
                throw s38_PermissionDenied { tool: c.name, mode: mode, reason: "operator denied" };
            }
        } else {
            // "read-only" (and any unknown/default mode): writes are refused.
            log.info("38 read-only denies write: " + c.name);
            throw s38_PermissionDenied { tool: c.name, mode: mode, reason: "read-only mode forbids writes" };
        }
    }
    out
}

/// A drop-in ToolLoop dispatcher (the `throws never` seam): runs `s38_gate` and
/// catches its typed refusal into a DENIED `ToolResult`, so a forbidden write is
/// fed back to the model as DATA and the loop continues rather than crashing.
function s38_permissioned_dispatch(
    mode: string,
    approve: ((baml.ai.ToolCall) -> bool throws never)?,
) -> (baml.ai.ToolCall[]) -> baml.ai.ToolResult[] throws never {
    (calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] {
        s38_gate(calls, mode, approve) catch (e) {
            let d: s38_PermissionDenied => {
                let r: baml.ai.ToolResult[] = [];
                for (let c in calls) {
                    r.push(baml.ai.ToolResult { id: c.id, output: "DENIED (" + d.mode + "): " + c.name });
                }
                return r;
            },
        }
    }
}


// ── 5. The scripted harness-like fake (offline Tools provider) ─────────────────
//
// Stands in for the design's subprocess ClaudeCode/FlueHarness (P8-blocked): an
// offline Tools-capable provider that emits `scripted_calls` one tool call per
// turn, then answers `final_json` (SAP-parsed into the caller's `T`). The
// PERMISSION policy is NOT in the provider — it rides in the loop's `dispatch`
// (s38_permissioned_dispatch), so the SAME harness is enforced differently per
// mode. This is the design's separation: the harness runs the loop; the gate
// bounds it.

class s38_HarnessState {
    remaining: baml.ai.ToolCall[],
    final_json: string,
}

class s38_HarnessAgent {
    scripted_calls: baml.ai.ToolCall[],
    final_json: string,

    implements baml.ai.Provider {}

    implements baml.ai.Tools {
        type Transcript = s38_HarnessState

        function begin<T>(self, prompt: string, tools: baml.ai.Tool[]) -> s38_HarnessState throws never {
            s38_HarnessState { remaining: self.scripted_calls, final_json: self.final_json }
        }
        function step<T>(self, t: s38_HarnessState) -> T | baml.ai.ToolCalls
            throws baml.errors.UnknownError {
            if (t.remaining.length() > 0) {
                let c = t.remaining[0];
                return baml.ai.ToolCalls { calls: [c] };
            }
            baml.sap.parse<T>(t.final_json) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["harness final parse failed"] },
            }
        }
        function submit(self, t: s38_HarnessState, results: baml.ai.ToolResult[]) -> s38_HarnessState throws never {
            s38_HarnessState { remaining: t.remaining.slice(1, t.remaining.length()), final_json: t.final_json }
        }
    }
}
