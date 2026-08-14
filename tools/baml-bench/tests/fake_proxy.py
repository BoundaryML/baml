"""A stub claude-proxy (plus a stub Cursor agents endpoint) for integration tests.

Returns canned AgentResults so we can exercise the baml-worker/baml-dedup pipeline
without real LLM auth, and a fake ``POST /v1/agents`` so FixDispatch's Cursor
cloud-agent launch can be driven end-to-end without hitting the real Cursor API.
"""

from __future__ import annotations

import json

from fastapi import FastAPI

app = FastAPI()


@app.post("/check-baml")
async def check_baml(req: dict):
    """Stub the baml check: pretend any supplied repro reproduces (errors).

    Args:
        req: The check request (only ``check_id`` is echoed back).

    Returns:
        A canned check result with a non-zero exit code and an alias error tail.
    """
    return {"check_id": req.get("check_id", ""), "exit_code": 4, "timed_out": False,
            "baml_on_path": True, "stdout_tail": "", "stderr_tail": "error: @alias not allowed"}


@app.post("/run-agent")
async def run_agent(req: dict):
    """Stub an agent run: return a canned trophy (worker) or issues (dedup).

    Branches on ``cell_id``: a ``baml-dedup-`` cell returns a deduplicated
    issues.json referencing the report id parsed from the prompt; any other cell
    returns a full self-reported trophy.json plus metrics and a two-turn log.

    Args:
        req: The run-agent request (uses ``cell_id`` and, for dedup, ``files``).

    Returns:
        A canned AgentResult dict with ``post_files`` for the caller to parse.
    """
    cell = req.get("cell_id", "")
    if cell.startswith("baml-dedup-"):
        import re
        reports = (req.get("files") or {}).get("reports.md", "")
        m = re.search(r"report_id:\s*(\S+)", reports)
        rid = m.group(1) if m else None
        issues = {"issues": [
            {"kind": "language", "category": "bug", "title": "enum alias rejected on parse",
             "description": "baml rejects @alias on enum values.",
             "suggestion": "allow @alias on enum values",
             "evidence": [{"report_id": rid, "call_index": 2}]},
        ]}
        return {"cell_id": cell, "status": "ok", "exit_code": 0, "transcript": "",
                "post_files": {"issues.json": json.dumps(issues)}}
    # worker run: the agent self-reports the whole trophy as trophy.json
    turn_log = [
        {"i": 1, "text_preview": "writing baml", "tools": [
            {"name": "Write", "input": {"path": "x.baml"}, "result_preview": "ok", "is_error": False}]},
        {"i": 2, "text_preview": "running fmt", "tools": [
            {"name": "Bash", "input": {"command": "baml fmt"},
             "result_preview": "error: @alias not allowed", "is_error": True}]},
    ]
    trophy = {
        "report_md": "## What worked\n- defined the function\n\n## What didn't work / bugs\n"
                     "- enum variant alias rejected by compiler\n\n## Suggestions for BAML team\n"
                     "- allow @alias on enum values",
        "task_completed": True,
        "summary": "Agent compiled a BAML function but hit an enum-parsing snag.",
        "what_went_well": ["defined the function", "used the skill"],
        "what_failed": ["enum variant alias rejected by compiler"],
        "issues": [
            {"kind": "language", "title": "enum alias rejected on parse",
             "description": "baml fmt rejected `@alias` on an enum value.", "call_index": 2,
             "suggestion": "allow @alias on enum values"},
        ],
        "suggestions": [
            {"target": "language", "suggestion": "support @alias on enum values",
             "rationale": "agents expect aliasing to work on enums like on classes"},
        ],
    }
    return {"cell_id": cell, "status": "ok", "exit_code": 0,
            "transcript": json.dumps({"num_turns": 2, "session_id": "s"}),
            "turns": 2, "tool_calls": 2, "api_calls": 2,
            "input_tokens": 1000, "output_tokens": 500, "total_tokens": 1500,
            "wall_clock_ms": 4200, "estimated_cost_usd": 0.01,
            "turn_log": turn_log,
            "post_files": {"x.baml": "function F(){}", "trophy.json": json.dumps(trophy)}}


@app.post("/v1/agents")
async def launch_agent(req: dict):
    """Stub the Cursor Cloud Agents launch endpoint.

    Stands in for ``POST {CURSOR_API_BASE}/v1/agents`` so FixDispatch's
    ``cursor_client.launch_agent`` call resolves end-to-end without the real API.

    Args:
        req: The launch body (prompt/repos/autoCreatePR); ignored by the stub.

    Returns:
        A minimal created-agent object with an ``id`` and ``url``.
    """
    return {"id": "agent-test-1", "url": "https://cursor.example/agents/agent-test-1"}
