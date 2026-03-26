# SAP Type Simplification Tests

## Format

Each test has a `## name`, optional `### aliases`, an `### input` type, and
`### expected` output type — all in a small DSL:

- Primitives: `int`, `float`, `string`, `bool`, `null`
- Literals: `5`, `true`, `false`
- Class refs: `MyClass` (any capitalized identifier)
- Type alias refs: `$X`
- Containers: `T[]`, `T?`, `map<K, V>`
- Unions: `A | B`
- Grouping: `(A | B)`
- SAP attrs: `@sap.parse_without_null`, `@sap.pending_never`, `@sap.in_progress_never`
- Asserts: `@assert((_) => f1)` where `f1` maps to func_idx=1

Attrs bind to the immediately preceding type. Use parens for union-level attrs:
`(int | string) @sap.parse_without_null`.

Alias defs use `Name = type` syntax, one per line.

---

# Primitives — passthrough

## basic_int

### input
int

### expected
int

---

## basic_string

### input
string

### expected
string

---

## basic_float

### input
float

### expected
float

---

## basic_bool

### input
bool

### expected
bool

---

## basic_null

### input
null

### expected
null

---

## basic_class

### input
MyClass

### expected
MyClass

---

# Unions — structural

## simple_union

### input
MyA | int

### expected
MyA | int

---

## dedup_identical

### input
int | int

### expected
int

---

## literal_subtype_dedup

### input
int | 5

### expected
int

---

## union_of_unions

### input
(int | bool) | float

### expected
int | bool | float

---

## null_to_end

### input
null | int

### expected
int | null

---

## null_already_at_end

### input
int | null

### expected
int | null

---

## dedup_then_unwrap

### input
int | int | int

### expected
int

---

## nested_union_dedup

### input
int | (int | null) | string

### expected
int | string | null

---

# SAP flag passthrough on simple types

## pending_never_passthrough

### input
int @sap.pending_never

### expected
int @sap.pending_never

---

## in_progress_never_passthrough

### input
int @sap.in_progress_never

### expected
int @sap.in_progress_never

---

## parse_without_null_passthrough

### input
int @sap.parse_without_null

### expected
int @sap.parse_without_null

---

# SAP flags — dedup in unions

## dedup_parse_without_null

### input
int @sap.parse_without_null | int

### expected
int

---

## dedup_equal_pending_never

### input
int @sap.pending_never | int @sap.pending_never

### expected
int @sap.pending_never

---

## literal_not_subtype_of_narrower_attr

5 (default attr) is structurally a subtype of int, but int has
`@sap.parse_without_null` (narrower). Since Unset is NOT ≤ Set,
`5` is not a subtype of `int @sap.pw` and both survive.

### input
int @sap.parse_without_null | 5

### expected
int @sap.parse_without_null | 5

---

# Asserts — dedup in unions

## assert_subtype_of_no_assert

### input
int @assert((_) => f1) | int

### expected
int

---

## dedup_equal_asserts

### input
int @assert((_) => f1) | int @assert((_) => f1)

### expected
int @assert((_) => f1)

---

## incomparable_asserts

### input
int @assert((_) => f1) | int @assert((_) => f2)

### expected
int @assert((_) => f1) | int @assert((_) => f2)

---

# Type alias expansion

## alias_expansion

### aliases
X = int

### input
int | 5 | $X

### expected
int

---

## alias_with_sap_attrs

### aliases
X = int @sap.parse_without_null

### input
$X | 5 | int @sap.parse_without_null

### expected
int @sap.parse_without_null | 5

---

# SAP attr distribution into unions

## distribute_parse_without_null

### input
(int | string) @sap.parse_without_null

### expected
(int @sap.parse_without_null | string @sap.parse_without_null) @sap.parse_without_null

---

## distribute_pending_never

### input
(int | string) @sap.pending_never

### expected
(int @sap.pending_never | string @sap.pending_never) @sap.pending_never

---

## distribute_in_progress_never

### input
(int | string) @sap.in_progress_never

### expected
(int @sap.in_progress_never | string @sap.in_progress_never) @sap.in_progress_never

---

## distribute_sap_idempotent_with_different_asserts

### input
(int @sap.parse_without_null @assert((_) => f1) | int @sap.parse_without_null @assert((_) => f2)) @sap.parse_without_null

### expected
(int @sap.parse_without_null @assert((_) => f1) | int @sap.parse_without_null @assert((_) => f2)) @sap.parse_without_null

---

# Assert distribution into unions

## distribute_asserts_into_union

### input
(5 @assert((_) => f1) | 6 @assert((_) => f2)) @assert((_) => f0)

### expected
5 @assert((_) => f1) @assert((_) => f0) | 6 @assert((_) => f2) @assert((_) => f0)

---

# Compound types — recurse into children

## list_inner_simplified

### input
(int | int)[]

### expected
int[]

---

## optional_inner_simplified

### input
(int | int)?

### expected
int | null

---

## map_value_simplified

### input
map<string, int | int>

### expected
map<string, int>

---

# Edge cases

## all_null_union

### input
null | null

### expected
null

---

## triple_dedup_with_literal

### input
int | 5 | 5

### expected
int

---

## nested_union_with_attrs_flatten

### input
(int @sap.pending_never | bool) | float

