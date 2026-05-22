// Union rows from 18a §"Type shapes":
//   - null-to-end ordering: `int | null | string` → `Union[int, str, None]`
//   - dedup of structurally equal: `int | int | string` → `Union[int, str]`
//   - singleton-union unwrap (after dedup): `int | int` → `int`
//   - Optional + explicit null in union: `T? | string` → `Union[T, str, None]`
//
// Nested-union flattening is not directly writable in BAML source — the
// converter handles it from inputs only it produces. We verify it via
// a class field whose declared shape lowers through the flatten path.

class T {
  v int
}

class UnionContainer {
  null_to_end int | null | string
  dedup int | int | string
  singleton_unwrap int | int
  optional_plus_null T? | string
}

function RoundTripNullToEnd(u: int | null | string) -> int | null | string {
  u
}

function RoundTripDedup(u: int | int | string) -> int | int | string {
  u
}

// After dedup the union collapses to a single `int`; codegen renders this
// as plain `int`, not `typing.Union[int]`.
function RoundTripSingletonUnwrap(u: int | int) -> int | int {
  u
}

function RoundTripOptionalPlusNull(u: T? | string) -> T? | string {
  u
}

function RoundTripT(t: T) -> T {
  t
}

function RoundTripUnionContainer(c: UnionContainer) -> UnionContainer {
  c
}
