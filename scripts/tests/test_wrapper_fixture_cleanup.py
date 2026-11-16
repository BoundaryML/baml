import errno
import os
from pathlib import Path
import runpy
import sys
import tempfile
import unittest
from unittest import mock


FIXTURES = Path(__file__).resolve().parents[2] / "baml_language/wrapper_tests"
with mock.patch.object(sys, "path", [str(FIXTURES), *sys.path]):
    FixtureFiles = runpy.run_path(str(FIXTURES / "fixture.py"))["FixtureFiles"]


class WrapperFixtureCleanupTest(unittest.TestCase):
    def remove_with_late_lock(self, root, target):
        rmdir = os.rmdir
        raced = False

        def create_lock_before_rmdir(path, *args, **kwargs):
            nonlocal raced
            if Path(path).name == "baml" and not raced:
                (root / "baml/telemetry.lock").touch()
                raced = True
            return rmdir(path, *args, **kwargs)

        with mock.patch.object(os, "rmdir", side_effect=create_lock_before_rmdir):
            FixtureFiles(root).remove([target])
        self.assertTrue(raced, "cleanup did not exercise the late lock race")

    def test_teardown_removes_a_lock_created_during_directory_removal(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "fixture"
            (root / "baml").mkdir(parents=True)
            self.remove_with_late_lock(root, ".")
            self.assertFalse(root.exists())

    def test_non_teardown_directory_errors_are_preserved(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "baml").mkdir()
            with self.assertRaises(OSError) as raised:
                self.remove_with_late_lock(root, "baml")
            self.assertEqual(raised.exception.errno, errno.ENOTEMPTY)
            self.assertTrue((root / "baml/telemetry.lock").exists())


if __name__ == "__main__":
    unittest.main()
