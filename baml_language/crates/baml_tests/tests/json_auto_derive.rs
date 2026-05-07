//! Auto-derived `to_json` / `from_json` on user classes.
//!
//! First-iteration thin slice: the synthesized methods are wrappers around
//! the existing `baml.json.to_string<Self>` / `baml.json.from_string<Self>`
//! intrinsics shipped by Phase 5. This proves the framework end-to-end:
//!
//! - methods are appended at AST lowering time,
//! - they are dispatched as regular class methods (`u.to_json()` and
//!   `User.from_json(j)`),
//! - they round-trip,
//! - they are filtered from the default bytecode display so user-source
//!   snapshots stay focused.
//!
//! Override-honouring on nested classes is **not** in scope for this
//! iteration; the runtime walker behind `to_string<T>` does not yet dispatch
//! through user `to_json` overrides on nested fields. That's the next
//! follow-up (per BEP-038 §"to_json / from_json Protocol").
//!
//! Phase 5b.2 additions: primitive companion class bridging and universal
//! TypeVar `to_json`/`from_json` resolution.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn simple_class_to_json_roundtrip() {
    // `to_json()` followed by `from_json(...)` should yield a value with the
    // original field values.
    let source = r#"
        class User { name string  age int }
        function main() -> string {
            let u: User = User { name: "Ada", age: 30 };
            let j: baml.json.json = u.to_json();
            let v: User = User.from_json(j);
            v.name
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Ada".to_string()))
    );
}

#[tokio::test]
async fn auto_derive_filtered_from_bytecode_by_default() {
    // The synthesized `to_json` / `from_json` methods must not appear in the
    // default bytecode snapshot — they would bloat every user-class test.
    let source = r#"
        class User { name string  age int }
        function main() -> int {
            let u: User = User { name: "Ada", age: 30 };
            u.age
        }
    "#;
    let output = baml_test!(source);
    assert!(
        !output.bytecode.contains("to_json"),
        "default bytecode should not show auto-derived to_json:\n{}",
        output.bytecode
    );
    assert!(
        !output.bytecode.contains("from_json"),
        "default bytecode should not show auto-derived from_json:\n{}",
        output.bytecode
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(30)));
}

#[tokio::test]
async fn auto_derive_visible_with_show_auto_derive_flag() {
    // `show_auto_derive: true` opts in to seeing the synthesized methods so
    // the synthesizer itself can be tested.
    let source = r#"
        class User { name string  age int }
        function main() -> int { 1 }
    "#;
    let output = baml_test!(baml: source, show_auto_derive: true);
    assert!(
        output.bytecode.contains("to_json"),
        "show_auto_derive: true should expose to_json in bytecode:\n{}",
        output.bytecode
    );
    assert!(
        output.bytecode.contains("from_json"),
        "show_auto_derive: true should expose from_json in bytecode:\n{}",
        output.bytecode
    );
}

#[tokio::test]
async fn user_to_json_override_suppresses_auto_derive() {
    // If the user defines `to_json`, the synthesizer must skip BOTH methods.
    // Calling the user's method should win.
    let source = r#"
        class Secret {
            value string

            function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
                "[redacted]"
            }
        }
        function main() -> baml.json.json throws baml.json.JsonSerializationError {
            let s: Secret = Secret { value: "hunter2" };
            s.to_json()
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("[redacted]".to_string()))
    );
}

// ── Phase 5b.2: primitive companion bridging ──────────────────────────────────

