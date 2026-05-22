// Root-namespace user type. 18a §"Routing rules" routes root types via
// `route_class_ref` with a leaf of either `<empty>` (true root use) or a
// non-root leaf (which fires the root-leaf shortcut). `Foo` is the
// canonical root-namespaced type referenced from each leaf below.

class Foo {
  v int
}

// Same-file, same-leaf (root): bare `Foo` → `Foo`.
function MakeFoo(v: int) -> Foo {
  Foo { v: v }
}

function RoundTripFoo(f: Foo) -> Foo {
  f
}
