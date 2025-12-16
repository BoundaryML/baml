# BAML `match` Expression (v3)

This page re-frames the `match` design in the Diátaxis style. The **Reference** section captures the normative specification: grammar, typing, evaluation, and diagnostics. The **Explanation** section records the rationale, constraints, and open questions that informed those choices.

This version (v3) incorporates the syntax decisions from the original proposal (v1), specifically using `variable: Type` for pattern matching.

---

## Reference (facts & work support)

### Summary

- `match` is an expression that evaluates to the value produced by the first arm whose pattern (and optional guard) succeeds.
- Arms are evaluated top-to-bottom; later arms are unreachable if an earlier pattern covers the same space without a guard.
- Patterns can bind identifiers; each binding is scoped to its arm body.
- Exhaustiveness is enforced for closed types (enums, unions, literal unions, classes). Use `_` or an unqualified binding pattern to opt in to “everything else.”
- `return` inside an arm exits the **function**, not just the `match`. To produce a value from the arm, yield an expression (single expression or the last expression of a block).

### Grammar (EBNF)

```ebnf
match-expression ::= "match" "(" expression ")" match-block
match-block      ::= "{" match-arm+ "}"
match-arm        ::= pattern guard? "=>" arm-body
guard            ::= "if" expression        // must be bool
arm-body         ::= expression | block     // block is `{ ... }`

pattern ::= wildcard-pattern
          | binding-pattern
          | typed-binding-pattern
          | literal-pattern
          | enum-variant-pattern
          | destructuring-pattern

wildcard-pattern        ::= "_" ( ":" type-name )?
binding-pattern         ::= identifier
typed-binding-pattern   ::= identifier ":" type-name
literal-pattern         ::= literal
enum-variant-pattern    ::= type-name "." identifier
destructuring-pattern   ::= type-name? "{" field-patterns? "}"
field-patterns          ::= field-pattern ( "," field-pattern )*
field-pattern           ::= identifier ":" pattern
                         | identifier
                         | ".."                     // captures remaining fields
```

### Pattern semantics

1. **Wildcard (`_` or `_: Type`)** matches any value (optionally constraining it to a type) and introduces no binding.
2. **Binding (`name`)** matches any remaining value and binds it without narrowing its type.
3. **Typed Binding (`name: Type`)** checks whether the candidate value is of the named type within a union and binds it to `name`.
4. **Literal** compares by value (numbers, strings, booleans).
5. **Enum variant** matches a specific variant (e.g., `Status.Active`).
6. **Destructuring** matches structured types (classes/records). Each field pattern must succeed. Literal field values (e.g., `User { name: "Admin" }`) are sugar for a guard `if name == "Admin"`. `..` keeps the remaining fields untouched but does not bind them.
7. Guards run only after the pattern matches; they must evaluate to `bool`. If the guard is `false` the arm is skipped and matching continues.

### Typing and binding rules

- The scrutinee has static type `T`. Each pattern is checked against the **current residual type** (what remains uncovered by previous arms).
- Literal and enum patterns narrow to the exact value; the residual type removes that value from the set.
- Typed patterns `binding: Type` can only appear when `Type` is a constituent of the residual union. The bound identifier has type `Type`.
- Binding patterns without a type qualifier (`name =>`) do **not** narrow the type; the bound identifier keeps the residual type.
- Destructuring patterns require the scrutinee to be (or contain) the referenced class/record type. Each field pattern is checked recursively; omitted fields are unconstrained.
- Guards cannot introduce new bindings; they can reference bindings from the pattern and outer scopes. Flow analysis uses successful guards when determining exhaustiveness of subsequent arms.
- Arm bodies must be type-compatible. The `match` expression’s type is the least upper bound of all arm body types.

### Exhaustiveness & reachability

1. **Closed sets** (enums, finite literal unions) must be fully covered by explicit patterns or a catch-all (`_` or unqualified binding). Missing cases produce an error listing uncovered variants/literals.
2. **Union types** require patterns that collectively cover every constituent. `_: Type` or `binding: Type` arms count as covering that constituent; destructuring of a class covers that class variant.
3. **Open types** (plain `string`, `int`, `any`, user-defined `any`) treat a final binding or `_` as sufficient coverage.
4. Guards affect coverage only when statically provable (e.g., `User { age } if age < 18` does **not** cover all `User`, so later arms must handle the remainder).
5. If a pattern (taking guards into account) can never match because earlier arms already cover its space, the compiler emits an “unreachable pattern” diagnostic.

