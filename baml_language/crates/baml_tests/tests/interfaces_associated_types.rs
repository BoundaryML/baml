//! Tests for BEP-057 associated types on interfaces.
//!
//! The suite covers declaration/binding syntax, default witnesses, projection
//! disambiguation, required-interface propagation, unions, destructuring, and
//! runtime dispatch through associated interface views.
//!
//! One ignored test requires isolated Rust compilation because its pattern
//! causes stack overflows in larger projects:
//! `runtime_guard_accepts_generic_requested_associated_type_var` (runtime
//! guard-template does not yet support typevar pins). The remaining Rust tests
//! cover VM metadata and formatter behavior that requires Rust-side
//! infrastructure; compile diagnostics now live in native BAML tests.

use baml_fmt::FormatOptions;
use baml_tests::{
    baml_test,
    engine::{OptLevel, compile_source_with_opt},
};
use bex_engine::BexExternalValue;
use bex_vm_types::Object;

fn compiled_function_metadata(source: &str, display_name_suffix: &str) -> (Vec<String>, String) {
    let program = compile_source_with_opt(source, OptLevel::One);
    // Interface bodies live in no name map, so metadata probes walk the
    // named functions plus the pool-enumerated bodies.
    let matches: Vec<_> = baml_tests::engine::named_and_interface_body_functions(&program)
        .filter(|(name, _)| {
            name.strip_prefix("user.")
                .unwrap_or(name)
                .ends_with(display_name_suffix)
        })
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one function ending with `{display_name_suffix}`, got: {:?}",
        matches
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
    );

    let (name, idx) = matches[0];
    let heap = baml_tests::engine::bound_pool(&program);
    let ptr = heap.compile_time_ptr(idx);
    // SAFETY: `ptr` indexes the pool the heap was just built from, and the
    // unsealed heap outlives every read below.
    let Object::Function(function) = (unsafe { ptr.get() }) else {
        panic!("`{name}` did not point at a function object");
    };

    (
        function
            .param_types
            .iter()
            .map(ToString::to_string)
            .collect(),
        function.return_type.to_string(),
    )
}

fn compiled_function_display_metadata(
    source: &str,
    display_name_suffix: &str,
) -> (Vec<String>, Vec<String>, String) {
    let program = compile_source_with_opt(source, OptLevel::One);
    let matches: Vec<_> = baml_tests::engine::named_and_interface_body_functions(&program)
        .filter(|(name, _)| {
            name.strip_prefix("user.")
                .unwrap_or(name)
                .ends_with(display_name_suffix)
        })
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one function ending with `{display_name_suffix}`, got: {:?}",
        matches
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
    );

    let (name, idx) = matches[0];
    let Some(Object::Function(function)) = program.objects.get(idx) else {
        panic!("`{name}` did not point at a function object");
    };

    (
        function.display_type_params.clone(),
        function.display_param_types.clone(),
        function.display_return_type.clone(),
    )
}

#[test]
fn vm_metadata_resolves_concrete_associated_type_projection_return() {
    let (_params, return_type) = compiled_function_metadata(
        r#"
        interface PublicIdentity {
            type Key
            key: Self.Key
        }

        class AccountRecord {
            public_key: string

            implements PublicIdentity {
                type Key = string
                key as public_key
            }
        }

        function get_public_key(account: AccountRecord) -> (AccountRecord as PublicIdentity).Key {
            return account.as<PublicIdentity<Key = string>>.key
        }
        "#,
        "get_public_key",
    );

    assert_eq!(return_type, "string");
}

#[test]
fn vm_metadata_resolves_self_associated_type_return_in_implements_method() {
    let (_params, return_type) = compiled_function_metadata(
        r#"
        interface Repository {
            type Record
            function find(self) -> Self.Record throws never
        }

        class UserRecord {
            name: string
        }

        class UserRepository {
            value: UserRecord

            implements Repository {
                type Record = UserRecord

                function find(self) -> Self.Record {
                    return self.value
                }
            }
        }
        "#,
        "<(user.UserRepository as user.Repository)>.find",
    );

    assert_eq!(return_type, "UserRecord");
}

