import json

from services.claude_proxy import runner


def test_parse_claude_session():
    stdout = "\n".join([
        "some log noise",
        json.dumps({
            "num_turns": 2, "num_tool_calls": 3, "session_id": "sess-1",
            "usage": {"input_tokens": 100, "output_tokens": 50, "total_tokens": 150,
                      "cache_read_input_tokens": 10, "cache_creation_input_tokens": 5},
        }),
    ])
    s = runner.parse_claude_session(stdout)
    assert s["turns"] == 2 and s["tool_calls"] == 3 and s["session_id"] == "sess-1"
    assert s["input_tokens"] == 100 and s["cache_write_tokens"] == 5


def test_parse_turn_log_indexes_and_backfills():
    lines = [
        {"type": "assistant", "message": {"content": [
            {"type": "thinking", "thinking": "let me think"},
            {"type": "text", "text": "I'll run baml"},
            {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "baml fmt"}},
        ]}},
        {"type": "user", "message": {"content": [
            {"type": "tool_result", "tool_use_id": "t1", "content": "error: bad", "is_error": True},
        ]}},
        {"type": "assistant", "message": {"content": [
            {"type": "tool_use", "id": "t2", "name": "Write", "input": {"path": "x.baml"}},
        ]}},
        {"type": "user", "message": {"content": [
            {"type": "tool_result", "tool_use_id": "t2",
             "content": [{"type": "text", "text": "ok"}], "is_error": False},
        ]}},
    ]
    jsonl = "\n".join(json.dumps(x) for x in lines)
    turns, api_calls = runner.parse_turn_log(jsonl)
    assert api_calls == 2
    assert [t["i"] for t in turns] == [1, 2]            # 1-based, matches worker anchors
    t1 = turns[0]["tools"][0]
    assert t1["name"] == "Bash" and t1["is_error"] is True and t1["result_preview"] == "error: bad"
    t2 = turns[1]["tools"][0]
    assert t2["name"] == "Write" and t2["result_preview"] == "ok" and t2["is_error"] is False
    assert turns[0]["thinking_preview"] == "let me think"


def test_validate_relative_path_rejects_traversal():
    import pytest
    with pytest.raises(ValueError):
        runner.validate_relative_path("../etc/passwd")
    with pytest.raises(ValueError):
        runner.validate_relative_path("/abs")
    runner.validate_relative_path("baml_src/foo.baml")  # ok
