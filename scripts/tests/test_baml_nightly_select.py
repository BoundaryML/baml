from __future__ import annotations

import datetime as dt
import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SELECTOR_PATH = ROOT / "tools" / "baml_nightly_select.py"


def selector_module():
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
    return {"sha": sha, "committedAt": committed_at}


def run(sha: str, run_id: int, conclusion: str) -> dict:
    return {
        "headSha": sha,
        "databaseId": run_id,
        "conclusion": conclusion,
        "workflowName": "CI - BAML Language",
        "headBranch": "canary",
        "event": "push",
    }


class NightlySelectorTests(unittest.TestCase):
    def test_selects_newest_successful_commit_in_ancestry_order(self) -> None:
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

    def test_filters_unrelated_recent_runs_locally(self) -> None:
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
