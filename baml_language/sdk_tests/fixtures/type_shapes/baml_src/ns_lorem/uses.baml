// 18a §"Routing rules" — leaf `lorem`, exercises:
//   - bare `Resume` from same-leaf (lorem)            → `Resume`
//   - `root.Foo` from non-root leaf                   → `Foo` (root-leaf shortcut)
//   - stdlib `baml.http.Response` from non-root leaf  → `baml.http.Response`
//
// 18a's "bare `Foo` (if visible via scope)" case from §"Routing rules /
// root-namespace user types" is intentionally not exercised: BAML's type
// resolver only looks up bare names within the current namespace, so a
// root-level `Foo` is never reachable as `Foo` from inside `lorem`. That
// case in 18a is misleading; the absolute `root.Foo` form covered above
// is the only correct way to write it.

function RoundTripResume(r: Resume) -> Resume {
  r
}

function RoundTripRootFoo(f: root.Foo) -> root.Foo {
  f
}

function RoundTripHttpResponse(r: baml.http.Response) -> baml.http.Response {
  r
}
