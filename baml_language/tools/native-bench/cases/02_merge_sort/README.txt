Question
  How well does user-written control flow execute?

Work
  Bottom-up integer merge sort with identical tie behavior. Each job copies
  the original input, allocates scratch, sorts and serializes the full sequence.

BAML path
  Branches, nested loops, indexing, array mutation, allocation and dispatch.

Boundary
  Input parsing is outside execution. Copy, scratch, sort and serialization
  are inside. Test random/sorted/reverse/duplicate inputs and non-powers of two.

Evidence
  Entire output equals an independent reference sort; the input remains
  unchanged. GC coverage is observed, not assumed from temporary arrays.

Claim
  Whole serialized sort-job cost. This is neither a native library sort
  comparison nor isolated instruction-dispatch timing.

References
  https://github.com/BoundaryML/baml/blob/340b9085d9427e6d458f1c700409ffc88c58b050/baml_language/crates/baml_compiler2_mir/src/lower.rs
  https://github.com/BoundaryML/baml/blob/340b9085d9427e6d458f1c700409ffc88c58b050/baml_language/crates/bex_vm/src/vm.rs
  https://github.com/BoundaryML/baml/blob/340b9085d9427e6d458f1c700409ffc88c58b050/baml_language/crates/bex_heap/src/gc_policy.rs
