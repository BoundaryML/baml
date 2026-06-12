"""Cron: daily, refresh the nightly baml build, then enqueue curated task(s).

A best-effort backstop — each agent run now also polls for the latest nightly
itself, so this daily refresh is no longer the only path that advances the
pinned build. A simple sleep loop (one cycle on start, then every
CRON_INTERVAL_SECS). For production you may instead drive this from a Convex
scheduled function.
"""

from __future__ import annotations

import asyncio
import logging
import os
import time

from bench_core.service_client import ServiceClient

from .reconcile import reconcile_cohorts_once

log = logging.getLogger("cron")

INTERVAL = int(os.environ.get("CRON_INTERVAL_SECS", "86400"))
# The cohort fan-in barrier runs on its own fast cadence (independent of the daily
# task cycle): it flips skill-arena cohorts pending -> queued once their member runs
# are all terminal, so a finished arena is compared within ~this many seconds.
COHORT_RECONCILE_SECS = int(os.environ.get("COHORT_RECONCILE_SECS", "30"))
# Changelog poller (the absorbed baml-changelog2 cron): every cycle, list the
# repo's releases and enqueue a queued changelogEntries row for any version we
# don't have yet. Disabled by default so it can't race the old cron until the
# Phase 2 cutover flips it on.
CHANGELOG_POLL_ENABLED = os.environ.get("CHANGELOG_POLL_ENABLED", "0") == "1"
CHANGELOG_POLL_SECS = int(os.environ.get("CHANGELOG_POLL_SECS", "600"))
# Channel where cron run results are posted (the worker acks + posts the trophy
# there, since cron tasks have no originating thread).
SLACK_RESULTS_CHANNEL = os.environ.get("SLACK_RESULTS_CHANNEL", "")

# Hard-tier BAML *language* tasks: implement real algorithms as BAML `function`s,
# not structured-extraction prompts. The cron rotates through them by day so each
# run picks a different one and all get covered.
HARD_TASKS = [
    "Implement, in BAML, `function edit_distance(a: string, b: string) -> int`: the "
    "Levenshtein edit distance between two strings. Return the minimum number of "
    "single-character insertions, deletions, or substitutions needed to turn `a` into "
    "`b`. Build the standard (len(a)+1) x (len(b)+1) dynamic-programming table where "
    "cell (i, j) is the edit distance between the first i chars of `a` and the first j "
    "chars of `b`: the first row/column count pure insertions/deletions, and each "
    "interior cell is min(delete = above + 1, insert = left + 1, substitute = diagonal "
    "+ (0 if a[i-1] == b[j-1] else 1)). Return the bottom-right cell. Include BAML "
    "tests: two equal strings return 0; \"kitten\" to \"sitting\" returns 3; when one "
    "string is empty the result is the other string's length.",

    "Implement, in BAML, `function count_n_queens(n: int) -> int`: the number of "
    "distinct solutions to the N-Queens puzzle. Count the ways to place `n` queens on "
    "an n x n board so that no two share a row, column, or diagonal. Use backtracking "
    "that places exactly one queen per row, tracking the used columns and both diagonal "
    "directions (col, row + col, and row - col) to prune, and increments a counter each "
    "time all n rows are filled. Return the total count (n = 0 returns 1). Include BAML "
    "tests: n = 1 returns 1; n = 4 returns 2; n = 8 returns 92.",

    "Implement, in BAML, `function huffman_cost(freqs: string) -> int`: the total "
    "number of bits in an optimal Huffman encoding. Input is a space-separated list of "
    "positive integer symbol frequencies (e.g. \"5 9 12 13 16 45\"). Repeatedly remove "
    "the two smallest frequencies, push their sum back into the pool, and add that sum "
    "to a running total until a single node remains; the accumulated total equals the "
    "sum over all symbols of frequency times code length (the optimal encoded length). "
    "Return that total (a single frequency returns 0, since no bits are needed to "
    "distinguish one symbol). Include BAML tests: \"5 9 12 13 16 45\" returns 224; \"1 "
    "1\" returns 2; a single \"7\" returns 0.",

    "Implement, in BAML, `function shortest_distance(n: int, edges: string, source: "
    "int, target: int) -> int`: single-source shortest path on a weighted directed "
    "graph. Nodes are labeled 0..n-1; `edges` is a space-separated list of \"u,v,w\" "
    "triples meaning a directed edge u to v with non-negative integer weight w. Run "
    "Dijkstra from `source`: keep tentative distances (source = 0, all others "
    "infinity), repeatedly settle the unsettled node with the smallest tentative "
    "distance and relax its outgoing edges. Return the shortest distance from `source` "
    "to `target`, or -1 if `target` is unreachable. Include BAML tests: a direct edge "
    "is used when it is shortest; a cheaper two-hop path beats a more expensive direct "
    "edge; an unreachable target returns -1.",

    "Implement, in BAML, `function prime_factorization(n: int) -> string`: the prime "
    "factorization of an integer n >= 2. Return the prime factors in ascending order, "
    "with multiplicity, as a space-separated string (e.g. n = 12 returns \"2 2 3\"). "
    "Divide out each candidate factor d starting at 2 while d * d <= n, appending d "
    "each time it divides n; if the value remaining after the loop is greater than 1 it "
    "is itself prime and is appended last. Include BAML tests: a prime like 13 returns "
    "\"13\"; 12 returns \"2 2 3\"; 360 returns \"2 2 2 3 3 5\".",
]


