import base64
import importlib.util
import json
from pathlib import Path
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


load = module('load', 'load-generator/run.py')



class HarnessTest(unittest.TestCase):
    def test_emf_bounded_batches_and_errors(self):
        lines = []
        c = load.Collector({'RunName': 'test', 'Variant': 'node-only', 'Architecture': 'arm64'}, lines.append)
        for _ in range(205):
            c.record({'code': 200, 'latency': 1_000_000, 'body': base64.b64encode(b'hello world').decode()})
        c.record({'code': 0, 'latency': 10_000_000, 'body': None})
        c.record({'code': 503, 'latency': 2_000_000, 'body': None})
        c.record({'code': 200, 'latency': 1_000_000, 'body': base64.b64encode(b'wrong').decode()})
        c.flush()
        events = [json.loads(line) for line in lines]
        self.assertEqual([len(e['LatencyMs']) for e in events if 'LatencyMs' in e], [100, 100, 8])
        counters = next(e for e in events if 'Requests' in e)
        self.assertEqual([counters[k] for k in load.COUNTERS], [208, 206, 1, 1, 1])
        self.assertTrue(all('RunName' in e for e in events))
        with patch.object(load.urllib.request, 'urlopen', side_effect=OSError('stopped')):
            c.scrape('http://stopped/metrics')
        self.assertEqual(json.loads(lines[-1])['ProcessMetricsUp'], 0)
        self.assertNotIn('ProcessRssBytes', json.loads(lines[-1]))



if __name__ == '__main__':
    unittest.main()
