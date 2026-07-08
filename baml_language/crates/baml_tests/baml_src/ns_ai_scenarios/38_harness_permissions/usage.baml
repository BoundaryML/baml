// scenario 38 — permissions & sandbox (+ 39 extensibility, 40 sessions, 41
// deployment): ../../../../../llm-provider/ideas/scenarios/38-permissions-sandbox/README.md
// BLOCKED: P8 — subprocess transport; config variance is the testable
// surface today (the harness IS its config).

function locked_down() -> baml.ai.ClaudeCode throws never {
    baml.ai.ClaudeCode {
        model: null,
        sandbox: "read-only",
        permission_mode: "ask",
        allowed_tools: ["read"],
    }
}

function permissive() -> baml.ai.ClaudeCode throws never {
    baml.ai.ClaudeCode {
        model: "opus",
        sandbox: "danger-full-access",
        permission_mode: "auto",
        allowed_tools: ["read", "edit", "bash"],
    }
}

test "38: sandbox + permission posture is provider config" {
    let l = locked_down();
    assert.equal(l.sandbox, "read-only")
    assert.equal(l.permission_mode, "ask")
}

test "39: the tool surface is extensible per instance (allowed_tools)" {
    assert.equal(locked_down().allowed_tools.length(), 1)
    assert.equal(permissive().allowed_tools.length(), 3)
}

test "41: deployment variance is config variance — both are drivable harnesses" {
    let a: baml.ai.Provider = locked_down();
    let b: baml.ai.Provider = permissive();
    let both = match (a) {
        let r1: baml.ai.Realtime => match (b) {
            let r2: baml.ai.Realtime => true,
            _ => false,
        },
        _ => false,
    };
    assert.is_true(both)
}
