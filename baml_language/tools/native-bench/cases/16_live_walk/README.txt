Question
  What does reading a large live recursive structure cost when nothing is
  allocated and nothing becomes garbage?

Work
  `prepare` builds a binary tree of `nodes` nodes in heap order, each with
  a children array and a parent pointer, kept for the process. Each job
  walks it recursively `walks` times, summing the values, and folds each
  walk's sum into a checksum.

BAML path
  Field reads, array iteration and recursive calls only. Natively `TNode`
  is tracked (its type can reach itself), so every handle copy the walk
  drops while the node lives is a possible root: the collector buffers
  the walked nodes and its collections retrace the live tree, with no
  garbage to find. Under plain reference counting the walk only adjusts
  counts. The cost of tracking a type that can be cyclic but whose values
  never are.

Boundary
  The tree is prepared. The walks and serializing the result are timed.

Evidence
  The oracle recomputes the tree's sum and the checksum in Python; the
  result also carries the node count. Checks cover an empty tree, one
  node, zero walks and small trees.

Claim
  The overhead of cycle collection on read-only work over a live heap,
  which the collector's pause totals and the comparison with plain
  reference counting show.
