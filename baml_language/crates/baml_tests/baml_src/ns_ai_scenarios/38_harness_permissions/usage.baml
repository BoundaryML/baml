// scenario 38 — permissions & sandboxing: ../../../../../llm-provider/ideas/scenarios/38-permissions-sandbox/usage.baml
//   usage.baml — APPLICATION-author code + runnable tests.
//
// The app bounds a code-fixing harness three ways, all on the REAL surface:
//   * the SANDBOX posture + permission MODE + tool list are bound CONFIG on the
//     shipped `baml.ai.ClaudeCode` (asserted; sandbox enforcement is P8-pending);
//   * the running enforcement is the PERMISSION MODE, applied at the loop's
//     `dispatch` seam (s38_permissioned_dispatch) — read-only / ask / full-auto;
//   * the "ask" decision rides the approval CALLBACK seam handed in per run.
//
// PREFER LLM FUNCTIONS: the agent task rides as the CLIENT of a plain LLM function
// (`s38_FixTask`), driven with a `client = baml.ai.ToolLoop { ... }` override whose
// `inner` is the scripted harness fake and whose `dispatch` is the permissioned
// gate — the design-true shape (the loop is implied by the provider, the policy by
// the dispatch). The default client is dead, so every offline test passes an
// explicit deterministic provider. NO live tier (harness subprocess transport is
// P8-blocked — the sandbox fence cannot be proven in the command path yet).
//
// Reuses (read-only, shared namespace): scenario 37's NullDeadChannel.


// ── The app's output schema ────────────────────────────────────────────────────
class s38_FixResult {
    summary: string,
}


// ── Config-variance: the harness IS its config (real baml.ai.ClaudeCode) ───────
//
// sandbox / permission_mode / allowed_tools are BOUND FIELDS, exactly like model.
// The three permission modes the design names are the `permission_mode` values;
// the sandbox posture is a sibling field (its enforcement is P8-pending).

function s38_read_only() -> baml.ai.ClaudeCode throws never {
    baml.ai.ClaudeCode {
        model: "claude-sonnet-4-6",
        sandbox: "read-only",
        permission_mode: "read-only",
        allowed_tools: ["read_file"],
    }
}
function s38_ask() -> baml.ai.ClaudeCode throws never {
    baml.ai.ClaudeCode {
        model: "claude-sonnet-4-6",
        sandbox: "workspace-write",
        permission_mode: "ask",
        allowed_tools: ["read_file", "write_file"],
    }
}
function s38_full_auto() -> baml.ai.ClaudeCode throws never {
    baml.ai.ClaudeCode {
        model: "opus",
        sandbox: "danger-full-access",
        permission_mode: "full-auto",
        allowed_tools: ["read_file", "write_file", "bash"],
    }
}


// ── Clients + the LLM function the app calls ───────────────────────────────────