#[tokio::test]
async fn int_to_json_resolves_and_executes() {
    // `(42).to_json()` must resolve to `baml.Int.to_json` (no E0007) and return
    // the integer as a json value.
    let source = r#"
        function main() -> baml.json.json {
            let n: int = 42;
            n.to_json()
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn float_to_json_resolves_and_executes() {
    // `2.5.to_json()` resolves to `baml.Float.to_json` and returns the float.
    let source = r#"
        function main() -> baml.json.json {
            let f: float = 2.5;
            f.to_json()
        }
    "#;
    let output = baml_test!(source);
    match output.result {
        Ok(BexExternalValue::Float(v)) => {
            assert!((v - 2.5).abs() < 1e-9, "expected 2.5, got {v}");
        }
        other => panic!("expected Float(2.5), got {other:?}"),
    }
}

#[tokio::test]
async fn bool_to_json_resolves_and_executes() {
    // `true.to_json()` resolves to `baml.Bool.to_json` and returns the bool.
    let source = r#"
        function main() -> baml.json.json {
            let b: bool = true;
            b.to_json()
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn null_to_json_resolves_and_executes() {
    // `null.to_json()` resolves to `baml.Null.to_json` and returns null as json.
    let source = r#"
        function main() -> baml.json.json {
            let n: null = null;
            n.to_json()
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Null));
}

// ── Phase 5b.4: Array<T>.to_json and Map<K,V>.to_json ────────────────────────

#[tokio::test]
async fn array_of_int_to_json_returns_json_array() {
    // `let a: int[] = [1, 2, 3]; a.to_json()` must return a json array [1, 2, 3].
    let source = r#"
        function main() -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
            let a: int[] = [1, 2, 3];
            a.to_json()
        }
    "#;
    let output = baml_test!(source);
    // The result should be a BAML array value. We stringify to verify shape.
    match &output.result {
        Ok(v) => {
            let s = format!("{v:?}");
            assert!(
                s.contains("1") && s.contains("2") && s.contains("3"),
                "expected [1,2,3] json array, got {s}"
            );
        }
        Err(e) => panic!("expected ok result, got error: {e:?}"),
    }
}

#[tokio::test]
async fn array_of_class_to_json_honors_override() {
    // `class B { x: int  function to_json(self) -> baml.json.json { 99 } }`
    // `bs.to_json()` must call B's override for each element, returning [99, 99].
    let source = r#"
        class B {
            x int

            function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
                99
            }
        }
        function main() -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
            let bs: B[] = [B { x: 1 }, B { x: 2 }];
            bs.to_json()
        }
    "#;
    let output = baml_test!(source);
    match &output.result {
        Ok(v) => {
            let s = format!("{v:?}");
            // Each B maps to 99, so result should be [99, 99].
            assert!(
                s.contains("99"),
                "expected B.to_json override to produce 99 per element, got {s}"
            );
            // Should NOT contain the raw field value 1 or 2.
            assert!(
                !s.contains("\"x\""),
                "expected override to suppress auto-derive field map, got {s}"
            );
        }
        Err(e) => panic!("expected ok result, got error: {e:?}"),
    }
}

#[tokio::test]
async fn map_of_int_to_json_returns_json_map() {
    // `let m: map<string, int> = {"a": 1, "b": 2}; m.to_json()` must return
    // a json map {"a": 1, "b": 2}.
    let source = r#"
        function main() -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
            let m: map<string, int> = {"a": 1, "b": 2};
            m.to_json()
        }
    "#;
    let output = baml_test!(source);
    match &output.result {
        Ok(v) => {
            let s = format!("{v:?}");
            assert!(
                s.contains("\"a\"") && s.contains("\"b\""),
                "expected map keys 'a' and 'b' in result, got {s}"
            );
        }
        Err(e) => panic!("expected ok result, got error: {e:?}"),
    }
}

// ── Phase 5b.2.3: universal TypeVar `to_json` / `from_json` ──────────────────

