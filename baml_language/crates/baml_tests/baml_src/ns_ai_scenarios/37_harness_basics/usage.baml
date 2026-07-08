// scenario 37 — harness basics: ../../../../../llm-provider/ideas/scenarios/37-harness-basics/README.md
// (also 42 harness abstraction). BLOCKED: P8 — the harness subprocess
// transport is stubbed (`ClaudeCode`/`PiAgent.run` throw Unsupported), so the
// offline tier covers configuration + negotiation; live driving graduates
// when the host surface lands.

// 42 — one abstraction over many runtimes: any Realtime-capable provider is
// a drivable harness.
function drive_any(h: baml.ai.Provider) -> string throws never {
    match (h) {
        let r: baml.ai.Realtime => "drivable harness",
        _ => "not a harness",
    }
}

function sample_claude_code() -> baml.ai.ClaudeCode {
    baml.ai.ClaudeCode {
        model: null,
        sandbox: "workspace-write",
        permission_mode: "auto",
        allowed_tools: ["read", "edit"],
    }
}

test "42: ClaudeCode and PiAgent both negotiate as drivable harnesses" {
    assert.equal(drive_any(sample_claude_code()), "drivable harness")
    assert.equal(drive_any(baml.ai.PiAgent { endpoint: "http://127.0.0.1:9" }), "drivable harness")
    assert.equal(drive_any(baml.ai.OpenAi { model: "m", api_key: "k", base_url: null }), "not a harness")
}

test "37: driving the stubbed harness transport is a typed Unsupported (P8 pending)" {
    // Existential binding: a match on the CONCRETE class would make the
    // interface arm irrefutable (gotchas).
    let h: baml.ai.Provider = sample_claude_code();
    let outcome = match (h) {
        let r: baml.ai.Realtime => {
            r.run("fix the bug", NullDeadChannel {}) catch (e) {
                let u: baml.errors.Unsupported => baml.ai.Transcript { text: "unsupported" },
                _ => baml.ai.Transcript { text: "wrong error" },
            }
        },
        _ => baml.ai.Transcript { text: "not drivable" },
    };
    assert.equal(outcome.text, "unsupported")
}

/// Offline channel stand-in (37's io param; the stub throws before using it).
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
