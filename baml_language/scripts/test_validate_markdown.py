import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


class ValidateMarkdownTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "repo"
        self.root.mkdir()
        self.env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
        self.git("init", "--quiet")
        language = self.root / "baml_language"
        (language / "scripts").mkdir(parents=True)
        shutil.copyfile(Path(__file__).with_name("validate_markdown.py"), language / "scripts/validate_markdown.py")
        (language / ".markdown-whitelist").write_text("ARCHITECTURE.md\n")
        (language / "ARCHITECTURE.md").write_text("Allowed\n")
        (self.root / "outside.md").write_text("Outside the validator's scope\n")
        self.git("add", ".")
        self.git("-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "--quiet", "-m", "fixture")

    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.root), *args], env=self.env, text=True)

    def validate(self, root=None, **environment):
        root = root or self.root
        return subprocess.run(
            [sys.executable, str(root / "baml_language/scripts/validate_markdown.py")],
            cwd=self.temporary.name,
            env={**self.env, **environment},
            capture_output=True,
            text=True,
        )

    def test_only_tracked_language_markdown_is_checked(self):
        forbidden = self.root / "baml_language/forbidden.md"
        forbidden.write_text("Not whitelisted\n")
        self.assertEqual(self.validate().returncode, 0)
        self.git("add", str(forbidden))
        result = self.validate()
        self.assertEqual(result.returncode, 1)
        self.assertIn("  - forbidden.md", result.stdout)
        self.assertNotIn("outside.md", result.stdout)
        self.git("rm", "--cached", str(forbidden))
        self.assertEqual(self.validate().returncode, 0)

    def test_linked_worktree_hook_environment_keeps_relative_paths(self):
        worktree = Path(self.temporary.name) / "linked"
        self.git("worktree", "add", "--quiet", "--detach", str(worktree))
        git_dir = subprocess.check_output(
            ["git", "-C", str(worktree), "rev-parse", "--absolute-git-dir"],
            env=self.env, text=True,
        ).strip()
        result = self.validate(worktree, GIT_DIR=git_dir)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
