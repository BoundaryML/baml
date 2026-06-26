// Unit tests for the terminal transcript renderer (core/transcript.baml).

testset "transcript" {
    test "renders assistant text, tool calls, and tool results" {
        let jsonl = #"
            {"type":"assistant","timestamp":"2026-06-25T21:00:00.000Z","message":{"content":[{"type":"text","text":"Listing files"},{"type":"tool_use","name":"Bash","input":{"command":"ls -la"}}]}}
            {"type":"user","timestamp":"2026-06-25T21:00:01.000Z","message":{"content":[{"type":"tool_result","content":"file1\nfile2","is_error":false}]}}
        "#;
        let r = render_terminal_transcript(jsonl);
        assert.is_true(r.includes("⏺ Listing files"));
        assert.is_true(r.includes("⏺ Bash(ls -la)"));
        assert.is_true(r.includes("⎿"));
        assert.is_true(r.includes("file1"));
        assert.is_true(r.includes("[21:00:00]"));
    }

    test "renders thinking + a Read tool by file_path" {
        let jsonl = #"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"I should read the file"},{"type":"tool_use","name":"Read","input":{"file_path":"/tmp/x.baml"}}]}}"#;
        let r = render_terminal_transcript(jsonl);
        assert.is_true(r.includes("Thinking"));
        assert.is_true(r.includes("I should read the file"));
        assert.is_true(r.includes("⏺ Read(/tmp/x.baml)"));
    }

    test "renders a string-content user prompt as > " {
        let jsonl = #"{"type":"user","message":{"content":"build me a thing"}}"#;
        let r = render_terminal_transcript(jsonl);
        assert.is_true(r.includes("> build me a thing"));
    }

    test "tool_result error is tagged" {
        let jsonl = #"{"type":"user","message":{"content":[{"type":"tool_result","content":"boom","is_error":true}]}}"#;
        assert.is_true(render_terminal_transcript(jsonl).includes("[error] boom"));
    }

    test "parse_turn_log builds a turn with back-filled tool result" {
        let jsonl = #"
            {"type":"assistant","timestamp":"2026-06-25T21:00:00.000Z","message":{"content":[{"type":"thinking","thinking":"thinking hard"},{"type":"text","text":"on it"},{"type":"tool_use","id":"tu_1","name":"Bash","input":{"command":"ls -la"}}]}}
            {"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"tu_1","content":"file1\nfile2","is_error":false}]}}
        "#;
        let turns = parse_turn_log(jsonl);
        assert.equal(turns.length(), 1);
        let t = match (turns.at(0)) {
            let v: TurnRow => v,
            null => {
                TurnRow {
                    i: 0,
                    ts: null,
                    thinking_chars: null,
                    thinking_preview: null,
                    text_chars: null,
                    text_preview: null,
                    tools: [],
                }
            },
        };
        assert.equal(t.i, 1);
        assert
            .equal(
                match (t.text_preview) {
                    let v: string => v,
                    null => "",
                },
                "on it",
            );
        assert
            .equal(
                match (t.thinking_preview) {
                    let v: string => v,
                    null => "",
                },
                "thinking hard",
            );
        assert.equal(t.tools.length(), 1);
        let tool = match (t.tools.at(0)) {
            let v: TurnTool => v,
            null => {
                TurnTool {
                    name: null,
                    input: null,
                    result_preview: null,
                    result_chars: null,
                    is_error: null,
                }
            },
        };
        assert
            .equal(
                match (tool.name) {
                    let v: string => v,
                    null => "",
                },
                "Bash",
            );
        assert
            .equal(
                match (tool.result_preview) {
                    let v: string => v,
                    null => "",
                },
                "file1\nfile2",
            );
    }

    test "session_log_path encodes the staging dir" {
        // HOME varies by machine; assert the encoded suffix (slashes -> dashes).
        assert
            .is_true(
                session_log_path("/tmp/staging/cell9", "sid-123")
                    .includes("/.claude/projects/-tmp-staging-cell9/sid-123.jsonl"),
            );
    }
}
