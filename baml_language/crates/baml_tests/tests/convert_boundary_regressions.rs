//! Regression tests for the TIR→`RuntimeTy` conversion boundary
//! (`ResolvedAliases::convert`), which panics when the error-recovery
//! sentinel `Ty::Error` reaches it. These cover producers that previously
//! leaked a sentinel into runtime lowering for *valid* programs, and the
//! gate (`CheckedWorld`) that keeps an error-bearing world out of emit.

use baml_db::ProjectDatabase;
use baml_tests::engine::TestDbExt;

fn db_with(src: &str) -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.workspace(std::path::Path::new("."));
    db.file("main.baml", src);
    db
}

/// The workspace root the fixture builder added: the package whose program
/// the test compiles.
fn package(db: &ProjectDatabase) -> baml_db::SourceRoot {
    db.workspace_root()
        .unwrap_or_else(|| unreachable!("the fixture builder adds one workspace root"))
}

/// Lower the project to bytecode and return the `Program` for inspection.
fn compile_program(db: &ProjectDatabase) -> bex_vm_types::Program {
    baml_db::CheckedWorld::collect(db, package(db))
        .expect("the world checks")
        .compile(baml_compiler2_emit::OptLevel::Two)
        .expect("should compile to bytecode")
}

/// An error-bearing program (here, an unresolved parameter type) records
/// the error sentinel. The in-process / runtime-eval entry point
/// (`ProjectDatabase::get_bytecode`) must gate on a clean diagnostic pass and
/// return a recoverable `CompileProgramError`, not panic at the convert
/// boundary.
#[test]
fn error_bearing_program_returns_recoverable_error() {
    let db = db_with("function f(a: NonexistentType) -> int { 0 }");
    match db.get_bytecode(package(&db)) {
        Ok(_) => panic!("expected a CompileProgramError for an error-bearing program"),
        Err(baml_db::CompileProgramError::WorldHasErrors(refused)) => {
            assert!(
                !refused.errors().is_empty(),
                "the refusal carries the errors"
            );
        }
        Err(other) => panic!("expected WorldHasErrors, got: {other}"),
    }
}

/// The gate's negative control: emit reached WITHOUT the world's check (the
/// lower layer called directly) on an error-bearing program trips code
/// generation's own probe in a debug build, rather than lowering the error
/// sentinel.
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "the error sentinel survived to code generation")]
fn emit_bypassing_the_check_trips_the_probe() {
    let db = db_with("function f(a: NonexistentType) -> int { 0 }");
    let _ =
        baml_compiler2_emit::emit_package(&db, package(&db), baml_compiler2_emit::OptLevel::Two);
}

/// An error in a source-bearing DEPENDENCY is an error of the world the root
/// compiles, even though the root's own files are clean: emit lowers the
/// whole world, so the gate counts it and returns the recoverable error
/// rather than reaching the convert boundary with the dependency's sentinel.
#[test]
fn dependency_error_returns_recoverable_error() {
    let mut db = db_with("function f() -> int throws never { 0 }");
    db.dependency("dep");
    db.file(
        "<builtin>/dep/lib.baml",
        "function g(a: NonexistentType) -> int throws never { 0 }",
    );
    let Err(error) = db.get_bytecode(package(&db)) else {
        panic!("expected a CompileProgramError for a world with a dependency error");
    };
    let baml_db::CompileProgramError::WorldHasErrors(refused) = &error else {
        panic!("expected WorldHasErrors, got: {error}");
    };
    assert_eq!(
        refused.errors().len(),
        1,
        "the dependency's one error is the world's"
    );
    // A playground shows only its root's files, so the build status must name
    // the dependency's error and where it is.
    assert_eq!(
        error.describe(&db),
        "lib.baml:1:15: [E0002] unresolved type: NonexistentType"
    );
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
    let idx =
        baml_tests::engine::function_index(&program, "user.f").expect("user.f should be compiled");
    let Some(bex_vm_types::Object::Function(func)) = program.objects.get(idx) else {
        panic!("user.f should resolve to a function object");
    };
    // An emitted program's heads carry the tag the linker assigned their
    // declaration, so the throws type is checked by identity rather than by
    // rendered name.
    let expected = baml_tests::engine::declared_type_tag(&program, "user.MyError")
        .expect("user.MyError should be declared");
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
