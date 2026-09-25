"""Synthetic renderer tests; never launches benchmark processes."""

import csv
from pathlib import Path
import tempfile
import unittest
import xml.etree.ElementTree as ET

from telemetry_resource_report import _reference_metrics, _ratio, render_report


class ReportTests(unittest.TestCase):
    def test_render_excludes_failed_and_capped_runs(self):
        base = dict(
            workload="tiny", mode="off", trial=1, cpu_percent=100,
            peak_rss_bytes=1024, output_bytes=0, drain_ms=0.25,
            execute_ms=1000, work_per_s=10, work_unit="roots",
            telemetry_success=True, error=None,
        )
        runs = [
            base,
            dict(base, trial=2, cpu_percent=200, work_per_s=20),
            dict(base, trial=3, cpu_percent=9999, error="<failed>"),
            dict(base, mode="cloud-fast", telemetry_success=False),
            dict(base, mode="local", peak_rss_bytes=4096),
            dict(base, mode="auto-no-sink", cpu_percent=None),
        ]
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder)
            render_report(runs, {"memory_limit_bytes": 2048, "machine": "<host>"}, output, None)
            with (output / "summary.csv").open() as stream:
                rows = {(r["workload"], r["mode"]): r for r in csv.DictReader(stream)}
            self.assertEqual(rows["tiny", "off"]["cpu_percent_median"], "150.0")
            self.assertEqual(rows["tiny", "off"]["valid_runs"], "2")
            self.assertEqual(rows["tiny", "off"]["attempted_runs"], "3")
            self.assertEqual(rows["tiny", "cloud-fast"]["work_per_s_median"], "")
            self.assertEqual(rows["tiny", "local"]["valid_runs"], "0")
            self.assertEqual(rows["tiny", "auto-no-sink"]["cpu_percent_median"], "")
            document = (output / "report.html").read_text()
            self.assertIn("&lt;failed&gt;", document)
            self.assertIn("&lt;host&gt;", document)
            self.assertIn("One cold root", document)
            self.assertNotIn("<script", document)
            ET.fromstring((output / "overview.svg").read_text())

    def test_reference_uses_first_named_table_only(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "reference.html"
            path.write_text(
                "<table><tr><td>unrelated</td></tr></table>"
                "<h2>Work completed &amp; resource <span>use</span></h2>"
                "<table><tr><th>Workload / mode</th><th>Work / second</th>"
                "<th>CPU %</th><th>Peak RSS MiB</th></tr>"
                "<tr><td>Tiny roots - Off</td><td>1,200 roots</td><td>100.0</td><td>30.5</td></tr>"
                "<tr><td>Tiny roots - Auto - no sink</td><td>600 roots</td><td>150</td><td>31</td></tr>"
                "</table><table><tr><td>ignore</td></tr></table>"
            )
            metrics = _reference_metrics(path)
            self.assertEqual(len(metrics), 2)
            self.assertEqual(metrics["tiny", "off"]["work_per_s"], 1200)
            self.assertEqual(_ratio(metrics["tiny", "auto-no-sink"]["cpu_percent"],
                                    metrics["tiny", "off"]["cpu_percent"]), 1.5)
            self.assertIsNone(_ratio(1, 0))
            self.assertIsNone(_ratio(None, 1))

    def test_empty_runs_and_missing_reference(self):
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder)
            render_report([], {}, output, output / "missing.html")
            self.assertIn("Reference comparison unavailable", (output / "report.html").read_text())
            ET.fromstring((output / "overview.svg").read_text())


if __name__ == "__main__":
    unittest.main()
