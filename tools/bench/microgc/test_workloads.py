#!/usr/bin/env python3

import tempfile
import unittest
from pathlib import Path

from workloads import HEAP_STATS_FIELDS, WORKLOADS, write_workload


class WorkloadTests(unittest.TestCase):
    def test_all_workloads_emit_the_diagnostic_fields(self) -> None:
        for workload in WORKLOADS.values():
            source = workload.source()
            for field in HEAP_STATS_FIELDS:
                self.assertIn(f"stats.{field}", source)

    def test_map_has_one_thousand_hardcoded_insertions(self) -> None:
        source = WORKLOADS["float-map-1000"].source()
        self.assertEqual(source.count('let _ = values.set("k'), 1000)
        self.assertIn('values.set("k0000", 2.17)', source)
        self.assertIn('values.set("k0999", 2.17)', source)

    def test_ten_thousand_hardcoded_scalars_do_not_use_a_container(self) -> None:
        source = WORKLOADS["hardcoded-scalars-10000"].source()
        self.assertEqual(source.count("        let f"), 10000)
        self.assertIn("let f00000 = 0.0001;", source)
        self.assertIn("let f09999 = 1.0000;", source)
        self.assertNotIn("match (", source)
        self.assertNotIn("baml.Array", source)
        self.assertNotIn("map<", source)

    def test_generated_project_contains_exact_source(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            workload = WORKLOADS["unused-scalars-3"]
            app = write_workload(workload, Path(directory))
            self.assertEqual((app / "baml_src/main.baml").read_text(), workload.source())
            self.assertIn('name = "microgc-unused-scalars-3"', (app / "baml.toml").read_text())


if __name__ == "__main__":
    unittest.main()
