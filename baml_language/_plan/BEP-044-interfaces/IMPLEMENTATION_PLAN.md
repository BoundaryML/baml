# BEP-044 Interfaces — Implementation Fix Plan

> Written 2026-05-14. Based on BEP rev 3 vs current git diff.

This plan is organized around **gaps between what the BEP says and what the code does**, expressed primarily through the tests that need to be rewritten or added to exercise each gap. The tests define the target behavior; the implementation work follows from making them pass.

---

## Gap 1: `requires` replaces `extends` for interface inheritance

### What the BEP says

Interfaces use `requires` to declare that implementors must also implement listed parent interfaces. `extends` is reserved exclusively for generic parameter bounds (`<T extends Named>`). The three keywords are disjoint:

| Keyword | Where | Meaning |
|---|---|---|
| `requires` | `interface` declaration | Implementors must also implement the listed interfaces |
| `extends` | Generic parameter bound | Type parameter must satisfy this type expression |
| `implements` | Class body or top-level `for T` | Fulfills an interface contract |

```baml
interface Person requires Named, Aged { ... }
```

### What the code does

Uses `extends` everywhere — `EXTENDS_CLAUSE`, `parse_extends_clause`, `InterfaceDef.extends`, `Interface.extends`, `interface_closure_locs` walks `.extends`. There is no `requires` token, no `KW_REQUIRES`, no `REQUIRES_CLAUSE`.

### Tests to rewrite

Every test that uses `extends` on interfaces must change to `requires`:

**`crates/baml_lsp2_actions_tests/test_files/syntax/interface/`:**

- `extends_cycle.baml` — change `interface A extends B` → `interface A requires B` (both occurrences). Update diagnostic message from `"extends chain forms a cycle"` → `"requires chain forms a cycle"` (or whatever message we settle on). Error code stays E0118.

- `error_interface_extends_field_conflict.baml` — change `interface Z extends X, Y {}` → `interface Z requires X, Y {}`. Update diagnostic message to say `"requires"` instead of `"extends"`.

- `valid_inheritance.baml` — change `interface Person extends Named, Aged` → `interface Person requires Named, Aged`.

- `valid_extends_chain_three_levels.baml` — change `interface Greeter extends HasName` → `interface Greeter requires HasName`, `interface Polite extends Greeter` → `interface Polite requires Greeter`.

- `valid_diamond_independent_resolution.baml` — change `interface Left extends Base` → `interface Left requires Base`, same for `Right`.

- `valid_interface_field_via_extends.baml` — change `interface Person extends Named, Aged` → `interface Person requires Named, Aged`.

- `valid_field_rule_3_merged_same_type.baml` — no change (no extends).

**`crates/baml_tests/tests/interfaces.rs`:**

Every source string containing `extends` on an interface must change to `requires`. Key groups:

- Group A: `interface_extends_aggregates_contracts` — `"interface Person extends Named, Aged"` → `"interface Person requires Named, Aged"`
- Group C: `extends_cycle_is_compile_error`, `three_way_extends_cycle_is_compile_error` — all `extends` → `requires`
- Group E: `implementing_extends_chain_satisfies_parent_required_methods`, `extends_chain_required_method_must_be_provided` — all `extends` → `requires`
- Group Q: all interface `extends` → `requires`
- Groups AB, AG: diamond/multi-level tests — all `extends` → `requires`
- Group R: `reflect_implements_transitive_via_extends` — rename test + source `extends` → `requires`
- Rename test functions: `extends_chain_*` → `requires_chain_*` or similar

**Important:** `extends` in generic bounds stays as-is — `<T extends Named>` is correct per the BEP.

---

## Gap 2: `requires` means explicit separate `implements` — no transitive satisfaction

### What the BEP says

When `interface Person requires Named, Aged`, a class implementing `Person` must also have **separate** `implements Named` and `implements Aged` blocks. Implementing `Person` does NOT automatically satisfy `Named` and `Aged`. This is Rust's supertrait model.

```baml
class Employee {
  implements Named  { name: string }        // required explicitly
  implements Aged   { age: int }            // required explicitly
  implements Person { occupation: string }  // now ok
}

class Bad {
  implements Person {}
  // ERROR (E0125): class `Bad` implements `Person`, which requires
  // `Named` and `Aged`, but `Bad` does not implement them.
}
```

