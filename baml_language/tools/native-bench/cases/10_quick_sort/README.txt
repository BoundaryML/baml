Three-way quicksort

Question
  How do indexed loads, swaps and data-dependent branches behave across runtimes?
Work
  Copy a prepared integer array, partition around its middle element, and sort
  the less-than and greater-than partitions using an explicit stack. Equal
  elements are removed from further partitioning. Return the full sorted array.
Boundary
  Random generation and input parsing occur in prepare/controller setup. Copy,
  stack allocation, sorting and full JSON serialization are inside each job.
BAML path
  Integer arithmetic, branches, loops, mutable array indexing and native JSON
  encoding. The sort is handwritten BAML; no native sort implementation is called.
Evidence
  Compare every returned element with the independent Python sorted oracle.
  Check empty, singleton, random, ascending, descending and duplicate-heavy data.
  Merge sort uses the same fixture generator, seed, size and value distributions.
Claim
  Algorithm-plus-serialization comparison, not isolated comparison/swap latency.
  Three-way partitioning handles repeated values; this is not a claim about all
  quicksort variants or worst-case complexity on arbitrary adversarial inputs.
References
  https://github.com/BoundaryML/baml/blob/340b9085d9427e6d458f1c700409ffc88c58b050/baml_language/crates/baml_builtins2/baml_std/baml/containers.baml
  https://algs4.cs.princeton.edu/23quicksort/
