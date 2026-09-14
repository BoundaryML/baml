// Tests for optional / default function parameters.
//
// Converted from crates/baml_tests/tests/optional_function_parameters.rs.
//
// `host_call_omitted_default_and_explicit_null_are_distinct`: the Rust test
// exercises two host-call variants separately (omitted default, explicit null).
// In BAML both can be called inline since default-parameter omission and
// explicit null are ordinary call expressions.

// ─── source_defaults_named_overrides_and_required_by_name ────────────────────

function score(
  query: string,
  max_results: int = 10,
  filter: string? = null,
  chunk_size: int = query.length(),
) -> int {
  let filter_bonus = if (filter == null) { 0 } else { 100 }
  max_results + chunk_size + filter_bonus
}

function ofp_source_defaults_named_overrides_and_required_by_name() -> int[] {
  [
    score("cats"),
    score("cats", max_results = 5, filter = "recent", chunk_size = 2),
    score(query = "birds"),
  ]
}

test "source_defaults_named_overrides_and_required_by_name" {
    assert.equal(ofp_source_defaults_named_overrides_and_required_by_name(), [14, 107, 15])
}

// ─── mutable_default_array_is_fresh_per_call ─────────────────────────────────

function push_default(value: int, items: int[] = []) -> int {
  items.push(value)
  items.length()
}

function ofp_mutable_default_array_is_fresh_per_call() -> int {
  push_default(1) * 10 + push_default(2)
}

test "mutable_default_array_is_fresh_per_call" {
    assert.equal(ofp_mutable_default_array_is_fresh_per_call(), 11)
}

// ─── optional_dropping_adapter_uses_defaults_for_hidden_params ───────────────

function combine(x: int, a: int = 10, b: int = 100) -> int {
  (x * 100) + (a * 10) + b
}

function ofp_optional_dropping_adapter_uses_defaults_for_hidden_params() -> int {
  let f: (x: int, b?: int) -> int throws never = combine
  f(1, b = 5)
}

test "optional_dropping_adapter_uses_defaults_for_hidden_params" {
    assert.equal(ofp_optional_dropping_adapter_uses_defaults_for_hidden_params(), 205)
}

// ─── default_named_call_args_do_not_reuse_body_call_metadata ─────────────────

function pair(a: int, b: int) -> int {
  (a * 10) + b
}

function ofp_default_named_call_args_do_not_reuse_body_call_metadata(value: int = pair(b = 2, a = 1)) -> int {
  pair(a = value, b = 0)
}

test "default_named_call_args_do_not_reuse_body_call_metadata" {
    assert.equal(ofp_default_named_call_args_do_not_reuse_body_call_metadata(), 120)
}

// ─── host_call_omitted_default_and_explicit_null_are_distinct ────────────────

function classify(value: int? = 7) -> int {
  if (value == null) { 1 } else { value }
}

// omitted → uses default 7; explicit null → returns 1
test "host_call_omitted_default_returns_default" {
    assert.equal(classify(), 7)
}

test "host_call_explicit_null_returns_1" {
    assert.equal(classify(value = null), 1)
}
