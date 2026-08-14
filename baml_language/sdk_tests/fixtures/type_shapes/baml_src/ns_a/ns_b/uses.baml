// 18a §"Routing rules", deep namespace, same leaf:
//   - bare `Thing` from `a/b/uses.baml`              → `Thing`
//   - `root.Foo` from a deeper leaf (`a/b`)           → `Foo` (root-leaf shortcut still fires)

function round_trip_thing_from_ab(t: Thing) -> Thing {
  t
}

function round_trip_root_foo_from_ab(f: root.Foo) -> root.Foo {
  f
}