### What the code does

The current implementation treats `extends` as transitive inheritance — implementing the child silently satisfies parents. There is no E0125 diagnostic.

### Tests to rewrite/add

**`crates/baml_lsp2_actions_tests/test_files/syntax/interface/`:**

- `valid_inheritance.baml` — currently Employee only has `implements Person`. Must add `implements Named {}` and `implements Aged {}` blocks to remain valid:
```baml
interface Person requires Named, Aged {
  occupation: string
  function introduce(self) -> string
}

class Employee {
  salary: float
  implements Named  { name: string }
  implements Aged   { age: int }
  implements Person {
    occupation: string
    function introduce(self) -> string { return "hello" }
  }
}
```

- `valid_extends_chain_three_levels.baml` — Robot must implement all three interfaces:
```baml
class Robot {
  implements HasName { name: string }
  implements Greeter {
    function greet(self) -> string { return "hello" }
  }
  implements Polite {
    function farewell(self) -> string { return "bye" }
  }
}
```

- `valid_diamond_independent_resolution.baml` — Diamond must implement Base explicitly:
```baml
class Diamond {
  implements Base {}
  implements Left {}
  implements Right {}
}
```
(This one may already be correct depending on the current test content.)

- `valid_interface_field_via_extends.baml` — Employee must implement Named and Aged separately:
```baml
class Employee {
  salary: float
  implements Named  { name: string }
  implements Aged   { age: int }
  implements Person { occupation: string }
}
```

- **NEW: `error_missing_required_interface.baml`** — test E0125:
```baml
interface Named { name: string }
interface Aged  { age: int }
interface Person requires Named, Aged { occupation: string }

class Bad {
  implements Person { occupation: string }
}
// ERROR (E0125): class `Bad` implements `Person`, which requires `Named` and `Aged`,
// but `Bad` does not implement them.
```

**`crates/baml_tests/tests/interfaces.rs`:**

- Group A `interface_extends_aggregates_contracts` — update Employee to have three implements blocks
- Group E `implementing_extends_chain_satisfies_parent_required_methods` — rewrite to test E0125
- Group E `extends_chain_required_method_must_be_provided` — rewrite
- All runtime tests that use extends chains (Groups L, N, O, P, R, AB, AG) — add separate `implements` blocks for each required parent
- **Add new compile-time tests:**
  - `missing_required_interface_is_compile_error` — E0125
  - `missing_one_of_two_required_interfaces_is_compile_error`
  - `satisfying_all_required_interfaces_is_ok`

---

## Gap 3: Fields must be redeclared inside `implements` blocks

### What the BEP says

Interface fields are **not auto-injected**. The implementor must redeclare each field inside the `implements` block:

```baml
class Server {
  max_connections: int       // class-own field

  implements Config {
    host: string             // redeclared to satisfy the contract
    port: int                // redeclared
  }
}
```

Missing a field from the interface in the implements block is E0113. Type mismatch is E0116.

### What the code does

Fields are auto-injected from the interface into the class. `ImplementsBlockDef` has no `fields` member. The parser rejects non-function content in implements blocks. `collect_class_fields_with_implements` in emit pulls fields from the interface definition, not from the implements block.

### Tests to rewrite

**Every valid test that uses interfaces with fields must put the fields inside the `implements` block.**

**`crates/baml_lsp2_actions_tests/test_files/syntax/interface/`:**

- `valid_basic.baml`:
```baml
class Dog {
  breed: string
  implements Animal {
    name: string              // was not here before — redeclare
    age: int                  // was not here before — redeclare
    function speak(self) -> string { return "Woof!" }
  }
}
```

- `valid_interface_typed_var_method_call.baml`:
```baml
class Dog {
  implements Animal {
    name: string              // add
    function speak(self) -> string { return "Woof!" }
  }
}
```

- `valid_field_rule_3_merged_same_type.baml` — under the new BEP each interface's fields are in separate namespaces, so this test changes semantically. Two interfaces with `name: string` are now two separate fields:
```baml
class Item {
  implements Named  { name: string }
  implements Labeled { name: string }
}
// No conflict — separate namespaces. Both fields kept.
// Access: item.Named.name vs item.Labeled.name
```

