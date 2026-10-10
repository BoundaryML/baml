Question
  What does a source-level loop of small function calls cost?

Work
  Repeatedly call leaf(value) = value + 1 using a runtime-supplied seed/count.
  Return the exact final value. The same source algorithm runs in every language.

BAML path
  Call lowering, argument/return handling, VM dispatch and function telemetry.

Boundary
  Prepared input -> loop -> decimal result. Function inlining and loop elimination
  are allowed optimizations. Inspect generated code before claiming actual calls.

Evidence
  Result equals seed + count. Medium/high BTEL records expose function completion
  totals. The reader checks run() totals; leaf-call attribution requires inspecting
  its recorded totals and emitted code. Low/off supply no such call evidence.

Claim
  Cost of this source program under the selected optimizer and telemetry policy.
  No tracing-GC claim follows from this workload.

References
  https://github.com/BoundaryML/baml/blob/340b9085d9427e6d458f1c700409ffc88c58b050/baml_language/crates/bex_vm/src/vm.rs
  https://github.com/BoundaryML/baml/blob/340b9085d9427e6d458f1c700409ffc88c58b050/baml_language/crates/btel_settings/src/mode.rs
