from __future__ import annotations

import datetime as dt
import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
SELECTOR_PATH = ROOT / "tools" / "baml_nightly_select.py"


def selector_module():
    """Load the standalone selector tool as an importable test module."""
    spec = importlib.util.spec_from_file_location("baml_nightly_select", SELECTOR_PATH)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


SELECTOR = selector_module()
OBSERVED_AT = dt.datetime(2026, 9, 15, 7, 25, tzinfo=dt.timezone.utc)


def commit(sha: str, committed_at: str) -> dict:
    """Build a git-log commit fixture."""
    return {"sha": sha, "committedAt": committed_at}


def run(sha: str, run_id: int, conclusion: str) -> dict:
    """Build an eligible canary CI run fixture."""
    return {
        "headSha": sha,
        "databaseId": run_id,
        "conclusion": conclusion,
        "workflowName": "CI - BAML Language",
        "headBranch": "canary",
        "event": "push",
    }


class NightlySelectorTests(unittest.TestCase):
    @mock.patch.object(SELECTOR, "command_json")
    def test_recent_query_filters_only_by_workflow(self, command_json) -> None:
        """The primary query scopes CI but leaves branch and event filtering local."""
        command_json.return_value = []

        self.assertEqual(SELECTOR.fetch_recent_runs("BoundaryML/baml"), [])
        command_json.assert_called_once_with(
            [
                "gh",
                "run",
                "list",
                "--repo",
                "BoundaryML/baml",
                "--workflow",
                "ci.yaml",
                "--limit",
                "100",
                "--json",
                SELECTOR.RUN_FIELDS,
            ]
        )

    def test_selects_newest_successful_commit_in_ancestry_order(self) -> None:
        """Git ancestry wins even when API results arrive out of order."""
        commits = [
            commit("newest", "2026-09-15T07:00:00Z"),
            commit("older", "2026-09-15T06:00:00Z"),
        ]
        runs = [run("older", 20, "success"), run("newest", 10, "success")]

        self.assertEqual(
            SELECTOR.select_candidate(commits, runs, OBSERVED_AT, "test"),
            ("newest", 10),
        )

    def test_uses_latest_successful_rerun_id(self) -> None:
        """The newest successful attempt attests a multiply-run commit."""
        commits = [commit("candidate", "2026-09-15T06:00:00Z")]
        runs = [
            run("candidate", 10, "failure"),
            run("candidate", 11, "success"),
            run("candidate", 12, "success"),
        ]

        self.assertEqual(
            SELECTOR.select_candidate(commits, runs, OBSERVED_AT, "test"),
            ("candidate", 12),
        )

    def test_rejects_stale_run_snapshot_before_selecting_older_green(self) -> None:
        """An old gap prevents silently releasing a stale green commit."""
        commits = [
            commit("missing", "2026-09-15T06:00:00Z"),
            commit("older", "2026-09-15T05:00:00Z"),
        ]

        with self.assertRaisesRegex(
            ValueError,
            "test has no CI run for missing",
        ) as raised:
            SELECTOR.select_candidate(
                commits, [run("older", 10, "success")], OBSERVED_AT, "test"
            )
        self.assertIn("age 1:25:00", str(raised.exception))

    def test_allows_actions_visibility_grace_period(self) -> None:
        """A newly pushed run may remain invisible for a short grace period."""
        commits = [
            commit("pending", "2026-09-15T07:20:00Z"),
            commit("older", "2026-09-15T06:00:00Z"),
        ]

        self.assertEqual(
            SELECTOR.select_candidate(
                commits, [run("older", 10, "success")], OBSERVED_AT, "test"
            ),
            ("older", 10),
        )

    def test_falls_back_when_recent_runs_have_a_canary_ci_hole(self) -> None:
        """A primary snapshot hole switches selection to per-commit data."""
        commits = [
            commit("newest", "2026-09-15T06:00:00Z"),
            commit("older", "2026-09-15T05:00:00Z"),
        ]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            recent_path = root / "recent.json"
            commits_path = root / "commits.json"
            SELECTOR.write_json(recent_path, [run("older", 10, "success")])
            SELECTOR.write_json(
                commits_path,
                {
                    "commits": commits,
                    "runs": [run("newest", 20, "success")],
                    "errors": [],
                },
            )

            self.assertEqual(
                SELECTOR.select_with_fallback(recent_path, commits_path, OBSERVED_AT),
                ("newest", 20, "per-commit fallback"),
            )

    def test_prefers_recent_runs_when_both_sources_have_a_candidate(self) -> None:
        """A valid broad snapshot remains the preferred source."""
        commits = [commit("candidate", "2026-09-15T06:00:00Z")]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            recent_path = root / "recent.json"
            commits_path = root / "commits.json"
            SELECTOR.write_json(recent_path, [run("candidate", 10, "success")])
            SELECTOR.write_json(
                commits_path,
                {
                    "commits": commits,
                    "runs": [run("candidate", 20, "success")],
                    "errors": [],
                },
            )

            self.assertEqual(
                SELECTOR.select_with_fallback(recent_path, commits_path, OBSERVED_AT),
                ("candidate", 10, "100 most recent workflow runs"),
            )

    def test_rejects_partial_per_commit_fallback(self) -> None:
        """Any failed exact-SHA query invalidates the fallback batch."""
        commits = [commit("candidate", "2026-09-15T06:00:00Z")]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            recent_path = root / "recent.json"
            commits_path = root / "commits.json"
            SELECTOR.write_json(recent_path, [])
            SELECTOR.write_json(
                commits_path,
                {
                    "commits": commits,
                    "runs": [run("candidate", 20, "success")],
                    "errors": [{"sha": "another", "error": "request failed"}],
                },
            )

            with self.assertRaisesRegex(
                ValueError, "per-commit fallback has 1 failed run query"
            ):
                SELECTOR.select_with_fallback(recent_path, commits_path, OBSERVED_AT)

    def test_filters_unrelated_recent_runs_locally(self) -> None:
        """Workflow, branch, and event filters are applied only in process."""
        matching = run("candidate", 10, "success")
        wrong_workflow = {**matching, "workflowName": "Another workflow"}
        wrong_branch = {**matching, "headBranch": "main"}
        wrong_event = {**matching, "event": "pull_request"}

        self.assertEqual(
            SELECTOR.eligible_ci_runs(
                [wrong_workflow, wrong_branch, wrong_event, matching]
            ),
            [matching],
        )


if __name__ == "__main__":
    unittest.main()
