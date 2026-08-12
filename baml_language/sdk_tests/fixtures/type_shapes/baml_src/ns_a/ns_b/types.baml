// 18a §"Routing rules" — `Thing` declared in `a/b/x.baml`. Referenced from
// three different leaves below: `a/b/y.baml` (same-leaf), `a/y.baml` (one
// level up — partial-prefix should NOT shortcut), and `lorem/y.baml`
// (different leaf entirely).

class Thing {
  v int
}