#[tokio::test]
async fn typevar_to_json_compiles_in_generic_class() {
    // `class G<T>` calling `x.to_json()` where `x: T` must compile without
    // `[E0007] type 'T' has no member 'to_json'`. The method `f` is defined
    // but not called from `main` — only compilation is verified here.
    let source = r#"
        class G<T> {
            items T[]
            function f(self) -> baml.json.json[] throws baml.json.JsonSerializationError | baml.json.JsonParseError {
                self.items.map((x: T) -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError { x.to_json() })
            }
        }
        function main() -> int {
            1
        }
    "#;
    // If this compiles without panic (no E0007 on x.to_json()), the test passes.
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// ── Phase 5b.6: per-field auto-derive, override-honoring, recursive, generic ──

#[tokio::test]
async fn primitive_field_to_json() {
    // `class C { x: int }; C{x:42}.to_json()` must produce `{"x":42}`.
    // This exercises the per-field synthesizer body (5b.5): each field maps
    // to `"field_name": self.<field>.to_json()`.
    let source = r#"
        class C { x int }
        function main() -> string throws baml.json.JsonSerializationError | baml.json.JsonParseError {
            let c: C = C { x: 42 };
            let j: baml.json.json = c.to_json();
            baml.json.stringify(j)
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(r#"{"x":42}"#.to_string()))
    );
}

#[tokio::test]
async fn array_of_primitive_to_json() {
    // `class C { xs: int[] }; C{xs:[1,2,3]}.to_json()` must produce `{"xs":[1,2,3]}`.
    // Exercises the per-field synthesizer when the field type is `int[]`.
    // The auto-derived body calls `self.xs.to_json()` which routes through
    // `Array<int>.to_json` (5b.4), itself calling `Int.to_json` per element.
    let source = r#"
        class C { xs int[] }
        function main() -> string throws baml.json.JsonSerializationError | baml.json.JsonParseError {
            let c: C = C { xs: [1, 2, 3] };
            let j: baml.json.json = c.to_json();
            baml.json.stringify(j)
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(r#"{"xs":[1,2,3]}"#.to_string()))
    );
}

#[tokio::test]
async fn nested_class_override_honored_direct() {
    // `class A { b: B }`, `class B { ... function to_json(self) -> json { "[secret]" } }`:
    // `A{b: B{y:7}}.to_json()` must produce `{"b":"[secret]"}` — the auto-derived
    // `A.to_json` body calls `self.b.to_json()` which dispatches to B's user override.
    let source = r#"
        class B {
            y int

            function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
                "[secret]"
            }
        }
        class A { b B }
        function main() -> string throws baml.json.JsonSerializationError | baml.json.JsonParseError {
            let a: A = A { b: B { y: 7 } };
            let j: baml.json.json = a.to_json();
            baml.json.stringify(j)
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(r#"{"b":"[secret]"}"#.to_string()))
    );
}

#[tokio::test]
async fn nested_class_override_honored_in_array() {
    // `class A { items: B[] }`, B has `to_json` override returning 0:
    // `A{items:[B{y:1},B{y:2}]}.to_json()` must produce `{"items":[0,0]}`.
    // The chain: auto-derived A.to_json → self.items.to_json() (Array<B>.to_json)
    // → B.to_json() per element → user override returns 0.
    let source = r#"
        class B {
            y int

            function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
                0
            }
        }
        class A { items B[] }
        function main() -> string throws baml.json.JsonSerializationError | baml.json.JsonParseError {
            let a: A = A { items: [B { y: 1 }, B { y: 2 }] };
            let j: baml.json.json = a.to_json();
            baml.json.stringify(j)
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(r#"{"items":[0,0]}"#.to_string()))
    );
}

#[tokio::test]
async fn nested_class_override_honored_in_map() {
    // `class A { lookup: map<string, B> }`, B has `to_json` override returning `"B"`:
    // The override must be applied per map value.
    // Chain: auto-derived A.to_json → self.lookup.to_json() (Map<string,B>.to_json)
    // → B.to_json() per value via YieldToCall trampoline (5b.4.2).
    let source = r#"
        class B {
            v int

            function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
                "B"
            }
        }
        class A { lookup map<string, B> }
        function main() -> string throws baml.json.JsonSerializationError | baml.json.JsonParseError {
            let a: A = A { lookup: {"k1": B { v: 1 }, "k2": B { v: 2 }} };
            let j: baml.json.json = a.to_json();
            baml.json.stringify(j)
        }
    "#;
    let output = baml_test!(source);
    // Map key order is insertion order (IndexMap). Verify both keys map to "B".
    match &output.result {
        Ok(BexExternalValue::String(s)) => {
            assert!(
                s.contains(r#""k1":"B""#) && s.contains(r#""k2":"B""#),
                "expected both map values to be \"B\" via override, got {s}"
            );
        }
        other => panic!(r#"expected Ok(String(...)), got {other:?}"#),
    }
}

#[tokio::test]
async fn recursive_class_to_json() {
    // `class Tree { value: int  children: Tree[] }` — the auto-derive synthesizer
    // must not loop infinitely at synthesis time, and one-level-deep recursion
    // must serialize correctly.
    //
    // Tree{value:1, children:[Tree{value:2, children:[]}]}.to_json()
    //   → {"value":1,"children":[{"value":2,"children":[]}]}
    let source = r#"
        class Tree {
            value int
            children Tree[]
        }
        function main() -> string throws baml.json.JsonSerializationError | baml.json.JsonParseError {
            let leaf: Tree = Tree { value: 2, children: [] };
            let root: Tree = Tree { value: 1, children: [leaf] };
            let j: baml.json.json = root.to_json();
            baml.json.stringify(j)
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            r#"{"value":1,"children":[{"value":2,"children":[]}]}"#.to_string()
        ))
    );
}

#[tokio::test]
async fn generic_class_to_json_concrete() {
    // `class Box<T> { value: T }; Box<int>{value:42}.to_json()` must produce
    // `{"value":42}`.  The synthesizer emits `baml.json.to_json(self.value)`
    // (the TypeVar branch of per-field synthesis), which dispatches at
    // runtime via `make_to_json_callee` to `Int.to_json`.
    let source = r#"
        class Box<T> { value T }
        function main() -> string throws baml.json.JsonSerializationError | baml.json.JsonParseError {
            let b: Box<int> = Box<int> { value: 42 };
            let j: baml.json.json = b.to_json();
            baml.json.stringify(j)
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(r#"{"value":42}"#.to_string()))
    );
}

#[tokio::test]
async fn generic_class_to_json_array_typearg() {
    // `class Box<T> { value: T }; Box<int[]>{value:[1,2,3]}.to_json()` must
    // produce `{"value":[1,2,3]}`. The TypeVar branch of per-field synthesis
    // emits `baml.json.to_json(self.value)`, which at runtime must dispatch to
    // `Array<int>::to_json` since T is bound to `int[]`. Composite-type-arg
    // analogue of `generic_class_to_json_concrete`.
    let source = r#"
        class Box<T> { value T }
        function main() -> string throws baml.json.JsonSerializationError | baml.json.JsonParseError {
            let b: Box<int[]> = Box<int[]> { value: [1, 2, 3] };
            let j: baml.json.json = b.to_json();
            baml.json.stringify(j)
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(r#"{"value":[1,2,3]}"#.to_string()))
    );
}

#[tokio::test]
async fn generic_class_honors_override_through_typevar() {
    // `class Box<T> { value: T }` instantiated as `Box<Secret>` where Secret
    // has a user `to_json` override returning `"[redacted]"` — the override
    // must be honored through the generic boundary.  This is the BEP-038
    // contract for generic classes: `baml.json.to_json(self.value)` dispatches
    // on the runtime value's actual type, so Secret's user `to_json` wins.
    let source = r#"
        class Secret {
            data string

            function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
                "[redacted]"
            }
        }
        class Box<T> { value T }
        function main() -> string throws baml.json.JsonSerializationError | baml.json.JsonParseError {
            let b: Box<Secret> = Box<Secret> { value: Secret { data: "hunter2" } };
            let j: baml.json.json = b.to_json();
            baml.json.stringify(j)
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            r#"{"value":"[redacted]"}"#.to_string()
        ))
    );
}

#[tokio::test]
async fn generic_class_with_optional_typevar_field() {
    // `class Box<T> { value: T? }`. After `?.` short-circuits null away, the
    // live receiver still has TypeVar type, so the synthesizer must route
    // through `baml.json.to_json(self.value)` rather than `self.value?.to_json()`
    // — otherwise MIR's `Ty::Void` erasure on TypeVar receivers panics.
    let source = r#"
        class Box<T> { value T? }
        function main() -> string throws baml.json.JsonSerializationError | baml.json.JsonParseError {
            let b: Box<int> = Box<int> { value: 42 };
            let j: baml.json.json = b.to_json();
            baml.json.stringify(j)
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(r#"{"value":42}"#.to_string()))
    );
}

#[tokio::test]
async fn generic_class_with_optional_typevar_field_null() {
    // Same shape as above but the nullable field actually carries `null` —
    // `baml.json.to_json(null)` must produce JSON null inside the object.
    let source = r#"
        class Box<T> { value T? }
        function main() -> string throws baml.json.JsonSerializationError | baml.json.JsonParseError {
            let b: Box<int> = Box<int> { value: null };
            let j: baml.json.json = b.to_json();
            baml.json.stringify(j)
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(r#"{"value":null}"#.to_string()))
    );
}

#[tokio::test]
async fn generic_class_with_optional_typevar_field_honors_override() {
    // `class Box<T> { value: T? }` with `T = Secret` where Secret has a user
    // `to_json` override. The non-null branch must still dispatch on the
    // runtime type and honor the user override.
    let source = r#"
        class Secret {
            data string

            function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
                "[redacted]"
            }
        }
        class Box<T> { value T? }
        function main() -> string throws baml.json.JsonSerializationError | baml.json.JsonParseError {
            let b: Box<Secret> = Box<Secret> { value: Secret { data: "hunter2" } };
            let j: baml.json.json = b.to_json();
            baml.json.stringify(j)
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            r#"{"value":"[redacted]"}"#.to_string()
        ))
    );
}