### Evaluation semantics

- Evaluate the scrutinee expression once before matching.
- For each arm in order:
  - Evaluate the pattern against the current value.
  - If it matches, evaluate the guard (if present).
  - On guard success, evaluate the arm body:
    - Single-expression arms return that value.
    - Block arms evaluate statements in order; the value of the last expression is the arm result.
    - `return`, `break`, `continue`, and `throw` behave exactly as they do outside of `match` (i.e., `return` exits the enclosing function).
  - The match expression yields the first arm result and stops; later arms are ignored.
- If no arm matches, emit a compile-time error (exhaustiveness should prevent this).

### Diagnostics

- **E0001 – Non-exhaustive match**: reports missing literals/types/variants.
- **E0002 – Unreachable pattern**: earlier arms cover the same cases.
- **E0003 – Invalid guard**: guard is not boolean or references undefined bindings.
- **E0004 – Invalid destructuring**: field name does not exist or pattern type mismatch.
- **E0005 – Ambiguous qualifier**: type pattern refers to a name not present in the residual union.
- Diagnostics should quote the arm text and suggest adding `_ => ...` when appropriate.

### Reference examples

```baml
match (status) {
  Status.Active => "Active"
  Status.Inactive => "Inactive"
  Status.Pending => "Pending"
}

match (input) {
  "Special" => "special literal"
  s: string => "plain string: " + s
}

// Assuming Result is a union of classes or similar structure
match (result) {
  ok: ResultOk => handle_ok(ok.value)
  err: ResultErr if err.retryable => retry(err)
  err: ResultErr => fail(err)
}

match (user) {
  User { name: "Admin" } => "Welcome, Administrator"
  User { name, age } if age < 18 => "Hello, young " + name
  User { name, .. } => "Hello, " + name
}
```

---

## Explanation (concepts & rationale)

### Goals

- **Type safety**: adding a new enum variant or union member must surface compile-time gaps.
- **Expressiveness**: LLM outputs are often polymorphic; pattern matching should make those flows concise.
- **Consistency**: Aligns with BAML’s expression-oriented functions and existing `scoped-catch` semantics.

### Expression vs. statement

- Chosen as an expression to allow `let result = match (...)` and concise `return match (...)`.
- Blocks evaluate to their last expression; explicit `return` remains available when exiting the entire function is clearer.

### Pattern model

- Blends value patterns (for enums/literals) with type patterns (for unions) to match BAML’s mix of structural and nominal typing.
- **Syntax Decision**: Use `variable: Type` (e.g., `s: string`).
  - **Rationale**: Aligns with BAML’s variable declaration syntax (`let s: string = ...`) and function arguments (`arg: Type`). It treats the pattern match as a "conditional declaration."
  - **Discarded Alternative**: `Type(variable)` (Rust style) was considered but rejected because it resembles constructor calls, and primitives in BAML are not wrappers.
- Binding patterns without qualifiers intentionally **do not** narrow; they exist for catch-alls and situations where the programmer wants the residual type wholesale.

### Destructuring and guards

- Record/class patterns make nested data pipelines readable, mirroring Rust/Python structural matching.
- Literal fields desugar to guards so we maintain one mental model for conditional matching.
- Guards are evaluated after successful pattern binding to keep them pure “filters” rather than part of structural matching.
- Destructuring uses `Type { field: pattern }` syntax, consistent with object construction.

### Exhaustiveness strategy

- Mirrors Rust/TypeScript union checking: we treat enums, literal unions, and union types as closed worlds and require full coverage.
- Wildcards and unqualified bindings are the opt-out; they retain the residual type, which keeps later code honest about what might arrive.
- We intentionally do **not** try to reason about arbitrary guard predicates for coverage; only trivially exhaustive patterns (like `_`) end the analysis.

### Diagnostics & usability

- Early feedback on missing cases and unreachable arms ensures refactors surface immediately.
- Explicit error codes make it easier to document and test IDE diagnostics.

### Open questions

- **Pattern spreads**: the reference grammar includes `..` but does not yet specify binding remaining fields; decide whether `rest` bindings are needed.
- **Nested `match` ergonomics**: consider sugar for `match` inside pipeline expressions.
- **IDE representation**: how should hover/quick-fix surfaces present missing-case suggestions for large unions?

---

Link the original exploratory notes (`match-syntax.md`) here once this spec is adopted, so readers can jump between the reference and the historical context.

