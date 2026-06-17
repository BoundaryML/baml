"""Hard-task generator: an agent that invents fresh, varied BAML benchmark tasks.

Instead of the worker rotating a small static list, this runs a Claude agent each
cron cycle to produce new algorithm-implementation tasks (a self-contained spec per
task), seeded with the recently-run prompts so it doesn't repeat them. Structurally
a sibling of baml_dedup / cohort_compare: render context -> run an agent -> parse the
posted JSON. The caller (services/cron) enqueues the returned prompts as `tasks`.
"""

from __future__ import annotations

import json
import logging
import os
import time
from typing import Any

from bench_core.jsonl import extract_last_json_object
from bench_core.schemas import RunAgentRequest

log = logging.getLogger("cron.generator")

GEN_MODEL = os.environ.get("GEN_TASK_MODEL", "claude-sonnet-4-6")
GEN_MAX_TURNS = int(os.environ.get("GEN_TASK_MAX_TURNS", "4"))
GEN_TIMEOUT_SECS = int(os.environ.get("GEN_TASK_TIMEOUT_SECS", "300"))

SYSTEM_PROMPT = """You invent HARD benchmark tasks for an autonomous agent that writes BAML.

BAML is a typed language: alongside LLM functions it has plain `function`s with expression \
bodies, classes with methods, a standard library (Array / String / Map, math), recursion, and \
control flow. These tasks exercise that general-purpose side — NOT prompt engineering.

Each task is a self-contained spec to implement a real ALGORITHM as a BAML `function`. A great task:
- names an exact signature, e.g. `function edit_distance(a: string, b: string) -> int`;
- specifies the behavior, the algorithm/method to use, and the edge cases precisely;
- requires genuine logic — dynamic programming, recursion, graph traversal, parsing, number \
theory, combinatorics, geometry, simulation, or a data structure (heap / trie / union-find / …);
- lists 2-3 concrete BAML test expectations (input -> exact output) the author MUST include;
- is solvable in pure BAML with no external IO, and encodes any structured input as a string the \
function parses (e.g. "u,v,w" edge triples), since inputs are scalars/strings.

Vary the domain widely across tasks and keep the depth of the two examples below. Do NOT reuse \
the examples, and do NOT produce anything similar to the prompts in `recent_tasks.md`.

# Style examples (do not reuse these)
1. Implement `function count_n_queens(n: int) -> int`: the number of distinct N-Queens solutions \
via backtracking that places one queen per row and prunes on column and both diagonals. Tests: \
n=1 -> 1; n=4 -> 2; n=8 -> 92.
2. Implement `function shortest_distance(n: int, edges: string, source: int, target: int) -> int`: \
Dijkstra over a weighted directed graph whose `edges` is a space-separated list of "u,v,w" triples; \
return the shortest distance or -1 if unreachable. Tests: a direct edge wins when shortest; a \
cheaper two-hop beats a pricier direct edge; unreachable -> -1.

# Output
Write a file `tasks.json` (working directory) with EXACTLY this shape and N tasks:
{ "tasks": [ "<full self-contained task prompt, one rich paragraph>", ... ] }
Write ONLY valid JSON in tasks.json, nothing else."""

USER_PROMPT = (
    "Read `recent_tasks.md` (recently-run tasks to avoid repeating), then invent {n} fresh, "
    "varied, hard tasks across different algorithmic domains and write them to `tasks.json`."
)


def _render_recent(prompts: list[str]) -> str:
    """Render recent task prompts into the avoid-repeats context file.

    Args:
        prompts: Recently-run task prompt strings.

    Returns:
        A markdown bullet list (truncated per prompt), or a placeholder when empty.
    """
    if not prompts:
        return "(no recent tasks yet)"
    return "# Recently-run tasks — do NOT repeat these or close variants\n\n" + "\n".join(
        f"- {p[:300]}" for p in prompts[:40]
    )


def _parse_tasks(result) -> list[str]:
    """Extract the task list the agent produced from its run result.

    Prefers the posted ``tasks.json`` file; falls back to the last JSON object in
    the transcript when the file is missing or unparseable.

    Args:
        result: The agent run result carrying post_files and transcript.

    Returns:
        The list of task prompt strings, or an empty list when none can be parsed.
    """
    raw = result.post_files.get("tasks.json")
    data: Any = None
    if raw:
        try:
            data = json.loads(raw)
        except json.JSONDecodeError:
            data = None
    if data is None:
        data = extract_last_json_object(result.transcript or "")
    if isinstance(data, dict):
        return data.get("tasks", []) or []
    return []


async def generate_hard_tasks(proxy, recent_prompts: list[str], count: int) -> list[str]:
    """Run the generator agent and return up to ``count`` fresh task prompts.

    Args:
        proxy: A ProxyClient used to run the agent.
        recent_prompts: Recently-run task prompts the agent should avoid repeating.
        count: How many new tasks to request.

    Returns:
        Clean, non-empty task prompt strings (at most ``count``); empty on failure,
        so the caller can fall back to its static list.
    """
    req = RunAgentRequest(
        cell_id=f"task-gen-{int(time.time())}",
        model=GEN_MODEL,
        max_turns=GEN_MAX_TURNS,
        prompt=USER_PROMPT.format(n=count),
        system_prompt=SYSTEM_PROMPT,
        files={"recent_tasks.md": _render_recent(recent_prompts)},
        post_file_patterns=["tasks.json"],
        max_file_bytes=256 * 1024,
        invocation_timeout_secs=GEN_TIMEOUT_SECS,
    )
    result = await proxy.run_agent(req, timeout=GEN_TIMEOUT_SECS + 120)
    tasks = [t.strip() for t in _parse_tasks(result) if isinstance(t, str) and t.strip()]
    log.info("task-generator produced %d task(s)", len(tasks))
    return tasks[:count]
