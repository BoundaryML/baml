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

function round_trip_null_to_end(u: int | null | string) -> int | null | string {
  u
}

function round_trip_dedup(u: int | int | string) -> int | int | string {
  u
}

// After dedup the union collapses to a single `int`; codegen renders this
// as plain `int`, not `typing.Union[int]`.
function round_trip_singleton_unwrap(u: int | int) -> int | int {
  u
}

function round_trip_optional_plus_null(u: T? | string) -> T? | string {
  u
}

// Differently-typed list arms. An empty list carries no item evidence, so a
// statically typed bridge annotates that selected child node with the exact
// list `value_type`. The round-trip pins that values land on the selected arm,
// rather than whichever list arm happens to be declared first.
function round_trip_str_or_int_list(x: string[] | int[]) -> string[] | int[] {
  x
}

function round_trip_t(t: T) -> T {
  t
}

function round_trip_union_container(c: UnionContainer) -> UnionContainer {
  c
}