- `valid_field_rule_5_subtype_field.baml`:
```baml
class Dog {
  implements Animal {
    parent: Animal?
    function speak(self) -> string { return "Woof!" }
  }
}
```

- `field_type_mismatch.baml` — now the mismatch is *inside* the implements block, not on a class-level field:
```baml
class Server {
  implements Config {
    port: string    // ERROR (E0116): expected `int`, found `string`
  }
}
```
(The old test had `port: string` as a class-own field — that's now a different thing entirely.)

- `conflicting_field_types.baml` — under the new BEP, two interfaces with same field name but different types are NOT a conflict (separate namespaces). This test needs to be rethought. The conflict case now only applies to `requires` chains:
```baml
interface X { id: string }
interface Y { id: int }
interface Z requires X, Y {}  // E0122: conflicting field types inherited via requires
```

- **NEW: `error_missing_field_in_implements_block.baml`**:
```baml
interface Config { host: string, port: int }
class Server {
  implements Config {
    host: string
    // ERROR (E0113): missing required field `port` of interface `Config`
  }
}
```

**`crates/baml_tests/tests/interfaces.rs`:**

- Group A: all tests with interfaces that have fields — add field redeclarations inside implements blocks
- Group D: `class_field_type_mismatch_is_compile_error` — move the mismatched field into the implements block
- Group D: `class_field_matching_interface_field_is_ok` — field goes inside implements block
- Group D: `two_interfaces_same_field_same_type_is_ok` — rewrite: two separate namespaced fields
- Group D: `conflicting_field_types_across_interfaces_is_compile_error` — E0117 only fires through `requires` chains now, not from a class implementing two unrelated interfaces
- Group L (runtime): all field-injection tests — change construction to put fields in implements blocks
- Group H, Group K, Group J, Group O, Group P, Group R — anywhere interface fields exist, redeclare them

---

## Gap 4: Qualified field construction (`Interface.field: value`)

### What the BEP says

At construction, interface fields are always addressed by `Interface.fieldname`:

```baml
let s = Server {
  max_connections: 100,      // class-own field: bare name
  Config.host: "localhost",  // interface field: qualified
  Config.port: 8080,         // interface field: qualified
}
```

### What the code does

No support for dotted keys in object literals. Fields are constructed with bare names because they're auto-injected.

### Tests to add

**`crates/baml_tests/tests/interfaces.rs`:**

- Group L `field_auto_injected_at_construction` — rename to `field_constructed_with_qualified_key`:
```baml
let s = Server { max_connections: 100, Config.host: "localhost", Config.port: 8080 }
assert s.host == "localhost"
assert s.Config.host == "localhost"
```

- All runtime tests that construct classes with interface fields must use qualified keys:
```baml
let d = Dog { breed: "Lab", Animal.name: "Rex", Animal.age: 3 }
```

- **NEW tests:**
  - `unqualified_interface_field_at_construction_is_error` — bare `host: "localhost"` when `host` only comes from an interface → error
  - `qualified_field_access_always_works` — `s.Config.host` always resolves
  - `unqualified_field_access_flattens_when_unambiguous` — `s.host` works when only Config contributes `host`
  - `unqualified_field_access_ambiguous_is_error` — `item.name` when both Named and Labeled contribute `name` → error

---

## Gap 5: `implements I for T` (out-of-body impls)

### What the BEP says

`implements` blocks can appear at the top level with `for T`:

```baml
implements ToJson for int {
  function to_json(self) -> json { return json.of(self) }
}
```

Rules: method-only injection for types with fixed shape (E0123), one impl per (interface, type) pair (E0114), orphan rule — must own one side (E0126).

### What the code does

Not implemented. The parser only handles `implements` inside `parse_class`. No top-level dispatch.

### Tests to add

**`crates/baml_lsp2_actions_tests/test_files/syntax/interface/`:**

- **NEW: `valid_implements_for_class.baml`**:
```baml
interface ToJson {
  function to_json(self) -> string
}
class Dog { breed: string }
implements ToJson for Dog {
  function to_json(self) -> string { return self.breed }
}
```

- **NEW: `error_implements_for_duplicate.baml`** — E0114 when both in-body and out-of-body exist

- **NEW: `error_implements_for_with_fields.baml`** — E0123 when interface has fields and target is a primitive/fixed-shape type

**`crates/baml_tests/tests/interfaces.rs`:**

- **NEW compile-time tests:**
  - `out_of_body_implements_for_class_compiles`
  - `out_of_body_implements_for_primitive_compiles` (method-only interface)
  - `out_of_body_implements_for_primitive_with_fields_is_error` (E0123)
  - `out_of_body_and_in_body_for_same_interface_is_error` (E0114)
  - `out_of_body_orphan_rule_violation_is_error` (E0126) — deferred if cross-package tests are hard

- **NEW runtime tests:**
  - `out_of_body_method_callable_on_instance`
  - `out_of_body_dispatch_through_interface_typed_var`

---

## Gap 6: `implement` (singular) as keyword alias

### What the BEP says

Both `implement` and `implements` are accepted as keywords.

### What the code does

Only `implements` is a token. No `implement` alias.

### Tests to add

- **NEW: `valid_implement_singular.baml`**:
```baml
interface Animal { function speak(self) -> string }
class Dog {
  implement Animal {
    function speak(self) -> string { return "Woof!" }
  }
}
```

- In `interfaces.rs`: `singular_implement_keyword_parses` — no errors

---

## Gap 7: Missing E0125 diagnostic (requires not satisfied)

### What the BEP says

```
// ERROR (E0125): class `Bad` implements `Person`, which requires
// `Named` and `Aged`, but `Bad` does not implement them.
```

### What the code does

E0112-E0123 exist. No E0125.

### Tests to add

Already covered in Gap 2 above. Add to both LSP tests and `interfaces.rs`.

---

## Gap 8: Interface-to-interface subtyping is unsound

### What the BEP says

A value held at one interface type is NOT directly assignable to another interface type, even when the concrete class implements both. Must narrow via `match`/`is` first.

```baml
let a: Animal = Dog {}
let s: Swimmer = a       // ERROR
```

### What the code does

`builder.rs:8425` allows `Interface A <: Interface B` when `has_common_implementor`. This is too permissive.

### Tests to rewrite

**`crates/baml_tests/tests/interfaces.rs`:**

- Group Z `cast_from_one_interface_to_another_when_class_implements_both` — this test currently expects success. It should expect a compile error:
```rust
#[test]
fn cross_interface_assignment_is_compile_error() {
    let src = r#"
        interface Animal { function speak(self) -> string }
        interface Swimmer { function swim(self) -> string }
        class Duck {
          implements Animal { function speak(self) -> string { return "Quack!" } }
          implements Swimmer { function swim(self) -> string { return "splash" } }
        }
        function bad() -> string {
          let a: Animal = Duck {}
          let s: Swimmer = a       // ERROR: Animal is not assignable to Swimmer
          return s.swim()
        }
    "#;
    assert_compile_error_contains(src, "Animal");
}
```

- **NEW: `cross_interface_narrowing_via_match_works`** — the valid way to convert:
```rust
fn cross_interface_via_match() {
    // let a: Animal = Duck {}
    // match (a) { let d: Duck => { let s: Swimmer = d; s.swim() } ... }
}
```

---

## Gap 9: Fields in separate namespaces (two interfaces, same field name)

### What the BEP says

Two interfaces declaring a field with the same name are NOT a conflict — they live in separate namespaces:

```baml
class Item {
  implements Named   { name: string }
  implements Labeled { name: string }
}
let i = Item { Named.name: "widget", Labeled.name: "WIDGET-001" }
i.Named.name      // "widget"
i.Labeled.name    // "WIDGET-001"
i.name            // ERROR: ambiguous
```

### What the code does

Fields are auto-injected and deduplicated by name. Two interfaces with same field name and same type → merged into one field.

### Tests to rewrite

- `valid_field_rule_3_merged_same_type.baml` — rename to `valid_same_field_name_separate_namespaces.baml`:
```baml
class Item {
  implements Named   { name: string }
  implements Labeled { name: string }
}
// No error — separate namespaces
```

- `conflicting_field_types.baml` — under the new model, two interfaces with different types for the same field name is NOT an error on the class (they're separate fields). E0117 only fires on `requires` chains. Rewrite:
```baml
class Thing {
  implements HasId    { id: string }
  implements HasNumId { id: int }
}
// No error — separate namespaces. Access via Thing.HasId.id vs Thing.HasNumId.id
```

- **NEW: `error_ambiguous_unqualified_field_access.baml`**:
```baml
class Item {
  implements Named   { name: string }
  implements Labeled { name: string }
}
function bad(i: Item) -> string {
  return i.name    // ERROR: ambiguous — Named.name vs Labeled.name
}
```

---

## Gap 10: Compile error — `baml_compiler2_emit` missing field

### What it is

`crates/baml_compiler2_emit/src/lib.rs:1779` — `Function` struct literal missing `generic_param_bounds: Vec::new()`. Blocks `cargo build`.

### Fix

Add the field. This is the P0 blocker.

---

## Gap 11: Parser doesn't support fields in `implements` blocks

### What the code does

`parse_implements_block` only accepts `function` definitions. Any non-function token triggers `"method definition expected in 'implements' block"`.

### What needs to change

The parser must accept field declarations (with optional `= default_value`) inside `implements` blocks, just like it does in class bodies. `ImplementsBlockDef` needs a `fields: Vec<ClassFieldDef>` member.

### Tests affected

Every test in Gap 3 depends on this parser change. The `valid_default_call_from_override.baml` test currently fails with 7 parse errors — most of those are because `= "TS"` default values aren't supported in the current context, which would also be fixed by this.

---

## Gap 12: `default.method()` — parser issues

### What the BEP says

`default.method()` calls the interface's default implementation from an override. Scoping rules: only valid inside `implements` block, doesn't capture into lambdas, can be shadowed by a local `default` binding.

### What the code does

TIR and MIR have `default` resolution logic, but the parser/LSP test `valid_default_call_from_override.baml` currently shows 7 parse errors. The `default` identifier isn't recognized in expression position properly.

### Tests to fix

- `valid_default_call_from_override.baml` — should show no diagnostics once parser is fixed
- `error_default_on_required_method.baml` — should work (E0123)
- `error_default_outside_implements.baml` — should work (E0003)
- Group M runtime tests: `default_call_from_override_returns_string`, `default_resolves_to_current_block`
- Group AC compile-time: `default_keyword_outside_implements_block_is_compile_error`
- Group AH runtime: all `default` corner cases

---

## Full Test Rewrite Checklist

### LSP tests (`crates/baml_lsp2_actions_tests/test_files/syntax/interface/`)

| File | Changes needed |
|---|---|
| `ambiguous_method.baml` | No change |
| `conflicting_field_types.baml` | Rewrite — separate namespaces means no E0117 here; move E0117 test to requires chain |
| `duplicate_implements.baml` | No change |
| `error_concrete_field_through_interface_type.baml` | Add field redeclaration in implements block |
| `error_default_on_required_method.baml` | No change |
| `error_default_outside_implements.baml` | No change |
| `error_generic_bound_violation.baml` | No change |
| `error_generic_different_type_params_on_one_class.baml` | No change |
| `error_interface_extends_field_conflict.baml` | `extends` → `requires`, update diagnostic text |
| `error_invariant_generic_assignment.baml` | Add field redeclarations in implements blocks |
| `error_match_without_wildcard.baml` | Add field redeclarations in implements blocks |
| `error_qualified_call_non_implementor.baml` | Add field redeclarations in implements blocks |
| `error_qualified_call_unknown_interface.baml` | Add field redeclarations in implements blocks |
| `extends_cycle.baml` | `extends` → `requires`, update diagnostic text |
| `field_type_mismatch.baml` | Move field into implements block |
| `method_signature_mismatch.baml` | No change |
| `missing_required_method.baml` | No change |
| `unknown_interface.baml` | No change |
| `unknown_interface_member.baml` | No change |
| `valid_basic.baml` | Add field redeclarations in implements block |
| `valid_class_methods_outside_interface.baml` | No change (no interface fields) |
| `valid_default_call_from_override.baml` | Fix to have no diagnostics once parser fixed |
| `valid_default_methods.baml` | No change |
| `valid_diamond_independent_resolution.baml` | `extends` → `requires`, add `implements Base {}` |
| `valid_extends_chain_three_levels.baml` | `extends` → `requires`, add separate implements for each |
| `valid_field_rule_3_merged_same_type.baml` | Rewrite for separate namespaces model |
| `valid_field_rule_5_subtype_field.baml` | Add field redeclaration in implements block |
| `valid_generic_bound.baml` | Add field redeclaration in implements block |
| `valid_generic_concrete_type_param.baml` | No interface-field change needed |
| `valid_inheritance.baml` | `extends` → `requires`, add separate implements for Named/Aged |
| `valid_interface_field_via_extends.baml` | `extends` → `requires`, add separate implements, fields in blocks |
| `valid_interface_typed_var_method_call.baml` | Add field redeclaration in implements block |
| `valid_match_narrows_to_concrete.baml` | No change (no interface fields) |
| `valid_match_with_wildcard.baml` | No change (no interface fields) |
| `valid_qualified_method_call.baml` | No change |
| `valid_self_qualified_call_in_implements.baml` | No change (name is class-own field) |

### New LSP test files to create

| File | What it tests |
|---|---|
| `error_missing_required_interface.baml` | E0125 — implements child without parents |
| `error_missing_field_in_implements_block.baml` | E0113 — interface field not redeclared |
| `error_ambiguous_unqualified_field_access.baml` | Two interfaces, same field name, bare access |
| `valid_implement_singular.baml` | `implement` keyword alias |
| `valid_implements_for_class.baml` | Out-of-body `implements ToJson for Dog` |
| `valid_separate_namespace_same_field_name.baml` | Two interfaces, same field, different namespaces |
| `valid_qualified_field_access.baml` | `obj.Interface.field` syntax |

### `crates/baml_tests/tests/interfaces.rs`

Every group needs updating. The changes fall into these categories:

1. **`extends` → `requires`** in all interface source strings (but NOT in generic bounds)
2. **Add separate `implements` blocks** for every required parent interface
3. **Redeclare fields** inside `implements` blocks instead of relying on auto-injection
4. **Qualified construction** — `Dog { breed: "Lab", Animal.name: "Rex", Animal.age: 3 }`
5. **Fix subtyping test** — cross-interface assignment becomes an error
6. **Add E0125 tests** — missing required parent interfaces
7. **Add field namespace tests** — qualified access, ambiguous access

### `crates/baml_lsp2_actions_tests/test_files/completion/top_level_keywords.baml`

Add `interface` and `implements` to the `SHOULD_CONTAIN` list.

---

## Implementation Phases

### Phase 1: Make it compile
1. Fix `baml_compiler2_emit/src/lib.rs:1779` — add `generic_param_bounds: Vec::new()`

### Phase 2: Add `requires` keyword
1. Add `Requires` token to lexer
2. Add `KW_REQUIRES`, `REQUIRES_CLAUSE` to syntax kinds
3. Change `parse_interface` to parse `requires` clause (keep `parse_extends_clause` for generic bounds only)
4. Change AST `InterfaceDef.extends` → `InterfaceDef.requires`
5. Update HIR `Interface.extends` → `Interface.requires`
6. Update TIR `interfaces.rs` to walk `.requires`
7. Update LSP `check.rs` to use `.requires`
8. Add `implement` singular token to lexer as alias

### Phase 3: Fields in implements blocks
1. Modify parser `parse_implements_block` to accept field declarations
2. Add `fields: Vec<ClassFieldDef>` to `ImplementsBlockDef`
3. Change HIR/TIR/emit to read fields from implements block, not from interface definition
4. Add qualified field construction support (dotted keys in object literals)
5. Add E0113 for missing field in implements block
6. Add qualified field access (`obj.Interface.field`)

### Phase 4: Explicit requires satisfaction
1. Add E0125 diagnostic — "class implements X which requires Y, but doesn't implement Y"
2. In validation (LSP check.rs or better: HIR/TIR), check that for each `implements I`, all of `I.requires` are also explicitly implemented
3. Remove transitive auto-satisfaction

### Phase 5: Out-of-body implements
1. Add top-level `implements I for T { ... }` parsing
2. Lower to same HIR as in-body implements
3. Add E0126 orphan rule check
4. Add E0123 for field-bearing interface on fixed-shape type

### Phase 6: Fix subtyping
1. Remove `has_common_implementor` check in `is_subtype`
2. Only allow `A <: B` when `A == B` or `A requires B` (transitively)

### Phase 7: Rewrite all tests
1. Update all LSP test files per the checklist above
2. Rewrite all `interfaces.rs` test sources
3. Add new test files
4. Update snapshots (`INSTA_UPDATE=1 cargo test`)
5. Update completion test to include `interface`
