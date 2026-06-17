"""Tiny in-container process supervisor for the consolidated `bammy-service`.

Runs all the pipeline processes — the combined web app plus every claim-loop /
sweep — as child `python -m services.<name>` subprocesses inside one machine,
restarting any that exit (capped exponential backoff) and forwarding SIGTERM /
SIGINT so a Fly machine stop drains cleanly.

The process set comes from the ``SUPERVISE`` env var: a comma-separated list of
``services.<name>`` module names, where an entry may be suffixed ``:N`` to run N
copies (e.g. ``baml_worker:2``). Claim-based processors (worker, dedup, redraft,
cohort_compare, changelog_worker, notion_fixer) are safe to run as N copies — the
Convex lease gives each row to one claimant. Singletons (web, bug_verify, cron)
must stay at 1.

Example:
    SUPERVISE="web,baml_worker:2,baml_dedup,baml_redraft,bug_verify,\
notion_fixer,cohort_compare,changelog_worker,baml_builder,cron"
"""

from __future__ import annotations

import logging
import os
import signal
import subprocess
import sys
import time
from typing import Optional

log = logging.getLogger("supervisor")

# Min seconds between a child's start and its exit for the restart backoff to
# reset — a child that survives this long is considered "healthy".
_HEALTHY_SECS = 30.0
_BACKOFF_CAP = 30.0


class Child:
    """One supervised subprocess and its restart bookkeeping."""

    def __init__(self, name: str, index: int):
        """Build a child spec for ``python -m services.<name>``.

        Args:
            name: The service module name (e.g. ``baml_worker``).
            index: Replica index, for log labels when N>1.
        """
        self.name = name
        self.label = f"{name}#{index}"
        self.proc: Optional[subprocess.Popen] = None
        self.backoff = 1.0
        self.started_at = 0.0

    def start(self) -> None:
        """Spawn (or respawn) the subprocess, inheriting the full environment."""
        self.proc = subprocess.Popen([sys.executable, "-m", f"services.{self.name}"],
                                     env=os.environ.copy())
        self.started_at = time.monotonic()
        log.info("supervisor: started %s (pid=%s)", self.label, self.proc.pid)


def _parse_specs(raw: str) -> list[Child]:
    """Expand the ``SUPERVISE`` string into Child instances (honoring ``name:N``)."""
    out: list[Child] = []
    for entry in (e.strip() for e in raw.split(",")):
        if not entry:
            continue
        name, _, count = entry.partition(":")
        n = int(count) if count else 1
        for i in range(n):
            out.append(Child(name.strip(), i))
    return out


def main() -> None:
    """Run the supervise loop until a termination signal arrives.

    Starts every child, then polls; a child that has exited is restarted with
    exponential backoff (reset once it has run ``_HEALTHY_SECS``). On SIGTERM /
    SIGINT every child is terminated and the supervisor exits 0.
    """
    logging.basicConfig(level=os.environ.get("LOG_LEVEL", "INFO"))
    specs = os.environ.get("SUPERVISE", "").strip()
    if not specs:
        log.error("supervisor: SUPERVISE is empty; nothing to run")
        sys.exit(1)
    children = _parse_specs(specs)
    log.info("supervisor: managing %d processes: %s",
             len(children), ", ".join(c.label for c in children))

    stopping = {"flag": False}

    def _on_signal(signum, _frame):
        log.info("supervisor: signal %s; stopping children", signum)
        stopping["flag"] = True

    signal.signal(signal.SIGTERM, _on_signal)
    signal.signal(signal.SIGINT, _on_signal)

    for c in children:
        c.start()

    try:
        while not stopping["flag"]:
            for c in children:
                if c.proc is None:
                    continue
                code = c.proc.poll()
                if code is None:
                    continue
                ran = time.monotonic() - c.started_at
                if ran >= _HEALTHY_SECS:
                    c.backoff = 1.0  # it stayed up; treat as a fresh crash
                log.warning("supervisor: %s exited (code=%s) after %.0fs; restart in %.0fs",
                            c.label, code, ran, c.backoff)
                time.sleep(c.backoff)
                c.backoff = min(_BACKOFF_CAP, c.backoff * 2)
                if not stopping["flag"]:
                    c.start()
            time.sleep(1.0)
    finally:
        for c in children:
            if c.proc and c.proc.poll() is None:
                c.proc.terminate()
        deadline = time.monotonic() + 10
        for c in children:
            if c.proc:
                try:
                    c.proc.wait(timeout=max(0.1, deadline - time.monotonic()))
                except Exception:  # noqa: BLE001
                    c.proc.kill()
    log.info("supervisor: exited")


if __name__ == "__main__":
    main()
