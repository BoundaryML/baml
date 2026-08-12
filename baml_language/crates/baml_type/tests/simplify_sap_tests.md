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

## flatten_with_inner_union_attrs_merged

Inner union has @sap.pw at union level. When flattened,
that flag merges into each inner variant.

### input
(int | bool) @sap.parse_without_null | string

### expected
int @sap.parse_without_null | bool @sap.parse_without_null | string

---
