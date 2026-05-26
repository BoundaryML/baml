// 18a §"Routing rules", deep namespace, same leaf:
//   - bare `Thing` from `a/b/uses.baml`              → `Thing`
//   - `root.Foo` from a deeper leaf (`a/b`)           → `Foo` (root-leaf shortcut still fires)

function RoundTripThingFromAB(t: Thing) -> Thing {
  t
}

function RoundTripRootFooFromAB(f: root.Foo) -> root.Foo {
  f
}
