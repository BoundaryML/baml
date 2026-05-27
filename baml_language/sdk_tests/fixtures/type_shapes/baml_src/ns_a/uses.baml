// 18a §"Routing rules", partial-prefix case:
//   - `root.a.b.Thing` from `a/uses.baml` → `a.b.Thing` (full leaf path
//     emitted because the leaves differ — leaf `a` is one level above the
//     declaring leaf `a/b`).
//
// BAML cross-namespace refs use the absolute `root.<ns>.<name>` form;
// the bare `b.Thing` form 18a hints at would be namespace-relative and
// resolve to a sibling under `a` that doesn't exist.

function round_trip_deep_thing_from_a(t: root.a.b.Thing) -> root.a.b.Thing {
  t
}
