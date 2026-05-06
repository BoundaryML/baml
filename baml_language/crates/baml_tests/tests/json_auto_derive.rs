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
