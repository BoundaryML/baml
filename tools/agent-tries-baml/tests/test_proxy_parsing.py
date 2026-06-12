import json

from services.claude_proxy import runner


def test_parse_claude_session():
    """parse_claude_session extracts the session summary from the agent's stdout.

    Verifies it pulls the turn and tool-call counts, the session id, and the token
    usage (including the cache-write count) out of the final JSON line, ignoring
    surrounding log noise.
    """
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
    """parse_turn_log builds 1-indexed turns and backfills tool results from the log.

    Verifies turns are numbered from 1 (matching the worker's anchors), api calls
    are counted, each tool_use is paired with its tool_result preview and error
    flag, and the thinking preview is captured.
    """
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
    """validate_relative_path rejects parent-traversal and absolute paths.

    Verifies that a "../" traversal and an absolute path each raise ValueError,
    while an ordinary relative path is accepted.
    """
    import pytest
    with pytest.raises(ValueError):
        runner.validate_relative_path("../etc/passwd")
    with pytest.raises(ValueError):
        runner.validate_relative_path("/abs")
    runner.validate_relative_path("baml_src/foo.baml")  # ok


def test_install_skills_runs_baml_agent_install(tmp_path):
    """_install_skills runs `baml agent install --dir .` with the cached baml on PATH.

    Verifies the happy path (a stub baml that records its argv and exits 0 yields
    True), the failure path (exit 1 yields False), and the no-binary path (None
    baml_dir yields False without running anything) — all without failing the run.
    """
    import asyncio
    import os
    import stat

    from services.claude_proxy.app import _install_skills

    staging = tmp_path / "staging"
    staging.mkdir()
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    argv_log = tmp_path / "argv.log"
    baml = bin_dir / "baml"
    baml.write_text(f'#!/bin/sh\necho "$@" >> {argv_log}\nexit 0\n')
    baml.chmod(baml.stat().st_mode | stat.S_IXUSR)

    assert asyncio.run(_install_skills(staging, bin_dir, "cell-1")) is True
    assert argv_log.read_text().strip() == "agent install --dir ."

    baml.write_text("#!/bin/sh\nexit 1\n")
    assert asyncio.run(_install_skills(staging, bin_dir, "cell-1")) is False

    assert asyncio.run(_install_skills(staging, None, "cell-1")) is False
    assert not (staging / "anything").exists()


def test_collect_post_files_excludes_agent_config_trees(tmp_path):
    """collect_post_files skips .claude/ and .agents/ (installed skills, not artifacts).

    Verifies an installed skill's SKILL.md is not collected even though its
    basename matches a `*.md` pattern, while project files still are.
    """
    (tmp_path / ".claude" / "skills" / "baml-core").mkdir(parents=True)
    (tmp_path / ".claude" / "skills" / "baml-core" / "SKILL.md").write_text("skill")
    (tmp_path / ".agents" / "skills" / "baml-core").mkdir(parents=True)
    (tmp_path / ".agents" / "skills" / "baml-core" / "SKILL.md").write_text("skill")
    (tmp_path / "notes.md").write_text("notes")

    out = runner.collect_post_files(tmp_path, ["*.md"], 1024, 4096)
    assert out == {"notes.md": "notes"}
