# BAML 0.24.0 measurement data

This directory contains the data behind [the release performance audit](../changelog-0.24.0-performance.md). Measurements compare published 0.23.0 and 0.24.0 macOS ARM64 compiler/CFFI artifacts and matching Python wheels. The audit records the workloads, timing boundaries, build profile, hardware, and limitations.

- [benchmark-results.json](benchmark-results.json): five interleaved release pairs for strings, enum comparisons, n-body, and Python call overhead.
- [array-results.json](array-results.json): the separate five-pair array measurement.
- [summary.json](summary.json): unrounded medians, ranges, changes, and checksums derived from the raw samples.
- [environment.json](environment.json): release commits, artifact fingerprints and sizes, environment details, and historical hashes of the original measurement files.

The benchmark runners and copied inputs have been removed. The historical hashes still identify those original files; they are not an inventory of files retained here. The workload sources are available in the [pinned BAML source](https://github.com/BoundaryML/baml/tree/dec4713a0e7c9699442bf5855b4f327063f769c5/baml_language/tools/speedtest/workloads).

These are interpreter and bridge measurements. They exclude process startup and compilation, do not measure provider latency, and do not isolate individual PR contributions. Preserve the ranges and control rows: the sync no-op median was slower, and sequential async ranges overlapped.
