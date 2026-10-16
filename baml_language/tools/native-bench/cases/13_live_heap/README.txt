Question
  What does ongoing allocation cost while a large object graph stays live?

Work
  prepare builds `live` items, each a class instance with a one-element tags
  array and a pointer to its heap-order parent (so the graph is a tree of
  depth log2(live): 2 * live + 1 objects). Each job then takes `churn` steps:
  pick a pseudo-random item (s = (s * 1103515245 + 12345) mod 2^31,
  x = (s / 256) mod live; an LCG mod `live` itself has period 32 at
  live = 500,000, so it would churn the same few items), replace it with
  an equal fresh item (the old one stays alive while its children point at
  it), allocate a short-lived item pointing at the fresh one, and walk four
  hops up the parent chain, folding values into a checksum.

BAML path
  Allocation with a large live heap: a tracing collector has to mark the
  whole graph on every full collection, so its pauses grow with `live`;
  reference counting pays per store and per dropped handle instead. `Item`
  can reach itself through `next`, so natively it is tracked: a
  handle dropped to a nonzero count (a walk step, a replaced item) is
  buffered, and the cycle collector's trial deletion traces what those
  roots reach, in short pauses. Replacing items mutates the old graph, so
  the live set is not frozen.

Boundary
  The graph is built in prepare, outside the timed region, and lives
  across every job of the process. Each job's churn (allocation,
  replacement, walking) and serializing the result are timed. Replaced
  items keep equal values, so every job computes the same result.

Evidence
  The oracle recomputes the checksum in Python from the item values
  (i * 7 + seed) mod 1000 and the heap-order parents; the result carries
  the item count. Checks cover an empty graph, one item and small graphs.

Claim
  Throughput of allocation and pointer-chasing at a given live-heap size,
  and with peak RSS, the memory a runtime needs to hold it. Pause times
  need a collector's own counters; job time shows their total.

References
  https://www.memorymanagement.org/
