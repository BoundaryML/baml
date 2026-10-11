Question
  What does building and dropping request-scoped cyclic object graphs cost,
  and what does the runtime do with the cycles?

Work
  For each of `requests` requests, build a graph of `nodes` nodes: each node
  has an edges array with a ring edge and a chord, and a parent pointer, so
  every node lies on a cycle. Walk it once (sum of neighbour values and
  parent ids) and drop it. Fold the per-request sums into a checksum.

BAML path
  Allocation of class instances and arrays, field stores that close cycles,
  array iteration. Natively `Node` and `Node[]` are on a cycle, so their
  handles are tracked: dropping a request's graph leaves its nodes as
  buffered possible roots, and the cycle collector frees them once the
  buffer reaches its threshold, inside the timed region. Plain reference
  counting would leak every graph (2 * nodes objects per request), so peak
  RSS staying flat across jobs is the reclamation signal. The VM's
  collector frees them too.

Boundary
  Only the two sizes are prepared. Allocation, linking, walking and
  dropping every request's graph are timed, as is serializing the result.
  Garbage not yet collected at the end of a job is carried into the next
  one, as a leak would be.

Evidence
  The oracle recomputes every request's sum in Python and folds the
  checksum the same way; the result also carries requests * nodes. Checks
  cover zero requests, zero nodes, one node and small graphs.

Claim
  Allocation and linking cost of a cyclic workload, and (from peak RSS) the
  runtime's ability to reclaim cycles: peak RSS growth with jobs is a leak.

References
  https://nim-lang.org/docs/mm.html