### expected
int @sap.pending_never | bool | float

---

# Stress tests — deep nesting, attr combos, aliases

## int_and_float_not_deduped

SAP treats int and float as distinct parse targets despite
int being a structural subtype of float.

### input
int | float

### expected
int | float

---

## deep_nested_union_flatten

### input
((int | bool) | (string | null)) | float

### expected
int | bool | string | float | null

---

## same_literal_narrower_subsumed

`5 @sap.pw` (narrower) is a subtype of `5` (wider) — gets dropped.

### input
5 @sap.parse_without_null | 5

### expected
5

---

## incomparable_sap_flags_same_literal

`@sap.pw` and `@sap.pn` are orthogonal — neither variant
is narrower-than-or-equal-to the other.

### input
5 @sap.parse_without_null | 5 @sap.pending_never

### expected
5 @sap.parse_without_null | 5 @sap.pending_never

---

## literal_with_assert_subsumed_by_base

`5 @assert(f1)` is narrower than bare `int` — dropped.

### input
5 @assert((_) => f1) | int

### expected
int

---

## literal_first_not_subsumed_by_narrower_base

`5 (def)` is structurally subtype of `int`, but `int` has
`@sap.pw` (narrower). `flag_leq(Unset, Set)` is false, so
`5` is NOT ≤ `int @sap.pw`. Neither subsumes the other.

### input
5 | int @sap.parse_without_null

### expected
5 | int @sap.parse_without_null

---

## multiple_literals_with_base_type

All literals get subsumed by the base.

### input
5 | int | 6

### expected
int

---

## only_literals_no_base

Different literals are incomparable — no dedup.

### input
5 | 6 | 7

### expected
5 | 6 | 7

---

## alias_chain

### aliases
X = $Y
Y = int

### input
$X

### expected
int

---

## alias_to_union_with_outer_assert

Assert distributes into the expanded union variants.

### aliases
X = int | string

### input
$X @assert((_) => f1)

### expected
int @assert((_) => f1) | string @assert((_) => f1)

---

## alias_to_union_with_outer_sap

SAP flag distributes into expanded union variants and stays
at union level.

### aliases
X = int | string

### input
$X @sap.parse_without_null

### expected
(int @sap.parse_without_null | string @sap.parse_without_null) @sap.parse_without_null

---

## nested_union_assert_distribution

Inner union gets assert f1 distributed into 5 and 6.
Outer union then distributes assert f2 into all three variants
(including the already-flattened inner variants).

### input
((5 | 6) @assert((_) => f1) | 7) @assert((_) => f2)

### expected
5 @assert((_) => f1) @assert((_) => f2) | 6 @assert((_) => f1) @assert((_) => f2) | 7 @assert((_) => f2)

---

## distribute_sap_into_already_set_variant

`or(Set, Set) = Set` — distributing @sap.pw into a variant
that already has it is idempotent.

### input
(int @sap.parse_without_null | string) @sap.parse_without_null

### expected
(int @sap.parse_without_null | string @sap.parse_without_null) @sap.parse_without_null

---

## alias_with_sap_and_outer_sap

Alias body carries @sap.pw. Use site adds @sap.pn.
Nesting merges both flags.

### aliases
X = int @sap.parse_without_null

### input
$X @sap.pending_never

### expected
int @sap.parse_without_null @sap.pending_never

---

## union_dedup_both_directions

When the wider type (int) appears second, the literals before
it should be dropped AND it should not be dropped by them.

### input
5 | 6 | int | 5

### expected
int

---

## optional_inner_null_reordered

Optional wraps a union — inner null moves to end.

### input
(null | int)?

### expected
int | null

---

## list_inner_dedup

Dedup happens inside list element types.

### input
(int | 5)[]

### expected
int[]

---

## map_key_simplified

Dedup also applies to map key types.

### input
map<int | int, string>

### expected
map<int, string>

---

## three_level_nested_flatten

Three levels of union nesting all flatten into one.

### input
((int | bool) | (string | float)) | (null | MyClass)

### expected
int | bool | string | float | MyClass | null

---

## assert_and_sap_together_on_literal

A literal with both an assert and a SAP flag — the assert
makes it narrower than the bare base type, so it gets dropped.

### input
5 @sap.parse_without_null @assert((_) => f1) | int

### expected
int

---

## assert_and_sap_together_base_has_sap_too

Both variants have @sap.pw. The asserted variant is narrower
(has asserts, other doesn't). Gets deduped.

### input
int @sap.parse_without_null @assert((_) => f1) | int @sap.parse_without_null

### expected
int @sap.parse_without_null

---

## distribute_mixed_sap_and_asserts

Union-level has both a SAP flag and asserts. SAP flag
distributes (or) and stays. Asserts distribute (concat)
and are removed from union level.

### input
(int | string) @sap.pending_never @assert((_) => f1)

### expected
(int @sap.pending_never @assert((_) => f1) | string @sap.pending_never @assert((_) => f1)) @sap.pending_never

---

## flatten_with_inner_union_attrs_merged

Inner union has @sap.pw at union level. When flattened,
that flag merges into each inner variant.

### input
(int | bool) @sap.parse_without_null | string

### expected
int @sap.parse_without_null | bool @sap.parse_without_null | string

---
