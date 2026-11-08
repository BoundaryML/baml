"""Regression guards for per-test baseline checks and exact output comparison."""
import unittest
from rt_corpus import regressions, rt_leaf, vm_leaf


class CorpusGateTests(unittest.TestCase):
    def test_compiler_panics_cannot_masquerade_as_unsupported(self):
        from rt_corpus import admission_category
        self.assertEqual(admission_category("unsupported in baml_rt: closure"), "unsupported")
        self.assertEqual(admission_category("RT compiler panic: unsupported in baml_rt: closure"), "crash/timeout")

    def test_same_counts_cannot_hide_a_match_regression(self):
        before = {"tests": {"a": {"category": "match"}, "b": {"category": "unsupported"}}}
        after = {"tests": {"a": {"category": "unsupported"}, "b": {"category": "match"}}}
        self.assertEqual(regressions(before, after), ["match regressed to unsupported: a"])

    def test_new_mismatch_or_crash_fails_even_when_coverage_improves(self):
        before = {"tests": {"a": {"category": "unsupported"}}}
        for category in ("mismatch", "crash/timeout"):
            after = {"tests": {"a": {"category": category, "signature": "broken"}}}
            self.assertTrue(regressions(before, after))

    def test_missing_test_and_new_exclusion_fail(self):
        before = {"tests": {"a": {"category": "match"}}}
        self.assertTrue(regressions(before, {"tests": {}}))
        self.assertTrue(regressions(before, {"tests": {"a": {"category": "excluded"}}}))

    def test_failures_and_printed_output_are_compared(self):
        vm = {"code": 2, "timeout": False, "stderr": "", "stdout": 'printed\nBAML_CORPUS_REPORT\t{"passed":[],"failed":["a"],"tolerated":[],"total":1,"messages":["bad"]}\n'}
        rt = {"code": 2, "timeout": False, "stderr": "", "stdout": 'printed\nBAML_CORPUS_RESULT\t{"name":"a","outcome":"fail","message":"bad"}\n'}
        self.assertEqual(vm_leaf(vm, "a"), rt_leaf(rt, "a"))
        rt["stdout"] = rt["stdout"].replace("printed", "different")
        self.assertNotEqual(vm_leaf(vm, "a"), rt_leaf(rt, "a"))

    def test_zero_or_multiple_vm_leaves_cannot_count_as_a_match(self):
        vm = {"code": 0, "timeout": False, "stderr": "", "stdout": 'BAML_CORPUS_REPORT\t{"passed":[],"failed":[],"tolerated":[],"total":0,"messages":[]}\n'}
        with self.assertRaises(RuntimeError):
            vm_leaf(vm, "a")

class OracleCacheTests(unittest.TestCase):
    def test_vm_edits_invalidate_observations_but_rt_edits_do_not(self):
        import tempfile
        from pathlib import Path
        from unittest.mock import patch
        from rt_corpus import oracle_fingerprint
        with tempfile.TemporaryDirectory() as directory:
            workspace = Path(directory)
            for config in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml"):
                path = workspace / config
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("")
            vm = workspace / "crates/bex_vm/src/lib.rs"
            native = workspace / "crates/baml_compiler2_rust/src/rt/driver.rs"
            for path in (vm, native):
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("original")
            with patch("subprocess.check_output", return_value=b"cargo pinned"):
                original = oracle_fingerprint(workspace, {})
                native.write_text("native changed")
                self.assertEqual(original, oracle_fingerprint(workspace, {}))
                vm.write_text("VM changed")
                self.assertNotEqual(original, oracle_fingerprint(workspace, {}))


if __name__ == "__main__":
    unittest.main()
