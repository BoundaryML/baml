// Default-parameter codegen coverage. Pure (non-LLM) function bodies
// keep this test fast and free of mock clients — the goal here is to
// pin the *emitted* `_define_function(..., required_positional_count)`
// shape and the `.pyi` default markers, not the engine's evaluator.
//
// Per BEP-033, the codegen rule for `.pyi` defaults is:
//   - Literal defaults (scalars, null, empty list/map) project directly
//     into the host language signature so callers see the actual value.
//   - Expression defaults render as the stub ellipsis placeholder,
//     because the host can't evaluate BAML expressions — only the BAML
//     runtime can. Runtime omission still uses the Python sentinel.
//
// Cases:
//   score              — one required, one scalar default   → `= 10` in .pyi
//   with_optional      — single null default                 → `= None` in .pyi
//   with_list          — single empty-list default           → `= []` in .pyi
//   all_required       — no defaults                         → no `*,` marker
//   with_expr_default  — non-empty array default             → `= ...` in .pyi
//   nullable_with_expr — nullable + expression default       → `= ...` in .pyi
//                        (the BEP's "trickiest case" — caller must be
//                        able to distinguish "not passed" from "passed null")

function score(query: string, max_results: int = 10) -> int {
  max_results
}

function with_optional(value: string? = null) -> string? {
  value
}

function with_list(items: int[] = []) -> int {
  items.length()
}

function all_required(a: int, b: int) -> int {
  a + b
}

function with_expr_default(items: int[] = [1, 2, 3]) -> int {
  items.length()
}

function nullable_with_expr(items: int[]? = [1, 2]) -> int? {
  null
}
