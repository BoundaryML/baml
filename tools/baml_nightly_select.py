#!/usr/bin/env python3
"""Collect independent CI run snapshots and select a nightly release source."""

from __future__ import annotations

import argparse
import concurrent.futures
import datetime as dt
import json
import subprocess
from pathlib import Path
from typing import Optional


CI_WORKFLOW = "CI - BAML Language"
MAX_RUN_LAG = dt.timedelta(minutes=10)
RUN_FIELDS = "headSha,databaseId,conclusion,createdAt,event,headBranch,workflowName"


def timestamp(value: str) -> dt.datetime:
    parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        raise ValueError(f"timestamp must include a timezone: {value}")
    return parsed


def command_json(command: list[str]) -> object:
    result = subprocess.run(command, check=True, text=True, capture_output=True)
    return json.loads(result.stdout)


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def fetch_recent_runs(repo: str) -> list[dict]:
    value = command_json(
        [
            "gh",
            "run",
            "list",
            "--repo",
            repo,
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


def read_optional_json(path: Path) -> Optional[object]:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (FileNotFoundError, json.JSONDecodeError) as exc:
        print(f"::warning::Could not read {path}: {exc}")
        return None


def select_with_fallback(
    recent_path: Path, commits_path: Path, observed_at: dt.datetime
) -> tuple[str, int, str]:
    commit_snapshot = read_optional_json(commits_path)
    if not isinstance(commit_snapshot, dict) or not isinstance(
        commit_snapshot.get("commits"), list
    ):
        raise ValueError("per-commit fallback did not produce a valid canary git log")
    commits = commit_snapshot["commits"]

    failures = []
    recent_runs = read_optional_json(recent_path)
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
    if isinstance(fallback_runs, list):
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


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    recent = subparsers.add_parser("fetch-recent")
    recent.add_argument("--repo", required=True)
    recent.add_argument("--output", required=True, type=Path)

    commits = subparsers.add_parser("fetch-commits")
    commits.add_argument("--repo", required=True)
    commits.add_argument("--checkout", required=True, type=Path)
    commits.add_argument("--output", required=True, type=Path)

    select = subparsers.add_parser("select")
    select.add_argument("--recent", required=True, type=Path)
    select.add_argument("--commits", required=True, type=Path)
    select.add_argument("--observed-at", required=True, type=timestamp)
    select.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    try:
        if args.command == "fetch-recent":
            runs = fetch_recent_runs(args.repo)
            write_json(args.output, runs)
            print(f"Fetched {len(runs)} unfiltered recent workflow runs.")
        elif args.command == "fetch-commits":
            snapshot = fetch_commit_runs(args.repo, args.checkout)
            write_json(args.output, snapshot)
            print(
                f"Queried {len(snapshot['commits'])} canary commits in parallel; "
                f"found {len(snapshot['runs'])} runs with {len(snapshot['errors'])} query errors."
            )
            for error in snapshot["errors"]:
                print(
                    f"::warning::Run query failed for {error['sha']}: {error['error']}"
                )
        else:
            sha, run_id, source = select_with_fallback(
                args.recent, args.commits, args.observed_at
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
