// 18a §"Type shapes / type alias / recursive": "RecList used as `field: RecList`
// from a different leaf → `RecList` (opaque, routed name)".
//
// BAML requires absolute `root.<ns>.<name>` for cross-namespace refs;
// bare `aliases.RecList` is treated as namespace-relative and would fail
// to resolve.

class RecListConsumer {
  field root.aliases.RecList
}
