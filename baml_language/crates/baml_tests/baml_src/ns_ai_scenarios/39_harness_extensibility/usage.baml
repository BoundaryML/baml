// scenario 39 — harness extensibility: ../../../../../llm-provider/ideas/scenarios/39-harness-extensibility/README.md
//   usage.baml — APPLICATION-author code. End-user surface.
//
// What an app author writes to TEACH a harness new tricks, on the real surface:
//   1. register a custom in-process tool INTO the harness tool table, drive the
//      loop, and watch the dispatched result flow back (offline `S39Harness`);
//   2. `allowed_tools` shaping that surface (real `baml.ai.ClaudeCode` config
//      variance) — the tool table = allowed built-ins + registered app tools;
//   3. a typed SKILL — an LLM function returning a structured class, SAP-parsed
//      back offline via a fake (the "prefer LLM functions" directive: the skill
//      IS an LLM function);
//   4. a typed SUB-AGENT — likewise an LLM function whose structured output is
//      SAP-parsed back (a sub-agent is a named Provider; its output is a value);
//   5. typed capability NEGOTIATION — a harness advertises its extension
//      surfaces (Tools / Realtime), a bare chat model does not;
//   6. the P8 marker — the real subprocess tool-table plumbing is stubbed
//      (`ClaudeCode.run` throws Unsupported), so live driving graduates later.
//
// Fixtures reused (shared ns): implementation.baml's S39Harness / s39_tool_table
// / s39_weather_tool / S39GeoArgs / S39HarnessAnswer; common/fakes.baml's
// EchoProvider / NoCapProvider. NO live tier (sanctioned — P8).


// ── A dead default client so every offline test passes an explicit provider. ──
client<llm> DeadS39 {
    provider "openai"
    options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}


// ============================================================================
// 1. The extension ROUND-TRIP: register a custom tool, model calls it, the
//    dispatched result flows back. The app's dispatcher is the design's
//    ToolSource-in_process handler — a name -> handler router (the real
//    `baml.ai.Tool` carries no dispatch source; the app owns dispatch).
// ============================================================================

/// The app's dispatcher: the get_temperature handler runs in THIS process (the
/// design's in-process ToolHandler closure). `invoke_tool` SAP-coerces the raw
/// JSON args into `S39GeoArgs` before the handler sees them.
function s39_dispatch(calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] throws never {
    let out: baml.ai.ToolResult[] = [];
    for (let c in calls) {
        if (c.name == "get_temperature") {
            log.info("39 dispatch: in-process handler runs for '" + c.name + "'");
            out.push(baml.ai.invoke_tool<S39GeoArgs>(c, (a: S39GeoArgs) -> string {
                // a real app fetches open-meteo here; offline we compute deterministically.
                "12.5C @ " + a.latitude.to_string() + "," + a.longitude.to_string()
            }));
        } else {
            out.push(baml.ai.ToolResult { id: c.id, output: "no such tool" });
        }
    }
    out
}

test "39: a registered custom tool is surfaced, called, and its result flows back" {
    // The app teaches the harness ONE new trick: get_temperature.
    let table = s39_tool_table(["Read", "Edit"], [s39_weather_tool()]);
    let harness = S39Harness { allowed_tools: ["Read", "Edit"], app_tool: "get_temperature", turns_wanted: 1 };

    let outcome = harness.run_tools<S39HarnessAnswer>("what's the temperature in Berlin?", table, s39_dispatch) catch (e) {
        _ => S39HarnessAnswer { tool_result: "loop failed" },
    };
    let answer = match (outcome) {
        let a: S39HarnessAnswer => a.tool_result,
        let b: baml.ai.Budget<S39HarnessAnswer> => "budget hit",
        let h: baml.ai.HandoffRequest => "handoff",
    };
    // The dispatched in-process result made the full round trip back to the value.
    assert.equal(answer, "12.5C @ 52.5,13.4")
}

