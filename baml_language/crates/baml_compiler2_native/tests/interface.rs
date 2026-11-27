//! What a library may export: the refusals of `interface.rs`, made when a
//! library is packed, each naming the function and the type.

use baml_compiler2_native::{export_name, export_refusal, library_interface};
use baml_db::{
    ProjectDatabase, SourceRootKind, SourceRootSpec,
    baml_compiler2_hir::{item_data::file_functions, loc::FunctionLoc},
};

const PROGRAM: &str = r#"
class Box<T> {
  v T
}

class Holder {
  b Box<int>
}

class Plain {
  a int
}

class Wrapped {
  inner Plain @json.flatten
  b int
}

function identity<T>(x: T) -> T {
  x
}

function make_box() -> Box<int> {
  Box { v: 1 }
}

function holder_value(h: Holder) -> int {
  h.b.v
}

function wrapped_b(w: Wrapped) -> int {
  w.b
}

function env_or(name: string, fallback: string) -> string {
  let value = baml.env.get(name);
  if (value != null) { value } else { fallback }
}

function plain_a(p: Plain) -> int {
  p.a
}
"#;

fn with_functions(check: impl FnOnce(&ProjectDatabase, &[FunctionLoc<'_>])) {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("program.baml");
    std::fs::write(&path, PROGRAM).expect("the program is written");
    let mut db = ProjectDatabase::new();
    db.ensure_stdlib_sources();
    let root = db
        .add_source_root(SourceRootSpec::new(
            directory.path(),
            SourceRootKind::Workspace,
        ))
        .expect("a source root");
    db.add_or_update_file_in(root, &path, PROGRAM);
    let functions: Vec<_> = db
        .workspace_files()
        .into_iter()
        .flat_map(|file| file_functions(&db, file).iter().copied())
        .collect();
    check(&db, &functions);
}

fn find<'db>(
    db: &'db ProjectDatabase,
    functions: &[FunctionLoc<'db>],
    name: &str,
) -> FunctionLoc<'db> {
    *functions
        .iter()
        .find(|function| export_name(db, **function) == name)
        .unwrap_or_else(|| panic!("no function {name}"))
}

fn refusal(name: &str) -> Option<String> {
    let mut why = None;
    with_functions(|db, functions| why = export_refusal(db, find(db, functions, name)));
    why
}

#[test]
fn a_function_with_type_parameters_is_refused() {
    let why = refusal("identity").expect("refused");
    assert!(why.contains("type parameters"), "{why}");
}

#[test]
fn a_generic_result_is_refused() {
    let why = refusal("make_box").expect("refused");
    assert!(why.contains("a generic class `Box`"), "{why}");
}

#[test]
fn a_generic_class_inside_a_field_is_refused() {
    let why = refusal("holder_value").expect("refused");
    assert!(
        why.contains("a generic class `Box`") && why.contains("field `b`"),
        "{why}"
    );
}

#[test]
fn a_class_the_library_cannot_describe_is_refused_by_name() {
    let why = refusal("wrapped_b").expect("refused");
    assert!(why.contains("Wrapped"), "{why}");
}

#[test]
fn a_throws_type_that_cannot_cross_does_not_refuse_the_function() {
    assert_eq!(refusal("env_or"), None);
    with_functions(|db, functions| {
        let interface =
            library_interface(db, &[find(db, functions, "env_or")]).expect("exportable");
        let function = &interface.functions[0];
        assert!(function.throws.is_some(), "it throws `errors.Io`");
        assert!(!function.throws_typed, "the throws type does not cross");
    });
}

#[test]
fn a_plain_function_is_exported() {
    assert_eq!(refusal("plain_a"), None);
    with_functions(|db, functions| {
        let error = library_interface(db, &[find(db, functions, "make_box")])
            .expect_err("a generic result");
        assert!(error.to_string().contains("make_box"), "{error}");
    });
}
