//! Regression tests for the TIR→`RuntimeTy` conversion boundary
//! (`ResolvedAliases::convert`), which panics when an inference-only
//! `Ty::Unknown`/`Ty::Error` reaches it. These cover producers that previously
//! leaked `Unknown` into runtime lowering for *valid* programs.

use baml_compiler2_emit::CompileOptions;
use baml_db::ProjectDatabase;
use baml_tests::engine::TestDbExt;

fn db_with(src: &str) -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.workspace(std::path::Path::new("."));
    db.file("main.baml", src);
    db
}

/// Lower the project to bytecode. A panic here (e.g. `Ty::Unknown` reaching the
/// convert boundary) fails the test directly.
fn bytecode_ok(db: &ProjectDatabase) -> Result<(), String> {
    baml_compiler2_emit::generate_project_bytecode(
        db,
        &CompileOptions {
            emit_test_cases: false,
        },
    )
    .map(|_| ())
    .map_err(|e| format!("{e:?}"))
}

/// Lower the project to bytecode and return the `Program` for inspection.
fn compile_program(db: &ProjectDatabase) -> bex_vm_types::Program {
    baml_compiler2_emit::generate_project_bytecode(
        db,
        &CompileOptions {
            emit_test_cases: false,
        },
    )
    .expect("should compile to bytecode")
}

/// `throw <non-literal expression>` in a function with no `throws` clause.
///
/// The HIR-level throw-fact typer can only name literal/path/object operands;
/// a call/binary/array result falls through. It must over-approximate to
/// `unknown` (a real runtime type), not leak `Ty::Unknown` (which has no
/// runtime representation) into the throws metadata and panic at codegen. These
/// are valid, zero-diagnostic programs.
#[test]
fn throw_of_non_literal_expression_compiles() {
    for src in [
        "class E { m string } function mk() -> E { E { m: \"x\" } } \
         function boom() -> int { throw mk() } function c() -> int { boom() }",
        "function boom() -> int { throw 1 + 2 } function c() -> int { boom() }",
        "function boom() -> int { throw [1, 2, 3] } function c() -> int { boom() }",
    ] {
        let db = db_with(src);
        // These are valid programs — the panic was a producer leaking `Unknown`.
        baml_db::testing::assert_no_diagnostic_errors(&db);
        assert!(
            bytecode_ok(&db).is_ok(),
            "should compile to bytecode: {src}"
        );
    }
}

/// A generic LLM function whose stream-expanded return type embeds a typevar
/// (`-> Box<T>`). The stream-return lowering must thread the function's generic
/// params so `T` lowers to a faithful `TypeVar`, not `Ty::Unknown`.
#[test]
fn generic_llm_function_with_generic_return_compiles() {
    let db = db_with(
        "class Box<T> { value T }\n\
         client Dummy = openai.ChatClient.new(model = \"gpt-4\")\n\
         function Extract<T>(text: string) -> Box<T> { client: Dummy\nprompt: `x` }\n",
    );
    baml_db::testing::assert_no_diagnostic_errors(&db);
    assert!(bytecode_ok(&db).is_ok());
}

/// An error-bearing program (here, an unresolved parameter type) produces
/// inference-only `Unknown` types. The in-process / runtime-eval entry point
/// (`ProjectDatabase::get_bytecode`) must gate on a clean diagnostic pass and
/// return a recoverable `LoweringError`, not panic at the convert boundary.
#[test]
fn error_bearing_program_returns_recoverable_error() {
    let db = db_with("function f(a: NonexistentType) -> int { 0 }");
    match db.get_bytecode() {
        Ok(_) => panic!("expected a LoweringError for an error-bearing program"),
        Err(baml_compiler2_emit::LoweringError::ProjectHasErrors { error_count }) => {
            assert!(error_count > 0, "expected a positive error count");
        }
        Err(other) => panic!("expected ProjectHasErrors, got: {other}"),
    }
}

/// `throw e` where `e` is a *parameter* thrown OUTSIDE a same-named `catch (e)`
/// arm is a real throw of the parameter, not a rethrow of the caught value. Its
/// type must appear in the function's inferred throws metadata — rethrow
/// detection is scoped to the catch arm, not the whole body.
#[test]
fn thrown_parameter_named_like_a_catch_binding_is_not_a_rethrow() {
    let db = db_with(
        "class MyError { msg string }\n\
         class OtherError { msg string }\n\
         function risky() -> int throws OtherError { throw OtherError { msg: \"x\" } }\n\
         function f(e: MyError, flag: bool) -> int {\n\
           let x = risky() catch (e) { _ => 0 };\n\
           if flag { throw e }\n\
           return x\n\
         }\n",
    );
    baml_db::testing::assert_no_diagnostic_errors(&db);
    let program = compile_program(&db);
    let idx = program
        .function_index("user.f")
        .expect("user.f should be compiled");
    let Some(bex_vm_types::Object::Function(func)) = program.objects.get(idx) else {
        panic!("user.f should resolve to a function object");
    };
    // An emitted program's heads are tag-only until the loader binds them, so
    // the throws type is checked by identity rather than by rendered name.
    let expected = baml_type::typetag::TypeTag::of_head("user.MyError");
    let mut found = false;
    func.throws_type.visit_heads(&mut |head| {
        if head.tag() == expected {
            found = true;
        }
    });
    assert!(
        found,
        "f throws its `MyError` parameter outside the catch arm, but the throws \
         metadata omitted it: {:?}",
        func.throws_type
    );
}
