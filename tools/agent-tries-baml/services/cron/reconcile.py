"""Cohort fan-in reconciler — the skill-arena barrier.

A cohort fans out into N member tasks (same prompt, different baml-skill branch).
The cohort can only be compared once every member is terminal. This sweep derives
that readiness from member state and flips the cohort ``pending -> queued`` so the
CohortCompare processor can claim it.

It lives here (an independent periodic sweep), NOT in the worker's completion path,
on purpose: if the LAST member crashes and is reaped to ``failed`` by the Convex
lease-reaper, no worker code runs afterward — a worker-driven check would strand the
cohort forever. A sweep re-derives readiness regardless of how a member became
terminal, so it survives crashes the same way the lease-reaper does. It uses only
generic table verbs, so it behaves identically against the real Convex backend and
the in-memory test backend.
"""

from __future__ import annotations

import logging

log = logging.getLogger("cohort-reconcile")

# A member task counts as terminal (done OR failed) for the fan-in barrier, so a
# member the reaper exhausted to "failed" can never deadlock its cohort.
TERMINAL_TASK_STATUSES = frozenset({"done", "failed"})


async def reconcile_cohorts_once(service) -> int:
    """Advance every pending cohort whose member tasks are all terminal.

    Lists cohorts still ``pending``, and for each one whose member tasks have all
    reached a terminal status (or no longer exist), transitions it to ``queued`` so
    CohortCompare can claim it. A cohort whose ``memberTaskIds`` is not yet populated
    (ingress hasn't finished fanning out) is left for the next sweep.

    Args:
        service: The ServiceClient bound to the api.

    Returns:
        The number of cohorts advanced ``pending -> queued`` this sweep.
    """
    pending = await service.list(
        "cohorts", field="status", value="pending",
        index="by_status_created", limit=100,
    )
    advanced = 0
    for cohort in pending:
        member_ids = cohort.get("memberTaskIds") or []
        if not member_ids:
            continue  # not yet fanned out; reconcile on a later sweep
        all_terminal = True
        for task_id in member_ids:
            task = await service.get("tasks", task_id)
            # A missing task (deleted) counts as terminal so it can't deadlock; an
            # in-flight one (queued/running) blocks the cohort.
            if task is not None and task.get("status") not in TERMINAL_TASK_STATUSES:
                all_terminal = False
                break
        if all_terminal:
            await service.transition("cohorts", cohort["_id"], "queued")
            advanced += 1
            log.info("cohort %s ready -> queued (%d members)", cohort["_id"], len(member_ids))
    return advanced
