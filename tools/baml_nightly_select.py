#!/usr/bin/env python3
"""Collect CI run data and select a nightly release source."""

from __future__ import annotations

import argparse
import concurrent.futures
import datetime as dt
import json
import subprocess
from pathlib import Path


CI_WORKFLOW = "CI - BAML Language"
MAX_RUN_LAG = dt.timedelta(minutes=10)
RUN_FIELDS = "headSha,databaseId,conclusion,createdAt,event,headBranch,workflowName"


def timestamp(value: str) -> dt.datetime:
    """Parse an ISO 8601 timestamp and require an explicit timezone."""
    parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        raise ValueError(f"timestamp must include a timezone: {value}")
    return parsed


def command_json(command: list[str]) -> object:
    """Run a command and parse its stdout as JSON."""
    result = subprocess.run(command, check=True, text=True, capture_output=True)
    return json.loads(result.stdout)


def fetch_recent_runs(repo: str) -> list[dict]:
    """Fetch the CI workflow's 100 newest runs without branch or event filters."""
    value = command_json(
        [
            "gh",
            "run",
            "list",
            "--repo",
            repo,
            "--workflow",
            "ci.yaml",
            "--limit",
            "100",
            "--json",
            RUN_FIELDS,
        ]
    )
    if not isinstance(value, list):
        raise ValueError("recent workflow runs query did not return a list")
    return value


def canary_commits(checkout: Path) -> list[dict]:
    """Read the 10 newest commits from the checked-out canary git log."""
    result = subprocess.run(
        [
            "git",
            "-C",
            str(checkout),
            "log",
            "-10",
            "--format=%H%x09%cI",
            "HEAD",
        ],
        check=True,
        text=True,
        capture_output=True,
    )
    commits = []
    for line in result.stdout.splitlines():
        sha, committed_at = line.split("\t", 1)
        commits.append({"sha": sha, "committedAt": committed_at})
    if not commits:
        raise ValueError("git log returned no canary commits")
    return commits


def normalize_api_run(run: dict) -> dict:
    """Convert a REST workflow run to the gh run list field names."""
    return {
        "headSha": run["head_sha"],
        "databaseId": run["id"],
        "conclusion": run["conclusion"],
        "createdAt": run["created_at"],
        "event": run["event"],
        "headBranch": run["head_branch"],
        "workflowName": run["name"],
    }


def fetch_runs_for_commit(repo: str, sha: str) -> list[dict]:
    """Fetch CI workflow runs for one exact commit SHA."""
    value = command_json(
        [
            "gh",
            "api",
            "--method",
            "GET",
            f"repos/{repo}/actions/workflows/ci.yaml/runs",
            "-f",
            f"head_sha={sha}",
            "-f",
            "per_page=10",
        ]
    )
    if not isinstance(value, dict) or not isinstance(value.get("workflow_runs"), list):
        raise ValueError(f"workflow runs query for {sha} returned an invalid response")
    return [normalize_api_run(run) for run in value["workflow_runs"]]


def fetch_commit_runs(repo: str, checkout: Path) -> dict:
    """Fetch CI runs for the newest canary commits concurrently."""
    commits = canary_commits(checkout)
    runs = []
    errors = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=len(commits)) as executor:
        futures = {
            executor.submit(fetch_runs_for_commit, repo, commit["sha"]): commit["sha"]
            for commit in commits
        }
        for future in concurrent.futures.as_completed(futures):
            sha = futures[future]
            try:
                runs.extend(future.result())
            except (
                subprocess.CalledProcessError,
                ValueError,
                KeyError,
                TypeError,
            ) as exc:
                errors.append({"sha": sha, "error": str(exc)})
    return {"commits": commits, "runs": runs, "errors": errors}


def eligible_ci_runs(runs: list[dict]) -> list[dict]:
    """Filter a run snapshot locally to canary push runs of BAML CI."""
    return [
        run
        for run in runs
        if run.get("workflowName") == CI_WORKFLOW
        and run.get("headBranch") == "canary"
        and run.get("event") == "push"
    ]


