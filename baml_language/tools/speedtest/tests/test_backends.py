import contextlib
import io
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from speedtest import cli, kpis, rows, runner
from speedtest.measure import Trial
from speedtest.fmt import fmt_ratio


class Backends(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.stage = Path(self.tmp.name)
        source = self.stage / 'main.baml'
        source.write_text('function main() -> int { 42 }')
        self.workload = {'id': 'process/startup', 'name': 'process::startup',
                         'description': 'startup', 'baml': source.read_text(), 'files': {'baml': source}}

    def pack(self, backend, error=None, missing=False):
        def invoke(command, **kwargs):
            if error is None and not missing:
                Path(command[command.index('-o') + 1]).write_bytes(b'executable')
            return subprocess.CompletedProcess(command, int(error is not None), '', error or '')
        with patch.object(runner.subprocess, 'run', side_effect=invoke) as call:
            result = runner.build_baml(Path('/baml'), self.workload, self.stage, backend)
        return result, call.call_args.args[0]

    def test_separate_builds_and_native_drivers(self):
        for backend in ('vm', 'native', 'rt'):
            result, command = self.pack(backend)
            self.assertEqual(result['status'], 'ok')
            self.assertEqual(result['bytes'], 10)
            self.assertGreaterEqual(result['seconds'], 0)
            self.assertEqual('--rust' in command, backend != 'vm')
            self.assertEqual('--rt' in command, backend == 'rt')
            self.assertIn('main' if backend == 'rt' else 'speedtest_driver', command)
            project = Path(result['artifact']).parent
            self.assertEqual((project / 'baml_src/speedtest_driver.baml').exists(), backend != 'rt')
            self.assertIn('BAML_TELEMETRY = "BAML_TELEMETRY"', (project / 'baml.toml').read_text())
            if backend != 'vm':
                self.assertIn('--rust-workspace', command)
                self.assertEqual(command[command.index('--rust-build-dir') + 1], str(self.stage / 'rust-build' / backend))

    def test_only_rt_compiler_rejection_is_unsupported(self):
        result, _ = self.pack('rt', 'error: unsupported in baml_rt: float\npack failed')
        self.assertEqual(result['status'], 'unsupported')
        self.assertEqual(result['error'], 'unsupported by baml_rt: float')
        result, _ = self.pack('native', 'error: unsupported in baml_rt: float')
        self.assertEqual(result['status'], 'failed')

    def test_other_failures_remain_failures(self):
        result, _ = self.pack('rt', 'cargo failed')
        self.assertEqual(result['status'], 'failed')

    def test_missing_binary_is_failure(self):
        result, _ = self.pack('rt', missing=True)
        self.assertEqual(result['status'], 'failed')

    def test_levels_share_backend_and_isolate_environment(self):
        for backend in ('native', 'rt'):
            off, medium = (rows.BY_KEY[f'{backend}-{level}'] for level in ('off', 'medium'))
            self.assertEqual(off.backend, medium.backend)
            env = medium.env({'BAML_COMPILED_BODIES': '0', 'BAML_GC_STRESS': '1',
                              'BOUNDARY_PROJECT': 'remote'}, self.stage)
            self.assertEqual(env['BAML_TELEMETRY'], 'medium')
            self.assertEqual(env['BAML_COMPILED_BODIES'], '1' if backend == 'native' else '0')
            self.assertEqual(env['BAML_GC_STRESS'], '0')
            self.assertNotIn('BOUNDARY_PROJECT', env)

    def test_unsupported_report_and_ratios(self):
        cell = {'status': 'unsupported', 'reason': 'unsupported by baml_rt: float'}
        row = rows.BY_KEY['rt-off']
        for metric in kpis.UNITS:
            self.assertEqual(kpis._value(metric, cell, {}, row), {'note': cell['reason']})
        self.assertEqual(kpis.rt_speedups({'rt-off': cell}), {'vm-off': None, 'node': None})
        cells = {k: {'status': 'ok', 'per_job_ns': {'median': v}}
                 for k, v in [('rt-off', 2), ('vm-off', 10), ('node', 1)]}
        self.assertEqual(kpis.rt_speedups(cells), {'vm-off': 5, 'node': .5})

    def test_build_kpis_for_each_backend(self):
        for backend in ('vm', 'native', 'rt'):
            build = {'status': 'ok', 'bytes': 2048, 'seconds': 3}
            self.assertEqual(kpis._value('binary_kib', {'status': 'ok'}, build,
                                        rows.BY_KEY[f'{backend}-off']), {'median': 2})
            self.assertEqual(kpis._value('build_s', {'status': 'ok'}, build,
                                        rows.BY_KEY[f'{backend}-medium']), kpis.SAME_AS_OFF)

    def test_failed_cells_have_no_aggregate(self):
        cell = runner.Cell(self.workload, rows.BY_KEY['rt-off'])
        cell.trials.append(Trial(ok=True, jobs=1, elapsed_ns=100))
        cell.fail('mismatch')
        self.assertNotIn('per_job_ns', cell.summary())

    def test_kpi_explicit_rows_build_once_per_backend(self):
        cli_file = self.stage / 'baml'
        cli_file.touch()
        (self.stage / 'target').mkdir()
        keys = ['vm-off', 'vm-medium', 'native-off', 'native-medium', 'rt-off', 'rt-medium', 'node']
        args = cli.build_parser().parse_args(['run', '--kpi', '--rows', ','.join(keys),
                                             '--baml', str(cli_file), '--trials', '1'])
        self.workload.update(jobs=1, warmup=0, expect='42')
        packs = []
        def pack(cli, workload, stage, backend, rust_build_dir=None):
            packs.append(backend)
            return {'status': 'unsupported', 'error': 'unsupported by baml_rt: float'} if backend == 'rt' else {
                'status': 'ok', 'artifact': backend, 'seconds': 1, 'bytes': 10}
        def trial(*args):
            return Trial(ok=True, result='42', jobs=1, elapsed_ns=10, process_ns=20,
                         cpu_ns=10, peak_rss_bytes=100)
        with patch.object(runner, '_repo_root', return_value=self.stage), \
             patch.object(runner, 'load_workloads', return_value=[self.workload]), \
             patch.object(kpis, 'workload_names', return_value={self.workload['name']}), \
             patch.object(runner, 'build_baml', side_effect=pack), \
             patch.object(runner, '_trial', side_effect=trial) as run_trial, \
             patch.object(rows, 'toolchain_version', return_value='v1'), \
             patch.object(runner, '_save') as save, \
             contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            runner.cmd_run(args)
        self.assertEqual(packs, ['vm', 'native', 'rt'])
        self.assertEqual([r.key for r in save.call_args.args[3]], keys)
        self.assertEqual(run_trial.call_count, 10)  # verify and one trial, unsupported never invoked
        cells = save.call_args.args[4]
        self.assertEqual(cells[(self.workload['id'], 'rt-medium')].status, 'unsupported')

    def test_small_speedup_is_not_rounded_to_zero(self):
        self.assertEqual(fmt_ratio(0.03922, 19.61), "0.002x")

    def test_table_handles_unsupported(self):
        row_list = rows.resolve(['vm-off', 'rt-off', 'node'])
        cells = {(self.workload['id'], r.key): runner.Cell(self.workload, r) for r in row_list}
        cells[(self.workload['id'], 'rt-off')].status = 'unsupported'
        with contextlib.redirect_stdout(io.StringIO()) as output:
            runner.print_table([self.workload], row_list, cells)
        self.assertIn('rt speedup vs VM', output.getvalue())
        self.assertIn('unsupported by baml_rt', output.getvalue())


if __name__ == '__main__':
    unittest.main()
