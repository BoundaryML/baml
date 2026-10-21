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

# Type alias expansion

## alias_expansion

### aliases
X = int

### input
int | 5 | $X

### expected
int

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

# Stress tests — deep nesting, aliases

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