def select_candidate(
    commits: list[dict], runs: list[dict], observed_at: dt.datetime, source: str
) -> tuple[str, int]:
    """Select the newest green commit without stepping past an old run gap."""
    if not commits:
        raise ValueError("canary git log returned no commits")

    runs_by_sha: dict[str, list[dict]] = {}
    for run in eligible_ci_runs(runs):
        runs_by_sha.setdefault(run["headSha"], []).append(run)

    for commit in commits:
        sha = commit["sha"]
        commit_runs = runs_by_sha.get(sha, [])
        if not commit_runs:
            committed_at = timestamp(commit["committedAt"])
            age = observed_at - committed_at
            detail = f"{sha} (committed {committed_at.isoformat()}, age {age})"
            if age > MAX_RUN_LAG:
                raise ValueError(f"{source} has no CI run for {detail}")
            print(f"Within the 10-minute CI visibility grace period: {detail}")
            continue

        successful_ids = [
            run["databaseId"] for run in commit_runs if run["conclusion"] == "success"
        ]
        if successful_ids:
            return sha, max(successful_ids)

    raise ValueError(
        f"{source} has no successful CI run for the {len(commits)} newest canary commits"
    )


def select_with_fallback(
    recent_runs: list[dict] | None,
    commit_snapshot: dict,
    observed_at: dt.datetime,
) -> tuple[str, int, str]:
    """Prefer the recent-run result, then try the per-commit result."""
    if not isinstance(commit_snapshot.get("commits"), list):
        raise ValueError("per-commit fallback did not produce a valid canary git log")
    commits = commit_snapshot["commits"]

    failures = []
    if isinstance(recent_runs, list):
        try:
            sha, run_id = select_candidate(
                commits, recent_runs, observed_at, "100 most recent workflow runs"
            )
            return sha, run_id, "100 most recent workflow runs"
        except (ValueError, KeyError, TypeError) as exc:
            failures.append(str(exc))
            print(
                f"::warning::Recent-runs selection failed; trying per-commit fallback: {exc}"
            )
    else:
        failures.append(
            "100 most recent workflow runs query did not produce valid JSON"
        )

    fallback_runs = commit_snapshot.get("runs")
    fallback_errors = commit_snapshot.get("errors")
    if isinstance(fallback_errors, list) and fallback_errors:
        failures.append(
            f"per-commit fallback has {len(fallback_errors)} failed run query or queries"
        )
    elif isinstance(fallback_runs, list):
        try:
            sha, run_id = select_candidate(
                commits, fallback_runs, observed_at, "per-commit fallback"
            )
            return sha, run_id, "per-commit fallback"
        except (ValueError, KeyError, TypeError) as exc:
            failures.append(str(exc))
    else:
        failures.append("per-commit fallback did not produce a runs list")

    raise ValueError("; ".join(failures))


def collect_and_select(
    repo: str, checkout: Path, observed_at: dt.datetime
) -> tuple[str, int, str]:
    """Always collect both run sources, then select with the primary first."""
    recent_runs = None
    try:
        recent_runs = fetch_recent_runs(repo)
        print(f"Fetched {len(recent_runs)} recent CI workflow runs.")
    except (subprocess.CalledProcessError, ValueError) as exc:
        print(f"::warning::Recent CI workflow run query failed: {exc}")

    commit_snapshot = fetch_commit_runs(repo, checkout)
    print(
        f"Queried {len(commit_snapshot['commits'])} canary commits in parallel; "
        f"found {len(commit_snapshot['runs'])} runs with "
        f"{len(commit_snapshot['errors'])} query errors."
    )
    for error in commit_snapshot["errors"]:
        print(f"::warning::Run query failed for {error['sha']}: {error['error']}")

    return select_with_fallback(recent_runs, commit_snapshot, observed_at)


def main() -> None:
    """Collect both CI data sources and select the nightly source commit."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--checkout", required=True, type=Path)
    parser.add_argument("--observed-at", required=True, type=timestamp)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    try:
        sha, run_id, source = collect_and_select(
            args.repo, args.checkout, args.observed_at
        )
        with args.output.open("a", encoding="utf-8") as output:
            output.write(f"sha={sha}\nrun_id={run_id}\n")
        print(f"Newest green canary commit: {sha} (CI run {run_id}, via {source})")
    except (subprocess.CalledProcessError, ValueError, KeyError, TypeError) as exc:
        message = str(exc).replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
        print(f"::error::{message}")
        raise SystemExit(1) from exc


if __name__ == "__main__":
    main()