// Offline default: a dead client, so every test passes an explicit provider override.
client<llm> s38_Dead {
    provider "openai"
    options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

/// The agent task. Driven with a ToolLoop client whose inner is the scripted
/// harness and whose dispatch is the permissioned gate.
function s38_FixTask(task: string) -> string {
    client s38_Dead
    prompt #"Fix the task, using tools as needed: {{ task }}"#
}


// ── Tool registry + scripted calls ─────────────────────────────────────────────

class ReadArgs  { path: string }
class WriteArgs { path: string, contents: string }

function s38_code_tools() -> baml.ai.Tool[] {
    [
        baml.ai.Tool { name: "read_file",  description: "read a file",  parameters: "{}" },
        baml.ai.Tool { name: "write_file", description: "write a file", parameters: "{}" },
    ]
}

function s38_read_call() -> baml.ai.ToolCall throws never {
    baml.ai.ToolCall { id: "r1", name: "read_file", args: "{\"path\":\"src/main.rs\"}" }
}
function s38_write_call() -> baml.ai.ToolCall throws never {
    baml.ai.ToolCall { id: "w1", name: "write_file", args: "{\"path\":\"src/main.rs\",\"contents\":\"fixed\"}" }
}


// ── Direct-gate helper: report the outcome of one call under one mode ───────────

function s38_gate_status(
    call: baml.ai.ToolCall,
    mode: string,
    approve: ((baml.ai.ToolCall) -> bool throws never)?,
) -> string throws never {
    let results = s38_gate([call], mode, approve) catch (e) {
        let d: s38_PermissionDenied => { return "denied:" + d.tool; },
    };
    (results[0]).output
}


// ── (1) The permission modes run offline through the gate ──────────────────────

test "38: read-only mode permits a read tool" {
    assert.equal(s38_gate_status(s38_read_call(), "read-only", null), "read ok: read_file")
}

test "38: read-only mode denies a write tool (typed PermissionDenied)" {
    assert.equal(s38_gate_status(s38_write_call(), "read-only", null), "denied:write_file")
}

test "38: ask mode escalates a write to the approval callback — approved runs it" {
    assert.is_true(s38_gate_status(s38_write_call(), "ask", s38_always_approve()).includes("wrote (approved)"))
}

test "38: ask mode with a denying callback refuses the write" {
    assert.equal(s38_gate_status(s38_write_call(), "ask", s38_never_approve()), "denied:write_file")
}

test "38: ask mode with NO callback (unattended) defaults to deny" {
    assert.equal(s38_gate_status(s38_write_call(), "ask", null), "denied:write_file")
}

test "38: full-auto mode permits both read and write" {
    assert.equal(s38_gate_status(s38_read_call(), "full-auto", null), "read ok: read_file")
    assert.is_true(s38_gate_status(s38_write_call(), "full-auto", null).includes("wrote"))
}


// ── (2) The harness's OWN permission_mode drives the running enforcement ───────
//
// The shipped ClaudeCode's posture field is pulled straight into the gate: the
// design's config IS the running policy, no separate wiring.

test "38: the harness's declared permission_mode drives the dispatch" {
    let ro = s38_read_only();
    let auto = s38_full_auto();
    assert.equal(s38_gate_status(s38_write_call(), ro.permission_mode, null), "denied:write_file")
    assert.is_true(s38_gate_status(s38_write_call(), auto.permission_mode, null).includes("wrote"))
}


// ── (3) The refusal answers the Failure axis (policy refusal, not retryable) ────

function s38_gate_failure_axis(call: baml.ai.ToolCall, mode: string) -> string throws never {
    s38_gate([call], mode, null) catch (e) {
        let f: baml.errors.Failure => {
            return if (f.is_policy_refusal() && !f.is_retryable()) { "policy-refusal" } else { "misclassified" };
        },
    };
    "not-refused"
}

test "38: a denied write is a non-retryable policy refusal on the Failure axis" {
    assert.equal(s38_gate_failure_axis(s38_write_call(), "read-only"), "policy-refusal")
}


// ── (4) End-to-end: the LLM function rides the ToolLoop with the harness fake ───
//
// The design-true shape: a plain LLM function whose CLIENT is a ToolLoop over the
// scripted harness, with the permissioned dispatch as its policy. A forbidden
// write comes back as a DENIED result (data, not a crash), the harness sees it and
// finishes — so the loop completes even under read-only.

test "38: read-only ToolLoop run encodes a forbidden write as a DENIED result, then finishes" {
    let r = s38_FixTask("edit the config file", client = baml.ai.ToolLoop {
        inner: s38_HarnessAgent {
            scripted_calls: [s38_write_call()],
            final_json: "could not write under read-only; reported instead",
        },
        tools: s38_code_tools(),
        dispatch: s38_permissioned_dispatch("read-only", null),
        stop_when: null,
    }) catch (e) {
        _ => "loop failed",
    };
    assert.equal(r, "could not write under read-only; reported instead")
}

test "38: full-auto ToolLoop run lets the harness read then write, then finishes" {
    let r = s38_FixTask("read then fix", client = baml.ai.ToolLoop {
        inner: s38_HarnessAgent {
            scripted_calls: [s38_read_call(), s38_write_call()],
            final_json: "applied the fix",
        },
        tools: s38_code_tools(),
        dispatch: s38_permissioned_dispatch("full-auto", null),
        stop_when: null,
    }) catch (e) {
        _ => "loop failed",
    };
    assert.equal(r, "applied the fix")
}

test "38: the permissioned dispatch encodes a read-only write as a DENIED result" {
    let d = s38_permissioned_dispatch("read-only", null);
    let results = d([s38_write_call()]);
    assert.is_true((results[0]).output.includes("DENIED"))
}


// ── (5) Config posture asserted (the real classes' bound fields) ───────────────

test "38: sandbox + permission posture is bound provider config" {
    let l = s38_read_only();
    assert.equal(l.sandbox, "read-only")
    assert.equal(l.permission_mode, "read-only")
    assert.equal(l.allowed_tools.length(), 1)

    let p = s38_full_auto();
    assert.equal(p.sandbox, "danger-full-access")
    assert.equal(p.allowed_tools.length(), 3)
}


// ── (6) P8 marker: the real sandbox fence needs subprocess transport ───────────
//
// The shipped ClaudeCode negotiates as a Realtime harness (the capability is
// present) but its subprocess transport — the ONLY thing that could actually run
// Bash inside a real sandbox and enforce the posture — is P8-pending, so `run`
// throws Unsupported. Offline, the config is asserted and the mode-enforcement
// runs through the gate; the real fence graduates when the host surface lands.

test "38: real sandbox enforcement is P8-pending — ClaudeCode.run is a typed Unsupported" {
    let h: baml.ai.Provider = s38_read_only();
    let outcome = match (h) {
        let r: baml.ai.Realtime => {
            r.run("apply the fix", NullDeadChannel {}) catch (e) {
                let u: baml.errors.Unsupported => "unsupported (P8 pending)",
                _ => "wrong error",
            }
        },
        _ => "not a harness",
    };
    assert.equal(outcome, "unsupported (P8 pending)")
}