// ── Phase 5c: per-field auto-derive `from_json` with override-honoring ────────

#[tokio::test]
async fn primitive_field_from_json() {
    // `class C { x: int }; C.from_json(parse("{\"x\":42}"))` must produce `C{x:42}`.
    // Exercises the per-field synthesizer body for from_json: each field maps
    // to `baml.json.from_json<F>(baml.json.field(j, "<name>"))`.
    let source = r#"
        class C { x int }
        function main() -> int throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            let j: baml.json.json = baml.json.parse("{\"x\":42}");
            let c: C = C.from_json(j);
            c.x
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn array_of_primitive_from_json() {
    // `class C { xs: int[] }; C.from_json(parse("{\"xs\":[1,2,3]}"))` must
    // produce `C{xs:[1,2,3]}`.  The auto-derived body calls
    // `baml.json.from_json<int[]>(field(j, "xs"))` which structurally walks the
    // json array per element.
    let source = r#"
        class C { xs int[] }
        function main() -> int throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            let j: baml.json.json = baml.json.parse("{\"xs\":[1,2,3]}");
            let c: C = C.from_json(j);
            c.xs[0] + c.xs[1] + c.xs[2]
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

#[tokio::test]
async fn nested_class_override_honored_direct_from_json() {
    // `class A { b: B }`, `class B { ... function from_json(j) -> B { B{y: 99} } }`:
    // `A.from_json(parse("{\"b\":{\"y\":7}}"))` must produce `A{b: B{y:99}}` —
    // the auto-derived A.from_json calls `baml.json.from_json<B>(field(j, "b"))`
    // which dispatches through B's user override (returning the constant).
    //
    // Without the override-dispatch path this would fall through to structural
    // decode and return `B{y:7}`; the assertion below catches that regression.
    let source = r#"
        class B {
            y int

            function from_json(j: baml.json.json) -> B throws baml.json.JsonParseError | baml.json.JsonDecodeError {
                B { y: 99 }
            }
        }
        class A { b B }
        function main() -> int throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            let j: baml.json.json = baml.json.parse("{\"b\":{\"y\":7}}");
            let a: A = A.from_json(j);
            a.b.y
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

#[tokio::test]
async fn nested_class_override_honored_in_array_from_json() {
    // `class A { items: B[] }`, B has `from_json` override that always
    // returns `B{y:0}`.  `A.from_json(parse("{\"items\":[{\"y\":1},{\"y\":2}]}"))`
    // must call B's override per element via the List trampoline, producing
    // `[B{y:0}, B{y:0}]`. Sum = 0.  Without override-honoring through arrays,
    // sum would be 1+2=3.
    let source = r#"
        class B {
            y int

            function from_json(j: baml.json.json) -> B throws baml.json.JsonParseError | baml.json.JsonDecodeError {
                B { y: 0 }
            }
        }
        class A { items B[] }
        function main() -> int throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            let j: baml.json.json = baml.json.parse("{\"items\":[{\"y\":1},{\"y\":2}]}");
            let a: A = A.from_json(j);
            a.items[0].y + a.items[1].y
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn nested_class_override_honored_in_map_from_json() {
    // `class A { lookup: map<string, B> }`, B has `from_json` override that
    // always returns `B{v:0}`.  Override must apply per map value via the
    // Map trampoline (the from_json-side analog of 5b.4.2's Map.to_json).
    let source = r#"
        class B {
            v int

            function from_json(j: baml.json.json) -> B throws baml.json.JsonParseError | baml.json.JsonDecodeError {
                B { v: 0 }
            }
        }
        class A { lookup map<string, B> }
        function main() -> int throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            let j: baml.json.json = baml.json.parse("{\"lookup\":{\"k1\":{\"v\":7},\"k2\":{\"v\":9}}}");
            let a: A = A.from_json(j);
            a.lookup["k1"].v + a.lookup["k2"].v
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn recursive_class_from_json() {
    // `class Tree { value: int  children: Tree[] }`. Auto-derive must not
    // loop at synthesis time (just like to_json), and decode must round-trip.
    //
    // Tree{value:1, children:[Tree{value:2, children:[]}]}.to_json() round-trips:
    //   parse(stringify) → from_json reconstructs the same shape.
    let source = r#"
        class Tree {
            value int
            children Tree[]
        }
        function main() -> int throws baml.json.JsonSerializationError | baml.json.JsonParseError | baml.json.JsonDecodeError {
            let leaf: Tree = Tree { value: 2, children: [] };
            let root: Tree = Tree { value: 1, children: [leaf] };
            let s: string = baml.json.stringify(root.to_json());
            let j: baml.json.json = baml.json.parse(s);
            let parsed: Tree = Tree.from_json(j);
            parsed.value + parsed.children[0].value
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn generic_class_from_json_concrete() {
    // `class Box<T> { value: T }; Box<int>.from_json(parse("{\"value\":42}"))`
    // must produce `Box<int>{value:42}`.  The synthesizer emits
    // `baml.json.from_json<T>(field(j, "value"))` where T is substituted with
    // int from the call site's type-args; the native then structural-decodes
    // an int.
    let source = r#"
        class Box<T> { value T }
        function main() -> int throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            let j: baml.json.json = baml.json.parse("{\"value\":42}");
            let b: Box<int> = Box<int>.from_json(j);
            b.value
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn generic_class_honors_override_through_typevar_from_json() {
    // BEP-038 contract for generics on the from_json side: `Box<Secret>` where
    // Secret has a user `from_json` override returning a constant must call
    // Secret.from_json through the TypeVar boundary.
    //
    // The lower-pass extension to `find_callee_generic_args` treats the
    // receiver-type `<Secret>` of `Box<Secret>.from_json(j)` as a call-site
    // type-arg, so the BEP-039 channel seeds `Box.from_json`'s frame with
    // `T = Secret`.  Inside the auto-derived body, `baml.json.from_json<T>`
    // substitutes `T` to `Secret` and dispatches `Secret.from_json` (the user
    // override).
    let source = r#"
        class Secret {
            data string

            function from_json(j: baml.json.json) -> Secret throws baml.json.JsonParseError | baml.json.JsonDecodeError {
                Secret { data: "[reset]" }
            }
        }
        class Box<T> { value T }
        function main() -> string throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            let j: baml.json.json = baml.json.parse("{\"value\":{\"data\":\"hunter2\"}}");
            let b: Box<Secret> = Box<Secret>.from_json(j);
            b.value.data
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("[reset]".to_string()))
    );
}

// ── Phase 5c: additional coverage ────────────────────────────────────────────

#[tokio::test]
async fn optional_class_override_honored_from_json_non_null() {
    // `class A { b: B? }` with B's user `from_json` override returning a
    // sentinel.  When the input has a non-null `b`, the native dispatcher's
    // `Optional` arm peels the wrapper and recurses on the inner type, which
    // `YieldToCall`s B's user from_json — the override must win.
    let source = r#"
        class B {
            y int

            function from_json(j: baml.json.json) -> B throws baml.json.JsonParseError | baml.json.JsonDecodeError {
                B { y: 99 }
            }
        }
        class A { b B? }
        function main() -> int throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            let j: baml.json.json = baml.json.parse("{\"b\":{\"y\":7}}");
            let a: A = A.from_json(j);
            match (a.b) {
                null => -1,
                let inner: B => inner.y,
            }
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

#[tokio::test]
async fn optional_class_override_honored_from_json_null() {
    // Same shape as above but the input has `b: null`. The native dispatcher's
    // `Optional` arm short-circuits to `Value::Null` without calling B's
    // `from_json`. Without the short-circuit, calling `B.from_json(null)`
    // would land in B's user override and return `B{y:99}` — so this test
    // catches a regression where Optional accidentally falls through.
    let source = r#"
        class B {
            y int

            function from_json(j: baml.json.json) -> B throws baml.json.JsonParseError | baml.json.JsonDecodeError {
                B { y: 99 }
            }
        }
        class A { b B? }
        function main() -> int throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            let j: baml.json.json = baml.json.parse("{\"b\":null}");
            let a: A = A.from_json(j);
            match (a.b) {
                null => -1,
                let inner: B => inner.y,
            }
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn optional_primitive_field_from_json() {
    // `class C { x: int? }`. Auto-derived from_json's per-field call is
    // `baml.json.from_json<int?>(field(j, "x"))`. Native dispatches Optional →
    // null pass-through OR structural-decode int. Tests both shapes.
    let source = r#"
        class C { x int? }
        function main() -> int throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            let j1: baml.json.json = baml.json.parse("{\"x\":42}");
            let c1: C = C.from_json(j1);
            let j2: baml.json.json = baml.json.parse("{\"x\":null}");
            let c2: C = C.from_json(j2);
            let v1: int = match (c1.x) {
                null => -1,
                let n: int => n,
            };
            let v2: int = match (c2.x) {
                null => -1,
                let n: int => n,
            };
            v1 + v2
        }
    "#;
    let output = baml_test!(source);
    // 42 + (-1) = 41
    assert_eq!(output.result, Ok(BexExternalValue::Int(41)));
}

#[tokio::test]
async fn map_of_primitive_from_json() {
    // `class C { lookup: map<string, int> }`. Auto-derived from_json's
    // per-field call is `baml.json.from_json<map<string,int>>(field(j,"lookup"))`.
    // Native walks the json object; values are primitives so each falls into
    // the no-yield structural-decode branch (no continuation trampoline
    // involvement). Tests the "structural map walk" path.
    let source = r#"
        class C { lookup map<string, int> }
        function main() -> int throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            let j: baml.json.json = baml.json.parse("{\"lookup\":{\"a\":1,\"b\":2,\"c\":3}}");
            let c: C = C.from_json(j);
            c.lookup["a"] + c.lookup["b"] + c.lookup["c"]
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

#[tokio::test]
async fn top_level_from_json_call_concrete_class() {
    // Direct `baml.json.from_json<User>(j)` — not via auto-derive synthesis.
    // The dispatcher looks up `User.from_json` (which IS the auto-derived
    // body) and YieldToCalls it.  Round-trip with manual call site asserts
    // the public dispatcher works on its own.
    let source = r#"
        class User { name string  age int }
        function main() -> string throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            let j: baml.json.json = baml.json.parse("{\"name\":\"Ada\",\"age\":30}");
            let u: User = baml.json.from_json<User>(j);
            u.name
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Ada".to_string()))
    );
}

#[tokio::test]
async fn top_level_from_json_call_primitive() {
    // Direct `baml.json.from_json<int>(j)` for a primitive — the dispatcher
    // structural-decodes via Phase 5a's `ty_serde_to_value`.  Tests that the
    // public function is callable for non-class types too.
    let source = r#"
        function main() -> int throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            let j: baml.json.json = baml.json.parse("42");
            baml.json.from_json<int>(j)
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn independent_synthesis_only_from_json_defined() {
    // BEP-038's `to_json` and `from_json` are independent.  `class B` defines
    // ONLY `from_json`; the synthesizer must still emit `to_json` for B
    // (otherwise A's auto-derived `to_json` body — `self.b.to_json()` — fails
    // to compile).
    //
    // Asserts the loosened `maybe_synthesize_json_methods` rule from Phase 5c
    // and verifies that A's `from_json` honors B's user override.
    let source = r#"
        class B {
            y int

            function from_json(j: baml.json.json) -> B throws baml.json.JsonParseError | baml.json.JsonDecodeError {
                B { y: 99 }
            }
        }
        class A { b B }
        function main() -> string throws baml.json.JsonSerializationError | baml.json.JsonParseError | baml.json.JsonDecodeError {
            let a_in: A = A { b: B { y: 1 } };
            // Auto-derived A.to_json must still compile (B has auto-derived to_json).
            let s: string = baml.json.stringify(a_in.to_json());
            // A.from_json dispatches B's user override, returning y=99.
            let parsed: A = A.from_json(baml.json.parse(s));
            match (parsed.b.y) {
                99 => "override-honored",
                _ => "regression"
            }
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("override-honored".to_string()))
    );
}

#[tokio::test]
async fn independent_synthesis_only_to_json_defined() {
    // Inverse of the previous test: `class B` defines ONLY `to_json`.  The
    // synthesizer must still emit `from_json` for B so A's auto-derived
    // `from_json` body can decode `b: B`.  When B has only user `to_json`,
    // A's `from_json` falls back to the auto-derived structural decode of B.
    let source = r#"
        class B {
            y int

            function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
                "[redacted]"
            }
        }
        class A { b B }
        function main() -> int throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            let j: baml.json.json = baml.json.parse("{\"b\":{\"y\":7}}");
            let a: A = A.from_json(j);
            a.b.y
        }
    "#;
    let output = baml_test!(source);
    // No user override on from_json side — auto-derive structural decode reads y=7.
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

#[tokio::test]
async fn round_trip_with_overrides_both_directions() {
    // End-to-end roundtrip with user overrides on BOTH `to_json` and
    // `from_json`.  Constructed Secret serializes to a fixed string per the
    // override, then parses back to a fixed `Secret { data: "[reset]" }`.
    // Asserts the two contracts compose correctly through stringify/parse.
    let source = r#"
        class Secret {
            data string

            function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
                "[redacted]"
            }
            function from_json(j: baml.json.json) -> Secret throws baml.json.JsonParseError | baml.json.JsonDecodeError {
                Secret { data: "[reset]" }
            }
        }
        function main() -> string throws baml.json.JsonSerializationError | baml.json.JsonParseError | baml.json.JsonDecodeError {
            let s: Secret = Secret { data: "hunter2" };
            let json_val: baml.json.json = s.to_json();
            let stringified: string = baml.json.stringify(json_val);
            let parsed_json: baml.json.json = baml.json.parse(stringified);
            let s2: Secret = Secret.from_json(parsed_json);
            s2.data
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("[reset]".to_string()))
    );
}

#[tokio::test]
async fn alias_chain_dispatch_to_json() {
    // `type Inner = B; type Outer = Inner` chains two non-cyclic aliases
    // through to `class B`.  Calling `b.to_json()` on a value typed as
    // `Outer` must walk the alias chain inside `resolve_member` until it
    // bottoms out on `B`, then dispatch B's user `to_json` override.
    //
    // Without the `Ty::TypeAlias` arm in `resolve_member` (`builder.rs:4977`)
    // this would fail with `[E0007] type 'Outer' has no member 'to_json'`.
    let source = r#"
        class B {
            y int

            function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
                42
            }
        }
        type Inner = B
        type Outer = Inner

        function main() -> int throws baml.json.JsonSerializationError | baml.json.JsonParseError {
            let b: Outer = B { y: 7 };
            let j: baml.json.json = b.to_json();
            match (j) {
                let n: int => n,
                _ => -1,
            }
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn alias_chain_dispatch_from_json() {
    // Same alias chain, but on the static-method side.  `Outer.from_json(j)`
    // must walk the alias chain to `B` and dispatch B's user `from_json`
    // override.  The override returns a constant — without alias-chained
    // member resolution the call would not even compile.
    let source = r#"
        class B {
            y int

            function from_json(j: baml.json.json) -> B throws baml.json.JsonParseError | baml.json.JsonDecodeError {
                B { y: 99 }
            }
        }
        type Inner = B
        type Outer = Inner

        function main() -> int throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            let j: baml.json.json = baml.json.parse("{\"y\":7}");
            let b: Outer = Outer.from_json(j);
            b.y
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

#[tokio::test]
async fn from_json_wrapper_body_fallback() {
    // Classes with field types unsafe for per-field synthesis (`$rust_type`,
    // function types, `unknown`) fall back to the wrapper body
    // `baml.json.from_string<Self>(stringify(j))` (mirrors the to_json side
    // 5b.5).  Field type `unknown` is the simplest trigger:
    // `class_is_safe_for_per_field_synthesis` returns false and the
    // synthesizer emits the wrapper body.
    //
    // The decode then goes through Phase 5a's structural-coerce path, which
    // round-trips concrete shapes regardless of the unknown declared type.
    let source = r#"
        class Holder { value unknown }
        function main() -> int throws baml.json.JsonParseError | baml.json.JsonDecodeError {
            let j: baml.json.json = baml.json.parse("{\"value\":42}");
            let h: Holder = Holder.from_json(j);
            // `value` is unknown so structural decode keeps it as the parsed
            // json int.  Match-narrow to extract.
            match (h.value) {
                let n: int => n,
                _ => -1
            }
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}
