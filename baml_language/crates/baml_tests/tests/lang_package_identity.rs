//! The runtime reaches the standard library by identity, never by a name.
//!
//! A package has no intrinsic name (TYPE_SYSTEM.md § Declarations, Packages,
//! and Names). The VM's native code is the language's own implementation, not
//! a package with an edge table, so when it needs a stdlib declaration it
//! names the language package by identity and the item by its path there —
//! whatever the program calls its packages. These need a root that carries a
//! stdlib package's name, which only the harness can build, so they live here
//! rather than in the BAML corpus.

use baml_base::Name;
use baml_compiler2_emit::OptLevel;
use baml_db::{ProjectDatabase, SourceRootKind, SourceRootSpec, compile_program};
use baml_tests::engine::{TestDbExt, run_compiled};
use bex_external_types::BexExternalValue;
use indexmap::IndexMap;

/// A root whose manifest names it `baml` — the label the prelude's core
/// package also carries — and that declares its own `toml.Table` and
/// `toml.ParseError`, so a lookup by that name and path would find the
/// root's classes first. Native code building a `baml.toml.Table`, raising
/// a `baml.toml.ParseError`, or dispatching `baml.ToJson` finds the
/// language package's: the parse result has the stdlib's `to_json`, and the
/// error is the class the `catch` arm names.
#[tokio::test]
async fn a_root_named_like_a_language_package_does_not_capture_stdlib_lookups() {
    let mut db = ProjectDatabase::new();
    db.ensure_stdlib_sources();
    let root = db
        .add_source_root(
            SourceRootSpec::new("/named-baml", SourceRootKind::Workspace).named(Name::new("baml")),
        )
        .expect("a workspace root may carry any manifest name");
    db.file(
        "/named-baml/ns_toml/toml.baml",
        r#"
        class Table {
            decoy int
        }

        class ParseError {
            decoy string
        }
        "#,
    );
    db.file(
        "/named-baml/main.baml",
        r#"
        function parsed() -> string {
            string.from(baml.toml.Table.parse("a = 1").to_json())
        }

        function failed() -> string {
            {
                baml.toml.Table.parse("a = ");
                "parsed"
            } catch (e) {
                baml.toml.ParseError => "parse error"
            }
        }
        "#,
    );
    let program = compile_program(&db, root, OptLevel::One).expect("the program compiles");
    let output = run_compiled(program.clone(), "baml.parsed", IndexMap::new(), false).await;
    assert_eq!(
        output.result.expect("`parsed` runs"),
        BexExternalValue::String("{\"a\": 1}".into())
    );
    let output = run_compiled(program, "baml.failed", IndexMap::new(), false).await;
    assert_eq!(
        output.result.expect("`failed` runs"),
        BexExternalValue::String("parse error".into())
    );
}
