from __future__ import annotations

import datetime as dt
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from scripts.baml_nightly_select import select_candidate, timestamp


NOW = timestamp("2026-09-04T12:00:00Z")
SCRIPT = Path(__file__).resolve().parents[1] / "baml_nightly_select.py"


def commit(sha: str, age_seconds: int) -> dict:
    return {
        "sha": sha,
        "commit": {
            # Author dates can predate a squash merge; use the committer date.
            "author": {"date": "2020-01-01T00:00:00Z"},
            "committer": {
                "date": (NOW - dt.timedelta(seconds=age_seconds)).isoformat()
            },
        },
    }


def run(sha: str, run_id: int, conclusion: str | None = "success") -> dict:
    return {"headSha": sha, "databaseId": run_id, "conclusion": conclusion}


class NightlySelectionTests(unittest.TestCase):
    def test_selects_by_ancestry_then_highest_successful_run_id(self) -> None:
        commits = [commit("new", 3600), commit("old", 7200)]
        runs = [run("old", 100), run("new", 2), run("new", 5), run("new", 6, "failure")]
        self.assertEqual(select_candidate(commits, runs, NOW), ("new", 5))

    def test_all_run_states_satisfy_coverage(self) -> None:
        conclusions = [None, "", "failure", "cancelled", "skipped", "success"]
        commits = [commit(str(i), 3600 + i) for i in range(len(conclusions))]
        runs = [run(str(i), i, conclusion) for i, conclusion in enumerate(conclusions)]
        self.assertEqual(select_candidate(commits, runs, NOW), ("5", 5))

    def test_missing_new_commit_gets_grace_including_exactly_ten_minutes(self) -> None:
        for age in (0, 599, 600):
            with self.subTest(age=age):
                commits = [commit("new", age), commit("old", 7200)]
                self.assertEqual(
                    select_candidate(commits, [run("old", 1)], NOW), ("old", 1)
                )

    def test_missing_commit_over_ten_minutes_fails_despite_green_ancestor(self) -> None:
        commits = [commit("new", 601), commit("old", 7200)]
        with self.assertRaisesRegex(ValueError, "older than 10 minutes"):
            select_candidate(commits, [run("old", 1)], NOW)

    def test_recent_run_does_not_hide_a_hole_in_the_results(self) -> None:
        commits = [commit("head", 60), commit("missing", 3600), commit("old", 7200)]
        with self.assertRaisesRegex(ValueError, "missing runs"):
            select_candidate(commits, [run("head", 3), run("old", 1)], NOW)

    def test_idle_canary_is_fresh_when_its_commits_are_present(self) -> None:
        commits = [commit("head", 7 * 24 * 3600)]
        self.assertEqual(select_candidate(commits, [run("head", 1)], NOW), ("head", 1))

    def test_disjoint_or_empty_runs_fail_as_stale(self) -> None:
        for runs in ([], [run("unrelated", 1)]):
            with self.subTest(runs=runs), self.assertRaisesRegex(ValueError, "stale"):
                select_candidate([commit("head", 3600)], runs, NOW)

    def test_complete_coverage_without_success_fails_as_unreleasable(self) -> None:
        with self.assertRaisesRegex(ValueError, "nothing is releasable"):
            select_candidate([commit("head", 3600)], [run("head", 1, "failure")], NOW)

    def test_empty_commits_fail(self) -> None:
        with self.assertRaisesRegex(ValueError, "no commits"):
            select_candidate([], [run("head", 1)], NOW)

    def test_cli_writes_outputs_only_after_validation_succeeds(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            commits = root / "commits.json"
            runs = root / "runs.json"
            output = root / "output"
            commits.write_text(json.dumps([commit("head", 3600)]))
            for present in (False, True):
                with self.subTest(present=present):
                    runs.write_text(json.dumps([run("head", 42)] if present else []))
                    output.write_text("existing=value\n")
                    result = subprocess.run(
                        [
                            sys.executable,
                            str(SCRIPT),
                            "--commits",
                            str(commits),
                            "--runs",
                            str(runs),
                            "--observed-at",
                            NOW.isoformat(),
                            "--output",
                            str(output),
                        ],
                        capture_output=True,
                        text=True,
                    )
                    self.assertEqual(
                        result.returncode, 0 if present else 1, result.stdout
                    )
                    self.assertEqual(
                        output.read_text(),
                        "existing=value\n"
                        + ("sha=head\nrun_id=42\n" if present else ""),
                    )
                    if not present:
                        self.assertIn("::error::", result.stdout)


if __name__ == "__main__":
    unittest.main()
