Question
  What is the cost of traversing an integer array?

Work
  Sum an immutable array. Iterator and indexed variants read prepared input;
  build additionally copies by repeated append inside each job.

BAML path
  Iterable-for MIR lowering, ArrayIterator.next, array access and VM dispatch.

Boundary
  Input parsing is outside execution. Sum and decimal result conversion are
  inside. Sweep lengths to separate entry overhead from per-element work.

Evidence
  Exact sum; inspect emitted code before attributing a change to iterator
  lowering. GC observation is reported but not required.

Claim
  Traversal cost for immutable inputs. Mutable/aliased iteration semantics
  require separate correctness checks; this case is not a collector benchmark.

References
  https://github.com/BoundaryML/baml/blob/340b9085d9427e6d458f1c700409ffc88c58b050/baml_language/crates/baml_compiler2_mir/src/lower.rs
  https://github.com/BoundaryML/baml/blob/340b9085d9427e6d458f1c700409ffc88c58b050/baml_language/crates/baml_builtins2/baml_std/baml/ns_iter/iter.baml