#[test]
fn vm_metadata_preserves_unresolved_generic_associated_projection_symbolically() {
    let (params, return_type) = compiled_function_metadata(
        r#"
        interface BoxLike {
            type Item
            function get(self) -> Self.Item throws never
        }

        function read_item<T extends BoxLike>(box: T) -> T.Item {
            return box.get()
        }
        "#,
        "read_item",
    );

    // `T` and its associated projection cannot be resolved statically here, but they
    // are *not* erased: the stored signature is a template over the callee frame, so
    // `T` is carried as the frame slot it occupies (`#0`) and the projection keeps its
    // resolved form — the declaring interface is determined at lowering, which is
    // strictly more precise than the bare `T.Item` for runtime resolution. Naming the
    // slot rather than the variable is what lets a *value* of this function
    // substitute the realized args it carries (see `bex_vm`'s `function_object_ty`).
    assert_eq!(params, vec!["#0"]);
    assert_eq!(return_type, "(#0 as BoxLike).Item");
}

#[test]
fn vm_metadata_displays_interface_default_method_self_type() {
    let (generic_params, params, return_type) = compiled_function_display_metadata(
        r#"
        interface Described<T> {
            function label(self) -> string throws never

            function describe(self) -> string throws never {
                return self.label()
            }
        }

        class Widget {
            name: string

            implements Described<string> {
                function label(self) -> string {
                    return self.name
                }
            }
        }
        "#,
        "Described.describe",
    );

    assert_eq!(generic_params, vec!["T"]);
    assert_eq!(params, vec!["Described<T>"]);
    assert_eq!(return_type, "string");
}

#[test]
fn formatter_accepts_associated_type_syntax() {
    let source = r#"
        class TextFormat {}

        interface Codec<Format> {
            type Output

            function decode(self, input: string) -> Self.Output throws never
        }

        class Document {
            raw: string

            implements Codec<TextFormat> {
                type Output = string

                function decode(self, input: string) -> string {
                    return self.raw
                }
            }
        }

        type TextOut = (Document as Codec<TextFormat, Output = string>).Output

        interface Source {
            type Item

            function get(self) -> Self.Item throws never
        }

        type TrailingSource = Source<Item = int,>
        type MultilineSource = Source<
            // item witness
            Item = int,
        >

        function takes_bound<S extends Source<Item = int | string>>(source: S) -> S.Item {
            return source.get()
        }

        function decode_as(doc: Document) -> (Document as Codec<TextFormat, Output = string>).Output {
            let output: (Document as Codec<TextFormat>).Output = doc.as<Codec<TextFormat, Output = string>>.decode("")
            return output
        }
        "#;

    let formatted = baml_fmt::format(source, &FormatOptions::default())
        .expect("formatter should accept associated type syntax");
    assert!(formatted.contains("type Output = string"));
    assert!(formatted.contains("Codec<TextFormat, Output = string>"));
    assert!(formatted.contains("(Document as Codec<TextFormat>).Output"));
    assert!(formatted.contains("S extends Source<Item = int | string>"));
    assert!(formatted.contains("TrailingSource"));
    assert!(formatted.contains("// item witness"));
}

// Return types are covariant in BAML (like throws): the interface declares
// `Self.Item | string`, which realizes to `int | string` at `IntProducer`, and
// the override's narrower `int` return conforms (`int <: int | string`).
// Conformance is whole-function subtyping — params contravariant, return and
// throws covariant — not the old checker's exact match.

#[tokio::test]
#[ignore = "Runtime match guards do not yet handle a type variable in the requested associated-type pin (`Source<Item = T>`), so the guard over-matches. Compiler-side typing is correct; un-ignore when the runtime guard-template supports typevar pins."]
async fn runtime_guard_accepts_generic_requested_associated_type_var() {
    // The parameter pins both admissible realizations (an existential value type
    // must pin its associated types); the *type pattern* then requests the pin at
    // the function's own generic (`Source<Item = T>`), so the runtime guard
    // filters by the realized binding: the int realization matches at
    // `score<int>`, the string one falls through.
    let output = baml_test!(
        r#"
        interface Source {
            type Item
        }

        class IntSource {
            implements Source {
                type Item = int
            }
        }

        class StringSource {
            implements Source {
                type Item = string
            }
        }

        function score<T>(source: Source<Item = T> | Source<Item = string>) -> int {
            return match (source) {
                let matching: Source<Item = T> => 1,
                _ => 0,
            }
        }

        function main() -> int {
            return score<int>(IntSource {}) * 10 + score<int>(StringSource {})
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(10));
}

// `Bucket<L>` + `Bucket<R>` on `Pair<L, R>` realize the same interface
// `Bucket<T>` at the diagonal `Pair<T, T>`, so they overlap and are rejected —
// even when the interface and impls live in different namespaces.

// `Bucket<L>` + `Bucket<R>` on `Pair<L, R>` realize the same interface
// `Bucket<T>` at the diagonal `Pair<T, T>`, so they overlap and are rejected.