def _tasks() -> list[str]:
    """Return the prompt(s) to enqueue this cycle.

    Uses the ``||``-separated ``CRON_TASKS`` override when set; otherwise rotates
    through ``HARD_TASKS`` by day so each run picks a different one and all get
    covered over time.

    Returns:
        A list of task prompt strings (a single-element list in the default
        rotating case).
    """
    raw = os.environ.get("CRON_TASKS", "")
    if raw.strip():
        return [t.strip() for t in raw.split("||") if t.strip()]
    # rotate by day so each cron run picks a different hard task, cycling all
    idx = int(time.time() // 86400) % len(HARD_TASKS)
    return [HARD_TASKS[idx]]


async def _cycle(service: ServiceClient) -> None:
    """Run one cron cycle: refresh both baml channels, then enqueue the day's task(s).

    Refreshes the nightly and canary channels (each best-effort — a failure is
    logged, not raised, so task enqueue still proceeds), keeping both buckets warm
    and pruned. Each enqueued task is tagged ``source=cron`` and, when configured,
    routed to ``SLACK_RESULTS_CHANNEL``.

    Args:
        service: The ServiceClient used to trigger the baml update and create tasks.
    """
    for channel in ("nightly", "canary"):
        try:
            upd = await service.baml_update(channel)
            log.info("baml update (%s): %s", channel, upd)
        except Exception:  # noqa: BLE001
            log.exception("baml update (%s) failed", channel)
    for prompt in _tasks():
        doc = {"source": "cron", "prompt": prompt, "status": "queued"}
        if SLACK_RESULTS_CHANNEL:
            doc["slackChannel"] = SLACK_RESULTS_CHANNEL
        tid = await service.create("tasks", doc)
        log.info("enqueued cron hard task %s", tid)


async def _daily_loop(service: ServiceClient) -> None:
    """Run the daily cycle: one on start, then every ``INTERVAL`` seconds."""
    while True:
        await _cycle(service)
        await asyncio.sleep(INTERVAL)


async def _reconcile_loop(service: ServiceClient) -> None:
    """Sweep the cohort fan-in barrier every ``COHORT_RECONCILE_SECS`` seconds.

    Best-effort: a transient sweep failure is logged and retried on the next tick
    rather than killing the loop.

    Args:
        service: The ServiceClient used to read cohorts and flip ready ones.
    """
    while True:
        try:
            await reconcile_cohorts_once(service)
        except Exception:  # noqa: BLE001
            log.exception("cohort reconcile sweep failed")
        await asyncio.sleep(COHORT_RECONCILE_SECS)


async def _changelog_poll_once(service: ServiceClient) -> None:
    """Enqueue changelog generation for any release tag without an entry.

    Thin wrapper over bench_core.changelog_sync (shared with the on-demand
    ``POST /entries/update`` ingress endpoint); the changelog worker does the
    generation, idempotency is by version.

    Args:
        service: The ServiceClient used to list entries and create rows.
    """
    from bench_core.changelog_sync import sync_missing_entries

    enqueued = await sync_missing_entries(service)
    for e in enqueued:
        log.info("changelog poll: enqueued %s (%s) -> %s", e["version"], e["channel"], e["id"])
    if not enqueued:
        log.info("changelog poll: up to date")


async def _changelog_poll_loop(service: ServiceClient) -> None:
    """Poll GitHub for new releases every ``CHANGELOG_POLL_SECS`` seconds.

    Best-effort: a failed cycle (GitHub rate limit, transient network) is
    logged and retried on the next tick.

    Args:
        service: The ServiceClient used by each poll cycle.
    """
    while True:
        try:
            await _changelog_poll_once(service)
        except Exception:  # noqa: BLE001
            log.exception("changelog poll cycle failed")
        await asyncio.sleep(CHANGELOG_POLL_SECS)


async def _amain() -> None:
    """Run the cron service: the daily task cycle, the cohort reconcile sweep,
    and (when enabled) the changelog release poller.

    Builds the shared ServiceClient, runs the loops concurrently, and closes the
    client on exit.
    """
    logging.basicConfig(level=os.environ.get("LOG_LEVEL", "INFO"))
    service = ServiceClient(os.environ["SERVICE_URL"], os.environ.get("ATB_SERVICE_TOKEN", ""))
    loops = [_daily_loop(service), _reconcile_loop(service)]
    if CHANGELOG_POLL_ENABLED:
        loops.append(_changelog_poll_loop(service))
    try:
        await asyncio.gather(*loops)
    finally:
        await service.aclose()


if __name__ == "__main__":
    asyncio.run(_amain())
