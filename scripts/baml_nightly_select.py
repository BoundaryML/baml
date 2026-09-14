"""Validate canary CI coverage before selecting a nightly release source."""

from __future__ import annotations

import argparse
import datetime as dt
import json
from pathlib import Path


MAX_RUN_LAG = dt.timedelta(minutes=10)


def timestamp(value: str) -> dt.datetime:
    parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        raise ValueError(f"timestamp must include a timezone: {value}")
    return parsed


def select_candidate(
    commits: list[dict], runs: list[dict], observed_at: dt.datetime
) -> tuple[str, int]:
    if not commits:
        raise ValueError("canary commits query returned no commits")

    runs_by_sha: dict[str, list[dict]] = {}
    for run in runs:
        runs_by_sha.setdefault(run["headSha"], []).append(run)

    # Canary uses one squash commit per push, so every commit should have a CI
    # run. Allow ten minutes for Actions to expose it, regardless of conclusion.
    # Check the entire ancestry window before selecting: even a response with
    # a recent run can have holes and silently select an older green commit.
    missing = []
    pending = []
    for commit in commits:
        sha = commit["sha"]
        committed_at = timestamp(commit["commit"]["committer"]["date"])
        if sha not in runs_by_sha:
            age = observed_at - committed_at
            detail = f"{sha} (committed {committed_at.isoformat()}, age {age})"
            if age > MAX_RUN_LAG:
                missing.append(detail)
            else:
                pending.append(detail)

    if missing:
        raise ValueError(
            "CI workflow runs query is stale or incomplete: missing runs for "
            f"{len(missing)} canary commit(s) older than 10 minutes; "
            "refusing to select a release source:\n" + "\n".join(missing)
        )
    for detail in pending:
        print(f"Within the 10-minute CI visibility grace period: {detail}")
    print(
        f"CI coverage validated for {len(commits)} canary commits "
        f"against {len(runs)} unfiltered runs."
    )

    # Commit ancestry determines which source is newest, not run creation time
    # or ID. For duplicate successful runs of that source, keep the newest ID.
    for commit in commits:
        successful_ids = [
            run["databaseId"]
            for run in runs_by_sha.get(commit["sha"], [])
            if run["conclusion"] == "success"
        ]
        if successful_ids:
            return commit["sha"], max(successful_ids)
    raise ValueError(
        f"no canary commit in the last {len(commits)} has a successful CI run, "
        "so nothing is releasable tonight"
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--commits", required=True, type=Path)
    parser.add_argument("--runs", required=True, type=Path)
    parser.add_argument("--observed-at", required=True, type=timestamp)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    try:
        sha, run_id = select_candidate(
            json.loads(args.commits.read_text()),
            json.loads(args.runs.read_text()),
            args.observed_at,
        )
    except (ValueError, KeyError, TypeError) as exc:
        # Keep a multiline diagnostic inside one GitHub Actions annotation.
        message = str(exc).replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
        print(f"::error::{message}")
        raise SystemExit(1) from exc

    with args.output.open("a") as output:
        output.write(f"sha={sha}\nrun_id={run_id}\n")
    print(f"Newest green canary commit: {sha} (CI run {run_id})")


if __name__ == "__main__":
    main()
