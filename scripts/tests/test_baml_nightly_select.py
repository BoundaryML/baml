from __future__ import annotations

import datetime as dt
import importlib.util
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SELECTOR_PATH = ROOT / "scripts" / "baml_nightly_select.py"


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
    return {"sha": sha, "commit": {"committer": {"date": committed_at}}}


def run(sha: str, run_id: int, conclusion: str) -> dict:
    return {"headSha": sha, "databaseId": run_id, "conclusion": conclusion}


class NightlySelectorTests(unittest.TestCase):
    def test_selects_newest_successful_commit_in_ancestry_order(self) -> None:
        commits = [
            commit("newest", "2026-09-15T07:00:00Z"),
            commit("older", "2026-09-15T06:00:00Z"),
        ]
        runs = [run("older", 20, "success"), run("newest", 10, "success")]

        self.assertEqual(
            SELECTOR.select_candidate(commits, runs, OBSERVED_AT), ("newest", 10)
        )

    def test_uses_latest_successful_rerun_id(self) -> None:
        commits = [commit("candidate", "2026-09-15T06:00:00Z")]
        runs = [
            run("candidate", 10, "failure"),
            run("candidate", 11, "success"),
            run("candidate", 12, "success"),
        ]

        self.assertEqual(
            SELECTOR.select_candidate(commits, runs, OBSERVED_AT),
            ("candidate", 12),
        )

    def test_rejects_stale_run_snapshot_before_selecting_older_green(self) -> None:
        commits = [
            commit("missing", "2026-09-15T06:00:00Z"),
            commit("older", "2026-09-15T05:00:00Z"),
        ]

        with self.assertRaisesRegex(
            ValueError,
            "missing runs for 1 canary commit",
        ) as raised:
            SELECTOR.select_candidate(
                commits, [run("older", 10, "success")], OBSERVED_AT
            )
        self.assertIn("\nmissing ", str(raised.exception))

    def test_allows_actions_visibility_grace_period(self) -> None:
        commits = [
            commit("pending", "2026-09-15T07:20:00Z"),
            commit("older", "2026-09-15T06:00:00Z"),
        ]

        self.assertEqual(
            SELECTOR.select_candidate(
                commits, [run("older", 10, "success")], OBSERVED_AT
            ),
            ("older", 10),
        )


if __name__ == "__main__":
    unittest.main()
