from __future__ import annotations

import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CI_WORKFLOW = ROOT / ".github" / "workflows" / "ci.yaml"
MISE_CONFIG = ROOT / "mise.toml"


def job_block(text: str, job: str) -> str:
    marker = f"  {job}:\n"
    start = text.index(marker)
    remainder = text[start + len(marker) :]
    lines = remainder.splitlines(keepends=True)
    next_job = next(
        (
            index
            for index, line in enumerate(lines)
            if line.startswith("  ")
            and not line.startswith("    ")
            and line.rstrip().endswith(":")
        ),
        len(lines),
    )
    return "".join(lines[:next_job])


class PrecommitCiContractTests(unittest.TestCase):
    def test_prek_job_installs_only_its_required_mise_tools(self) -> None:
        prek = job_block(CI_WORKFLOW.read_text(encoding="utf-8"), "prek")

        self.assertIn('install_args: "cargo:prek python uv node clang-format"', prek)
        self.assertIn('MISE_TASK_RUN_AUTO_INSTALL: "false"', prek)
        self.assertIn(
            'clang-format = "22.1.7"',
            MISE_CONFIG.read_text(encoding="utf-8"),
        )


if __name__ == "__main__":
    unittest.main()
