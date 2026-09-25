import json
import os
import sys
import unittest
from urllib.request import Request, urlopen

from telemetry_resources import MacSampler, MockCloud, WORKLOADS


class ResourceProbeTests(unittest.TestCase):
    @unittest.skipUnless(sys.platform == "darwin", "macOS sampler")
    def test_mach_cpu_conversion_matches_process_cpu_time(self):
        sampler = MacSampler()
        calibration = sampler.calibrate()
        self.assertAlmostEqual(
            calibration["process_time_s"], calibration["libproc_cpu_s"], delta=0.005
        )
        self.assertGreater(sampler.read(os.getpid())["rss_bytes"], 0)

    def test_reference_work_units_and_idle(self):
        self.assertEqual(WORKLOADS["spawn"][1:4], (64, 0, 2048))
        self.assertEqual(WORKLOADS["async"][1:4], (4, 0, 128))
        self.assertEqual(WORKLOADS["burst"][1:4], (4, 200, 128))
        self.assertEqual(WORKLOADS["calls"][1:4], (100000, 0, 100000))

    def test_mock_is_separate_and_preserves_target_membership(self):
        cloud = MockCloud(0)
        try:
            self.assertNotEqual(cloud.process.pid, os.getpid())
            request = {
                "recording": {"recording_id": "abc", "recording_file_sequence": 7},
                "candidates": [{}, {}, {}],
                "proposed_uploads": [
                    {"client_target_id": 0, "kind": "recording", "candidate_indices": [0]},
                    {"client_target_id": 1, "kind": "cas_batch", "candidate_indices": [1]},
                    {"client_target_id": 2, "kind": "cas_object", "candidate_indices": [2]},
                ],
            }
            with urlopen(Request(
                cloud.url + "/v1/recordings/abc/uploads:prepare",
                data=json.dumps(request).encode(),
                headers={"Content-Type": "application/json"},
            ), timeout=5) as response:
                plan = json.load(response)
            for target, expected in zip(plan["uploads"], request["proposed_uploads"]):
                self.assertEqual(target["candidate_indices"], expected["candidate_indices"])
                with urlopen(Request(target["presigned_put_url"], data=b"body", method="PUT"), timeout=5) as response:
                    self.assertEqual(response.status, 200)
            stats = cloud.stats()
            self.assertEqual(stats["acknowledged_bytes"], 12)
            self.assertEqual(stats["acknowledged_puts"], 3)
            self.assertEqual(stats["offered_candidates"], 3)
        finally:
            cloud.close()


if __name__ == "__main__":
    unittest.main()
