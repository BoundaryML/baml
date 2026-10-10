Question
  What does one correct invocation cost?

Work
  Read a runtime-supplied integer and return its decimal representation.

BAML path
  Engine creation, input conversion, call entry, result conversion and shutdown.

Boundary
  Fresh process, zero warmup, one job. Show whole-process wall/CPU/RSS alongside
  launch-to-ready and execution. Do not treat adapter startup as minimal hello-world.

Evidence
  Correct returned value; GC and telemetry counters attributed to their phase.

Claim
  Fixed lifecycle overhead for this feature bundle. A run without GC is valid
  here and does not establish sustained performance.

References
  https://github.com/BoundaryML/baml/blob/340b9085d9427e6d458f1c700409ffc88c58b050/baml_language/crates/bex_engine/src/lib.rs
  https://github.com/BoundaryML/baml/blob/340b9085d9427e6d458f1c700409ffc88c58b050/baml_language/crates/baml_pack_host/src/main.rs
