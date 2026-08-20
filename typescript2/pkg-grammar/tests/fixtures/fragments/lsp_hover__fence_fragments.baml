// Hover-fence fragment shapes: member lines, bare owner/receiver types, and
// a `let` binding, exactly as the LSP emits them inside ```baml fences. Not
// valid top-level BAML — this fixture locks the fence-fragment friendliness
// rules (member-fragment / type-fragment). It lives under `fragments/` so
// the tree-sitter port's whole-file parse gate (top-level `*.baml` only)
// skips it while the recursive TextMate/hljs harnesses still cover it.
// A real hover fence is its own
// one-fragment document; here the multi-line `let` context sits last so the
// `$`-terminated fragment lines above it stay at the root context.
asdf: Box<int>
label?: string
name: string
Active: Status
T[]
int[]
map<K, V>
user.util.Widget<T>
baml.future.Future<V, E>
user
string
let turns: int
