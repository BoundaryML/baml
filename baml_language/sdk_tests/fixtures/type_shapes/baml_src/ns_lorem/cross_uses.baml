// 18a §"Routing rules" — cross-leaf use of a deep namespace:
//   - `root.a.b.Thing` from `lorem/cross_uses.baml` → `a.b.Thing` (full path).
//
// BAML cross-namespace refs use the absolute `root.<ns>.<name>` form.

function round_trip_deep_thing_from_lorem(t: root.a.b.Thing) -> root.a.b.Thing {
  t
}
