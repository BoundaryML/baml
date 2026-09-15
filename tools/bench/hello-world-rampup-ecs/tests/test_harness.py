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



class HarnessTest(unittest.TestCase):
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
