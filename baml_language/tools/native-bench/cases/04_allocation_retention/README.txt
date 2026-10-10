Question
  What does allocation cost as the surviving object graph grows?

Work
  Allocate cells containing four integers, mutate through aliases, and keep
  a bounded ring of batches reachable. Verify total and retained sums.

BAML path
  TLAB reservations, allocation budget, safepoint parking, root scanning,
  copying survivors and repairing references.

Boundary
  Allocate, mutate, traverse, replace retained batches and serialize sums
  inside each job. Retention spans batches within a job; it resets between jobs.

Evidence
  For a sustained-GC claim, observe repeated natural full collections inside
  execution and a bounded memory pattern. Shutdown-only GC does not qualify.

Claim
  Allocation under the stated retention pattern. Sweep churn at fixed
  retention, then retention at fixed churn. Slots exclude backing payload bytes;
  RSS is not live heap. Rust preserves aliasing with Rc<RefCell<Cell>>.
  Higher retention also increases the final checksum traversal. A timing change
  therefore does not isolate the collector's copying cost.

References
  https://github.com/BoundaryML/baml/blob/340b9085d9427e6d458f1c700409ffc88c58b050/baml_language/crates/bex_heap/src/tlab.rs
  https://github.com/BoundaryML/baml/blob/340b9085d9427e6d458f1c700409ffc88c58b050/baml_language/crates/bex_heap/src/gc_policy.rs
  https://github.com/BoundaryML/baml/blob/340b9085d9427e6d458f1c700409ffc88c58b050/baml_language/crates/bex_heap/src/gc.rs
  https://github.com/BoundaryML/baml/blob/340b9085d9427e6d458f1c700409ffc88c58b050/baml_language/crates/bex_engine/src/bex_work.rs