test "39: a tool NOT surfaced into the table can never be called (typed failure)" {
    // The app forgot to register get_temperature — the harness table has only
    // built-ins, so the scripted model's call for it is a typed loop failure.
    let table = s39_tool_table(["Read", "Edit"], []);
    let harness = S39Harness { allowed_tools: ["Read", "Edit"], app_tool: "get_temperature", turns_wanted: 1 };
    let outcome = harness.run_tools<S39HarnessAnswer>("temp?", table, s39_dispatch) catch (e) {
        let u: baml.errors.UnknownError => "not surfaced",
        _ => "wrong error",
    };
    let answer = match (outcome) {
        let s: string => s,
        _ => "reached a value (should not)",
    };
    assert.equal(answer, "not surfaced")
}


// ============================================================================
// 2. `allowed_tools` shaping the surface — REAL `baml.ai.ClaudeCode` config.
//    The surfaced table is the harness's allowed built-ins + registered app
//    tools; a locked-down vs permissive posture is pure config variance.
// ============================================================================

/// Build the surfaced tool table FROM a real `ClaudeCode`'s `allowed_tools`,
/// plus the app's registered tools. This ties the real config field to the
/// design's uniform `Tool[]` surface.
function s39_surface_for(cc: baml.ai.ClaudeCode, app_tools: baml.ai.Tool[]) -> baml.ai.Tool[] throws never {
    s39_tool_table(cc.allowed_tools, app_tools)
}

function s39_locked_agent() -> baml.ai.ClaudeCode throws never {
    baml.ai.ClaudeCode { model: null, sandbox: "read-only", permission_mode: "ask", allowed_tools: ["Read"] }
}

function s39_dev_agent() -> baml.ai.ClaudeCode throws never {
    baml.ai.ClaudeCode { model: "sonnet", sandbox: "workspace-write", permission_mode: "acceptEdits", allowed_tools: ["Read", "Edit", "Bash", "Grep"] }
}

test "39: allowed_tools shapes the surfaced tool table (config variance)" {
    // Locked-down: 1 built-in + the 1 registered app tool = 2.
    let locked = s39_surface_for(s39_locked_agent(), [s39_weather_tool()]);
    assert.equal(locked.length(), 2)
    // Dev agent: 4 built-ins + the 1 registered app tool = 5.
    let dev = s39_surface_for(s39_dev_agent(), [s39_weather_tool()]);
    assert.equal(dev.length(), 5)
    // The registered app tool is surfaced regardless of posture.
    assert.is_true(s39_table_has(dev, "get_temperature"))
}


// ============================================================================
// 3. A typed SKILL — an LLM function returning a structured class, SAP-parsed
//    back (the design's `TriageIssue.skill<Triage>(...)`). "Prefer LLM
//    functions": the skill IS an LLM function; the structured output falls out
//    of the desugar's SAP-parse of the provider reply into `Triage`.
// ============================================================================

class S39Triage {
    severity: string,
    reproducible: bool,
    summary: string,
}

function s39_TriageSkill(issue: string) -> S39Triage {
    client DeadS39
    prompt #"
        {{ _.role("system") }}
        You are the triage skill. Classify the issue.

        {{ _.role("user") }}
        {{ issue }}
    "#
}

test "39: a typed skill's structured output is SAP-parsed back" {
    let t = s39_TriageSkill(
        "login returns 500",
        client = EchoProvider { reply: "{\"severity\": \"high\", \"reproducible\": true, \"summary\": \"auth handler 500s\"}" },
    ) catch (e) {
        _ => S39Triage { severity: "err", reproducible: false, summary: "call failed" },
    };
    assert.equal(t.severity, "high")
    assert.is_true(t.reproducible)
    assert.equal(t.summary, "auth handler 500s")
}

