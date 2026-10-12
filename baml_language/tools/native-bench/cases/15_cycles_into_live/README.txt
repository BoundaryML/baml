Question
  What does dropping request-scoped cycles cost when each one points into a
  large graph that stays live, and does the runtime leave that graph alone?

Work
  `prepare` builds a ring of `live` nodes, kept for the process. Each job
  runs `requests` requests: a two-node cycle whose second node also holds
  an edge to one node of the ring. The request sums its edges' values and
  drops the cycle. Fold the per-request sums into a checksum.

BAML path
  Class instances and arrays, field stores that close cycles, an array
  store of a live node into a short-lived array. Natively `Node` is
  tracked, so every request leaves buffered possible roots whose trial
  deletion walks from the garbage into the live ring. The collector must
  free the cycles and give back the counts it took from the live nodes
  they point at; freeing a cycle that points at live data is the case
  12_cyclic_churn (no live data) and 13_live_heap (no cycles) do not
  cover. Plain reference counting would leak every request's cycle.

Boundary
  The ring is prepared. Building, walking and dropping every request's
  cycle is timed, as is serializing the result. Garbage not yet collected
  at the end of a job is carried into the next one.

Evidence
  The oracle recomputes every request's sum in Python and folds the
  checksum the same way; the result also carries the ring's size. Checks
  cover an empty ring, no requests, a one-node ring and small rings.

Claim
  The cost of collecting cycles that reference a live heap, the pause
  that comes from retracing it, and (from peak RSS) whether the cycles are
  reclaimed.
