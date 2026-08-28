# B-1649 repro: omitted class fields become `null`

This standalone BAML file characterizes B-1649 across scalar, nested,
collection, boolean-control-flow, and generic class fields.

From `baml_language/repros/b-1649` in the `vbv/b-1649` worktree, run:

```bash
cargo run --manifest-path ../../Cargo.toml -p baml_cli -- \
  run --file main.baml --output-format json
```

The result contains runtime `null` values in fields whose declared types do
not include `null`. Compare `baseline` with `valid_optional_control`: their JSON
shapes are identical even though only the latter permits those values.

The matrix also shows a silent control-flow failure: the missing field declared
as `bool` takes the true branch when its runtime `null` is used as a condition.

The problem is a type-soundness bug, not only a JSON rendering bug. This entry
point compiles because `attempts` is statically typed as `int`, but fails when
the integer addition consumes the runtime `null`:

```bash
cargo run --manifest-path ../../Cargo.toml -p baml_cli -- \
  run arithmetic_use_of_missing_int --file main.baml
```

Expected behavior after a fix: a literal must initialize every non-nullable
class field, directly or through a spread. Only fields whose declared type
includes `null` should be omittable and receive a `null` default.

The broader characterization matrix lives in
`crates/baml_tests/baml_src/ns_compiler/ns_class_constructors/ns_required_fields`.
These are ordinary, pure BAML tests whose assertions dynamically compile
isolated BAML snippets through `reflect.Package.compile`. Run them with the
standard test runner:

```bash
cargo run --manifest-path ../../Cargo.toml -p baml_cli -- test \
  --from ../../crates/baml_tests/baml_src \
  -i 'root.compiler.class_constructors.required_fields::*'
```

## What the adversarial matrix covers

The suite currently has 45 BAML cases, all discovered and executed directly by
`baml test`. The cases cross these seams:

- scalar, literal, enum, union, `null`, alias, recursive, nested, list, and map
  field types;
- explicit, inferred, multi-parameter, nested, nullable, phantom, and bounded
  generics;
- local, cross-file, and mounted-package class resolution;
- classes that implement interfaces;
- empty, shorthand, reordered, duplicate, complete, partial-spread, and
  complete-spread constructors;
- downstream observation through JSON, equality, `is null`, boolean control
  flow, methods, and statically specialized arithmetic;
- controls proving that explicit `null`, wrong field values, unknown fields,
  wrong spread classes, and uninferred generic arguments are still rejected.

This is deliberately a dimension matrix, not a claim of literal exhaustiveness.
The durable invariant for future generated/property tests is: after a class's
generic arguments are substituted, every field whose resulting type excludes
`null` must be supplied by the literal or by a valid spread.

## Compiler pipeline and root cause

1. Type inference validates every field that was written, including generic
   substitution, but never computes the set of required fields that was not
   written. The mounted-package constructor has the same omission.
2. MIR lowering allocates a slot for every declared field, initializes every
   slot to `null`, and overwrites the slots supplied by named fields or spreads.
   That initialization is necessary for legitimately nullable partial classes;
   it relies on inference having already rejected missing non-nullable fields.
3. The VM allocates the instance from those slots without rechecking every
   value against the declared field type. This is a normal optimization for a
   statically typed VM, but it makes the missing front-end check consequential.
4. JSON serialization merely reveals the invalid object. Other consumers can
   behave worse: a missing `bool` follows the true branch, and integer bytecode
   reaches an internal assertion because it trusts its operands are integers.

The legacy compiler contained the exact missing-required-field check. It
skipped the check when a spread was present, then rejected every absent field
whose type was not optional. The compiler2 checker never carried that rule
over, and the later inference rewrite preserved the omission. This makes the
likely implementation cause a localized porting oversight in constructor
validation, while the resulting behavior is a core type-soundness failure.

In compiler terms, the program violates **preservation**: inference assigns
`Job { id: 1 }` the non-nullable type `Job`, but evaluation produces an object
whose `label` and `attempts` values do not inhabit their declared types. Once
preservation is broken, later optimized stages are allowed to make assumptions
that are no longer true.
