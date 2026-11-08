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

    def test_program_stderr_is_part_of_the_comparison(self):
        vm = {"code": 0, "timeout": False, "stderr": "program warning\n", "stdout": 'BAML_CORPUS_REPORT\t{"passed":["a"],"failed":[],"tolerated":[],"total":1,"messages":[]}\n'}
        rt = {"code": 0, "timeout": False, "stderr": "program warning\n", "stdout": 'BAML_CORPUS_RESULT\t{"name":"a","outcome":"pass","message":null}\n'}
        self.assertEqual(vm_leaf(vm, "a"), rt_leaf(rt, "a"))
        rt["stderr"] = "different warning\n"
        self.assertNotEqual(vm_leaf(vm, "a"), rt_leaf(rt, "a"))

    def test_output_without_newline_and_literal_marker_is_preserved(self):
        output = 'BAML_CORPUS_RESULT\tuser output without newline'
        vm = {"code": 0, "timeout": False, "stderr": "", "stdout": output + 'BAML_CORPUS_REPORT\t{"passed":["a"],"failed":[],"tolerated":[],"total":1,"messages":[]}\n'}
        rt = {"code": 0, "timeout": False, "stderr": "", "stdout": output + 'BAML_CORPUS_RESULT\t{"name":"a","outcome":"pass","message":null}\n'}
        self.assertEqual(vm_leaf(vm, "a"), rt_leaf(rt, "a"))
        self.assertEqual(vm_leaf(vm, "a")["output"], output)

    def test_legacy_cli_footer_is_metadata_but_protocol_two_preserves_program_lines(self):
        import json
        program = "    Finished 1 passed, 0 failed, 1 total in 2s\n"
        report = {"passed":["a"], "failed":[], "tolerated":[], "total":1, "messages":[]}
        vm = {"code":0, "timeout":False, "stderr":program + "    Finished 1 passed, 0 failed, 1 total in 3s\n",
              "stdout":"BAML_CORPUS_REPORT\t" + json.dumps(report) + "\n"}
        self.assertEqual(vm_leaf(vm, "a")["stderr"], program)
        report["stream_protocol"] = 2
        vm.update(stderr=program, stdout="BAML_CORPUS_REPORT\t" + json.dumps(report) + "\n")
        self.assertEqual(vm_leaf(vm, "a")["stderr"], program)

    def test_distinct_non_utf8_output_bytes_remain_distinct(self):
        import os, sys, tempfile
        from pathlib import Path
        from rt_corpus import invoke
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            result = invoke([sys.executable, "-c", "import sys; sys.stdout.buffer.write(bytes([255,13,10])); sys.stderr.buffer.write(bytes([254,10]))"],
                            env=os.environ, cwd=root, timeout=10, evidence=root/"bytes")
            self.assertEqual(result["stdout"].encode("utf-8", "surrogateescape"), b"\xff\r\n")
            self.assertEqual(result["stderr"].encode("utf-8", "surrogateescape"), b"\xfe\n")
            self.assertNotEqual(result["stdout"], result["stderr"])

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


class AdmissionWatchdogTests(unittest.TestCase):
    def test_generated_module_build_error_isolated_and_retried_next_run(self):
        import json, tempfile
        from pathlib import Path
        from rt_corpus import isolate_native_compilation_errors, restore_native_compilation_entries
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "pack/src/main.rs"
            source.parent.mkdir(parents=True)
            body = "const VALUE: i64 = missing;\n"
            source.write_text("mod test_3 {\n" + body + "}\nfn main() {}\n")
            admission = root / "admission"
            cache = admission / "cli/program/3.json"
            cache.parent.mkdir(parents=True)
            original = {"Ok": body}
            cache.write_text(json.dumps(original))
            backups = root / "originals"
            shared_error = "error[E0425]: missing runtime symbol\n --> runtime/src/lib.rs:1:2\n"
            self.assertEqual(isolate_native_compilation_errors(shared_error, source, admission, backups), [])
            self.assertEqual(json.loads(cache.read_text()), original)
            error = "error[E0425]: missing symbol\n --> src/main.rs:2:20\nerror: could not compile `host`\n"
            failures = isolate_native_compilation_errors(error, source, admission, backups)
            self.assertEqual(failures, [(3, "RT native compilation failed: error[E0425]: missing symbol")])
            self.assertEqual(json.loads(cache.read_text()), {"Err": failures[0][1]})
            restore_native_compilation_entries(admission, backups)
            self.assertEqual(json.loads(cache.read_text()), original)

    def test_aborted_compiler_is_attributed_to_its_active_entry(self):
        import json, os, sys, tempfile
        from pathlib import Path
        from rt_corpus import invoke
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            event = {"phase":"start", "name":"root::aborted", "entry":10, "cache_file":str(root / "10.json")}
            code = "import sys; print(" + repr("BAML_CORPUS_ADMISSION\t" + json.dumps(event)) + ",file=sys.stderr,flush=True); sys.exit(7)"
            env = dict(os.environ, BAML_RT_CORPUS_ADMISSION_CACHE=directory)
            result = invoke([sys.executable,"-c",code,"pack"], env=env, cwd=root, timeout=10, evidence=root/"attempt")
            self.assertEqual(result["code"],7)
            self.assertIsNone(result["admission_timeout"])
            self.assertEqual(result["admission_crash"]["entry"],10)

    def test_stuck_entry_is_identified_for_restart_without_losing_other_entries(self):
        import json, os, sys, tempfile
        from pathlib import Path
        from rt_corpus import invoke
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            event = {"phase":"start", "name":"root::stuck", "entry":9, "cache_file":str(root / "9.json")}
            code = "import sys,time; print(" + repr("BAML_CORPUS_ADMISSION\t" + json.dumps(event)) + ",file=sys.stderr,flush=True); time.sleep(30)"
            env = dict(os.environ, BAML_RT_CORPUS_ADMISSION_CACHE=directory, BAML_RT_CORPUS_TIMEOUT_SECS="1")
            result = invoke([sys.executable,"-c",code,"pack"], env=env, cwd=root, timeout=10, evidence=root/"attempt")
            self.assertFalse(result["timeout"])
            self.assertEqual(result["admission_timeout"]["entry"],9)
            self.assertEqual(result["admission_timeout"]["name"],"root::stuck")
            self.assertNotEqual(result["code"],0)


if __name__ == "__main__":
    unittest.main()