test "39: a skill over a capability-less provider is a typed Unsupported" {
    let t = s39_TriageSkill("x", client = NoCapProvider {}) catch (e) {
        let u: baml.errors.Unsupported => S39Triage { severity: "unsupported", reproducible: false, summary: "" },
        _ => S39Triage { severity: "wrong", reproducible: false, summary: "" },
    };
    assert.equal(t.severity, "unsupported")
}


// ============================================================================
// 4. A typed SUB-AGENT — a named Provider whose structured output is SAP-parsed
//    back (the design's `Reviewer()` sub-agent returning a `Patch`). Again an
//    LLM function: the sub-agent is opaque, one prompt in, one typed value out.
// ============================================================================

class S39Patch {
    files_changed: string[],
    summary: string,
}

function s39_ReviewerSubAgent(task: string) -> S39Patch {
    client DeadS39
    prompt #"
        {{ _.role("system") }}
        You are the code-reviewer sub-agent. Return the patch you would apply.

        {{ _.role("user") }}
        {{ task }}
    "#
}

test "39: a sub-agent's structured output is SAP-parsed back to a typed value" {
    let p = s39_ReviewerSubAgent(
        "fix the login 500",
        client = EchoProvider { reply: "{\"files_changed\": [\"auth.rs\", \"session.rs\"], \"summary\": \"guard the null session\"}" },
    ) catch (e) {
        _ => S39Patch { files_changed: [], summary: "call failed" },
    };
    assert.equal(p.files_changed.length(), 2)
    assert.equal(p.summary, "guard the null session")
}


// ============================================================================
// 5. Typed capability NEGOTIATION — a harness advertises its extension surfaces;
//    a bare chat model does not. The extension surface IS the capability set.
// ============================================================================

/// Does this provider expose the tool-loop extension surface (Tools)?
function s39_is_tool_extensible(p: baml.ai.Provider) -> bool throws never {
    match (p) {
        let t: baml.ai.Tools => true,
        _ => false,
    }
}

/// Does this provider expose the steerable-session surface (Realtime)?
function s39_is_drivable(p: baml.ai.Provider) -> bool throws never {
    match (p) {
        let r: baml.ai.Realtime => true,
        _ => false,
    }
}

test "39: harness advertises its extension surfaces; a chat model does not" {
    let harness: baml.ai.Provider = S39Harness { allowed_tools: ["Read"], app_tool: "get_temperature", turns_wanted: 1 };
    let chat: baml.ai.Provider = EchoProvider { reply: "hi" };
    // The offline harness is tool-extensible; the chat model is not.
    assert.is_true(s39_is_tool_extensible(harness))
    assert.is_true(!s39_is_tool_extensible(chat))
    // The real subprocess harness advertises the steerable Realtime surface.
    let cc: baml.ai.Provider = s39_dev_agent();
    assert.is_true(s39_is_drivable(cc))
    assert.is_true(!s39_is_drivable(chat))
}


// ============================================================================
// 6. P8 marker — the REAL subprocess tool-table plumbing is pending. Driving the
//    real `ClaudeCode` harness (where the tool table would be surfaced to the
//    binary) is a typed Unsupported until the host transport lands.
// ============================================================================

/// Offline channel stand-in (the `run` io param; the stub throws before use).
class S39NullChannel {
    implements baml.ai.Channel {
        function on(self, handler: (string) -> void) -> null throws never { null }
        function send(self, data: string) -> null throws baml.errors.Io { null }
        function close(self) -> null throws never { null }
    }
}

test "39: driving the real harness's tool-table plumbing is a typed Unsupported (P8)" {
    let h: baml.ai.Provider = s39_dev_agent();
    let outcome = match (h) {
        let r: baml.ai.Realtime => {
            r.run("register tools and run", S39NullChannel {}) catch (e) {
                let u: baml.errors.Unsupported => "P8 pending",
                _ => "wrong error",
            }
        },
        _ => "not drivable",
    };
    assert.equal(outcome, "P8 pending")
}
