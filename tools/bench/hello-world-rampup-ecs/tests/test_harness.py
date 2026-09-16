import importlib.util
import json
from pathlib import Path
import unittest
from unittest.mock import patch

from scripts.local_binary_search import midpoint

ROOT = Path(__file__).resolve().parents[1]


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


load = module('load', 'load-generator/run.py')
analyze = module('analyze', 'scripts/analyze.py')
gc_grid_data = module('gc_grid_data', 'scripts/gc_grid_data.py')



class HarnessTest(unittest.TestCase):
    def test_gc_grid_merge_preserves_run_history_and_newer_cells(self):
        def source(run, cells, runs=None):
            metadata = {'run': run, 'rates_rps': [cell['rate'] for cell in cells], 'gc_frequencies_hz': [cell['gc_frequency_hz'] for cell in cells]}
            if runs:
                metadata['runs'] = runs
            return {'metadata': metadata, 'cells': cells, 'app_stops': [], 'frontier': []}

        original = source('original', [{'gc_frequency_hz': 0, 'rate': 300, 'outcome': 'complete'}])
        first_merge = source('first-merge', [{'gc_frequency_hz': 0, 'rate': 325, 'outcome': 'oom'}], ['original', 'first-merge'])
        newest = source('newest', [{'gc_frequency_hz': 0, 'rate': 300, 'outcome': 'oom'}])
        result = gc_grid_data.merge_data(gc_grid_data.merge_data(original, first_merge), newest)
        cells = {(cell['gc_frequency_hz'], cell['rate']): cell for cell in result['cells']}
        self.assertEqual(result['metadata']['runs'], ['original', 'first-merge', 'newest'])
        self.assertEqual(cells[(0, 300)]['outcome'], 'oom')
        self.assertEqual(cells[(0, 325)]['outcome'], 'oom')

    def test_explicit_gc_requires_ok_response(self):
        response = unittest.mock.MagicMock()
        response.status = 200
        response.read.return_value = b'ok'
        response.__enter__.return_value = response
        with patch.object(load.urllib.request, 'urlopen', return_value=response):
            result = load.request_explicit_gc('http://example.test/gc')
        self.assertTrue(result['ok'])
        self.assertEqual(result['status'], 200)
        self.assertIsNone(result['error'])

    def test_ready_wait_requires_probe_and_gc(self):
        stop = load.threading.Event()
        with patch.object(load, 'probe_body', side_effect=[(False, False), (False, True)]), \
             patch.object(load, 'request_explicit_gc', return_value={'ok': True, 'status': 200, 'duration_ms': 1, 'error': None}), \
             patch.object(stop, 'wait', return_value=False):
            result = load.wait_until_ready('http://target/', 'http://target/gc', stop)
        self.assertTrue(result['ok'])

    def test_gc_summary_reports_achieved_frequency_and_durations(self):
        results = [
            {'ok': True, 'duration_ms': 10},
            {'ok': False, 'duration_ms': 40},
            {'ok': True, 'duration_ms': 20},
        ]
        summary = load.gc_summary(results, skipped=2, frequency_hz=10, elapsed_seconds=2)
        self.assertEqual(summary['configured_frequency_hz'], 10)
        self.assertEqual(summary['attempted'], 3)
        self.assertEqual(summary['successful'], 2)
        self.assertEqual(summary['failed'], 1)
        self.assertEqual(summary['skipped_ticks'], 2)
        self.assertEqual(summary['achieved_frequency_hz'], 1)
        self.assertEqual(summary['duration_ms_median'], 20)

    def test_gc_frontier_midpoint_prioritizes_the_middle_resolution_step(self):
        self.assertEqual(load.frontier_midpoint(5000, 10000, 500), 7500)
        self.assertEqual(load.frontier_midpoint(7500, 10000, 500), 8500)
        self.assertEqual(load.frontier_midpoint(9500, 10000, 500), 10000)

    def test_binary_search_midpoint_respects_resolution(self):
        self.assertEqual(midpoint(5000, 10000, 100), 7500)
        self.assertEqual(midpoint(5000, 7500, 100), 6200)

    def test_emf_bounded_batches_and_errors(self):
        lines = []
        c = load.Collector({'RunName': 'test', 'Variant': 'node-only', 'Architecture': 'arm64'}, lines.append)
        summary = load.summarize_report({'requests': 208, 'status_codes': {'200': 206, '503': 1},
                                         'latencies': {'50th': 1_000_000, '90th': 2_000_000, '99th': 3_000_000, 'max': 4_000_000}},
                                        210, True)
        self.assertEqual([summary[k] for k in load.COUNTERS], [208, 206, 1, 1, 1])
        self.assertEqual([summary[k] for k in ('LatencyP50Ms', 'LatencyP90Ms', 'LatencyP99Ms', 'LatencyMaxMs')], [1, 2, 3, 4])
        c.emit(summary, {key: 'Count' for key in summary})
        self.assertEqual(json.loads(lines[-1])['RunName'], 'test')
        with patch.object(load.urllib.request, 'urlopen', side_effect=OSError('stopped')):
            c.scrape('http://stopped/metrics')
        self.assertEqual(json.loads(lines[-1])['ProcessMetricsUp'], 0)
        self.assertNotIn('ProcessRssBytes', json.loads(lines[-1]))

    def test_rate_ramps_and_caps(self):
        self.assertEqual([load.rate_at(100, 100, 30, 1000, elapsed) for elapsed in (0, 29.9, 30, 299, 999)],
                         [100, 100, 200, 1000, 1000])

    def test_analysis_requires_five_clean_cycles_and_finds_first_failure(self):
        events = []
        for rate, count in ((100, 6), (200, 6), (300, 6)):
            for cycle in range(count):
                scheduled = rate * 4
                ok = scheduled if rate == 100 else scheduled - 20
                events.append({'Variant': 'node-only', 'Architecture': 'arm64', 'rate': rate, 'scheduled': scheduled,
                               'Http200': ok, 'TransportErrors': scheduled - ok, 'HttpErrors': 0, 'BodyMismatches': 0,
                               'completion_ratio': ok / scheduled, 'forced_stop': False, 'cycle': cycle})
        result = analyze.summarize(events)['node-only-arm64']
        self.assertEqual(result['highest_passing_rate'], 100)
        self.assertEqual(result['first_failure_rate'], 200)

    def test_task_stops_are_matched_to_the_latest_cycle_rate(self):
        events = [{'Variant': 'baml-only', 'Architecture': 'x64', 'rate': rate, 'log_timestamp': timestamp}
                  for rate, timestamp in ((100, 1000), (200, 2000), (300, 3000))]
        stops = [{'cell': 'baml-only-x64', 'log_timestamp': 2500}]
        self.assertEqual(analyze.attach_stop_rates(events, stops)[0]['rate'], 200)

    def test_task_stop_fails_an_otherwise_clean_rate(self):
        events = [{'Variant': 'baml-only', 'Architecture': 'x64', 'rate': 100, 'scheduled': 400, 'Http200': 400,
                   'TransportErrors': 0, 'HttpErrors': 0, 'BodyMismatches': 0, 'completion_ratio': 1, 'forced_stop': False}
                  for _ in range(6)]
        result = analyze.summarize(events, [{'cell': 'baml-only-x64', 'rate': 100}])['baml-only-x64']
        self.assertIsNone(result['highest_passing_rate'])
        self.assertEqual(result['first_failure_rate'], 100)



if __name__ == '__main__':
    unittest.main()
