//! Integration tests for `baml_compiler2_emit`.
//!
//! Each test creates a minimal DB, adds a `.baml` file, runs the full
//! compiler2 pipeline through the build-time, byte-identical stdlib prefix,
//! and verifies the resulting `Program` has the expected structure.
//! Basic function, enum, and class execution is covered by the native suites
//! in `baml_src/ns_prompt_parameter`, `ns_enums`, and `ns_classes`.

use baml_db::{CheckedWorld, ProjectDatabase};

use crate::{
    engine::TestDbExt,
    stdlib_prefix::{OptLevel, prefix},
};

const SNAPSHOT_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/snapshots/compiler2_emit");
const OPTIONAL_DEFAULTS_SOURCE: &str = r#"
function add(base: int, amount: int = base + 2) -> int {
  base + amount
}

function main() -> int {
  add(5)
}
"#;

fn make_db() -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.workspace(std::path::Path::new("."));
    db.set_seeded_stdlib_interface(prefix(OptLevel::Two).interfaces.clone());
    db
}

fn compile(db: &ProjectDatabase) -> bex_vm_types::Program {
    let package = db
        .workspace_root()
        .unwrap_or_else(|| unreachable!("`make_db` adds one workspace root"));
    CheckedWorld::collect(db, package)
        .expect("the world checks")
        .compile_with(OptLevel::Two, prefix(OptLevel::Two))
        .expect("compilation should succeed")
}

/// The object index of the callable rendered `name` — `pkg.ns.name` for a
/// free function, `pkg.ns.Class.name` for a method — from the package tables.
fn callable(program: &bex_vm_types::Program, name: &str) -> usize {
    program
        .rendered_callables()
        .get(name)
        .unwrap_or_else(|| {
            panic!(
                "`{name}` is not a callable of the program; callables: {:?}",
                rendered_names(program)
            )
        })
        .object
        .raw()
}

/// Every rendered callable name, sorted, for diagnostics.
fn rendered_names(program: &bex_vm_types::Program) -> Vec<String> {
    let mut names: Vec<String> = program.rendered_callables().into_keys().collect();
    names.sort();
    names
}

/// The workspace package (spelled `user`) and its ordinal in the executable.
fn user_package(program: &bex_vm_types::Program) -> (u32, &bex_vm_types::types::ProgramPackage) {
    (program.root, &program.packages[program.root as usize])
}

#[test]
fn typed_pattern_emits_atomic_narrow_bind() {
    use bex_vm_types::Instruction;

    let mut db = make_db();
    db.file(
        "test.baml",
        r#"
class Foo { field: int }

function main(x: Foo | int) -> int {
  let task = spawn { x = 0; };
  match (x) {
    let foo: Foo => foo.field,
    let n: int => n,
  }
}
"#,
    );
    let program = compile(&db);
    let main_idx = callable(&program, "user.main");
    let bex_vm_types::Object::Function(main) = &(*program.objects)[main_idx] else {
        panic!("expected user.main to be a function")
    };

    let (narrow_bind_idx, destination) = main
        .bytecode
        .instructions
        .iter()
        .enumerate()
        .find_map(|(idx, instruction)| match instruction {
            Instruction::NarrowBind { destination, .. } => Some((idx, *destination)),
            _ => None,
        })
        .expect("narrow_bind instruction");
    assert!(
        main.bytecode
            .instructions
            .iter()
            .skip(narrow_bind_idx + 1)
            .any(|instruction| match instruction {
                Instruction::LoadVar(slot)
                | Instruction::StoreVarLoadVar(slot)
                | Instruction::LoadVarField(slot, _)
                | Instruction::LoadVarFieldLocal(slot, _)
                | Instruction::AddLocalSmall(_, slot, _) => *slot == destination,
                Instruction::LoadVar2(first, second) => {
                    *first == destination || *second == destination
                }
                _ => false,
            }),
        "{:?}",
        main.bytecode.instructions
    );
}

macro_rules! emit_snapshot {
    ($name:expr, $output:expr) => {
        assert_compiler2_snapshot!(SNAPSHOT_PATH, $name, $output);
    };
}

#[test]
fn builtin_functions_included() {
    let mut db = make_db();
    db.file("test.baml", "function f() -> string { return \"x\"; }");
    let program = compile(&db);
    // Builtins from the baml and env packages should be present
    let names = rendered_names(&program);
    let has_baml = names.iter().any(|k| k.starts_with("baml."));
    let has_baml_env = names.iter().any(|k| k.starts_with("baml.env."));
    assert!(
        has_baml,
        "expected at least one 'baml.*' function, got: {names:?}"
    );
    assert!(
        has_baml_env,
        "expected at least one 'baml.env.*' function, got: {names:?}"
    );
}

#[test]
fn optional_param_metadata_and_omitted_sentinel_emit() {
    let mut db = make_db();
    db.file("test.baml", OPTIONAL_DEFAULTS_SOURCE);
    let program = compile(&db);

    let add_idx = callable(&program, "user.add");
    let bex_vm_types::Object::Function(add) = &(*program.objects)[add_idx] else {
        panic!("expected user.add to be a function");
    };
    assert_eq!(add.param_has_default, vec![false, true]);
    assert!(
        add.bytecode
            .constants
            .iter()
            .any(|c| matches!(c, bex_vm_types::ConstValue::OmittedArg)),
        "expected default prologue to compare against OmittedArg"
    );

    // The call site's sentinel, where the call is still a call: at `Two` it
    // is inlined and the omission folds away with it.
    let unoptimized = CheckedWorld::collect(&db, db.workspace_root().expect("workspace root"))
        .expect("the world checks")
        .compile_with(OptLevel::One, prefix(OptLevel::One))
        .expect("compilation should succeed");
    let main_idx = callable(&unoptimized, "user.main");
    let bex_vm_types::Object::Function(main) = &(*unoptimized.objects)[main_idx] else {
        panic!("expected user.main to be a function");
    };
    assert!(
        main.bytecode
            .constants
            .iter()
            .any(|c| matches!(c, bex_vm_types::ConstValue::OmittedArg)),
        "expected omitted source argument to be emitted as OmittedArg"
    );

    emit_snapshot!(
        "optional_defaults_emit_snapshot",
        crate::engine::display_user_functions(&program)
    );
}

// ─── Phase 3 let-binding tests ────────────────────────────────────────────────
//
// Note: `set_synthetic_items_for_file` was removed from the DB trait as part of
// the compiler2 migration (Phase 2). These tests now use actual BAML source
// declarations (`client Name = <expr>;`) which produce `Item::Let` bindings
// and exercise the same let-binding infrastructure.

/// Verify that a client declaration:
/// - Produces a let binding with a global slot
/// - Gives the user package an `$init`, scheduled in the init order
#[test]
fn let_binding_global_slot_and_init_function() {
    let mut db = make_db();
    db.file(
        "test.baml",
        r#"
        client MyClient = openai.ResponsesClient.new(model = "gpt-4");
        function f() -> string { return "x"; }
        "#,
    );

    let program = compile(&db);

    // The client let binding owns a global slot.
    let lets = program.rendered_lets();
    assert!(
        lets.keys().any(|k| k.contains("MyClient")),
        "expected 'MyClient' among the lets, got: {:?}",
        lets.keys().collect::<Vec<_>>()
    );

    // $init should be synthesized and scheduled.
    let (ordinal, user) = user_package(&program);
    assert!(
        user.init.is_some(),
        "expected the user package to own an `$init`"
    );
    assert!(
        program.init_order.contains(&ordinal),
        "expected the user package in the init order, got: {:?}",
        program.init_order
    );
}

// ─── Test file globals ──────────────────────────────────────────────────────

/// A file with `test` blocks declares one `$tests_<file>` global, written by
/// the package's `$init`: the declaration the testing package finds a
/// package's tests through. A file without tests declares none.
#[test]
fn test_blocks_declare_one_test_file_global() {
    let test_globals = |program: &bex_vm_types::Program| -> usize {
        user_package(program)
            .1
            .globals
            .keys()
            .filter(|path| {
                matches!(path, bex_vm_types::DeclPath::Let(name)
                    if name.name.as_str().starts_with("$tests_"))
            })
            .count()
    };

    let mut db = make_db();
    db.file(
        "test.baml",
        r#"
        test "foo" {
          assert.is_true(true)
        }

        test "bar" {
          assert.is_true(true)
        }
        "#,
    );
    let program = compile(&db);
    assert_eq!(test_globals(&program), 1);
    assert!(
        user_package(&program).1.init.is_some(),
        "the package's `$init` writes the test file global"
    );

    let mut db = make_db();
    db.file(
        "test.baml",
        "function greet(name: string) -> string { return name; }",
    );
    assert_eq!(test_globals(&compile(&db)), 0);
}

/// Verify that multiple client declarations:
/// - Both own global slots
/// - `$init` is synthesized to initialize them
#[test]
fn multiple_let_bindings_with_valid_dependencies() {
    let mut db = make_db();
    db.file(
        "test.baml",
        r#"
        client ClientA = openai.ResponsesClient.new(model = "gpt-4");
        client ClientB = openai.ResponsesClient.new(model = "gpt-3.5-turbo");
        function f() -> string { return "x"; }
        "#,
    );

    let program = compile(&db);

    // Both clients own global slots.
    let lets = program.rendered_lets();
    assert!(
        lets.keys().any(|k| k.contains("ClientA")),
        "expected 'ClientA' among the lets, got: {:?}",
        lets.keys().collect::<Vec<_>>()
    );
    assert!(
        lets.keys().any(|k| k.contains("ClientB")),
        "expected 'ClientB' among the lets, got: {:?}",
        lets.keys().collect::<Vec<_>>()
    );

    // $init should be synthesized
    assert!(
        user_package(&program).1.init.is_some(),
        "expected the user package to own an `$init`"
    );
}

/// `InterfaceDef::fields` is the interface's field *index space*: every
/// implementation's `RuntimeImplRule::field_links` is baked parallel to it, and a
/// virtual field access carries a position into it. So the list must hold every
/// declared field, in declaration order, with nothing dropped.
///
/// The regression this pins: a field whose type mentions `Self` or an associated
/// type (`key: Self.Key`) used to fail runtime lowering and be silently filtered
/// out, shifting every later field's index.
#[test]
fn interface_field_index_space_keeps_every_declared_field() {
    let mut db = make_db();
    db.file(
        "test.baml",
        r#"
interface Shelf {
  type Key

  label: string
  key: Self.Key
  count: int
}

class Book {
  label: string
  key: string
  count: int

  implements Shelf {
    type Key = string
  }
}

function main() -> int { 0 }
"#,
    );
    let program = compile(&db);

    let iface = (*program.objects)
        .iter()
        .find_map(|obj| match obj {
            bex_vm_types::Object::Interface(i) if i.path.name.as_str() == "Shelf" => Some(i),
            _ => None,
        })
        .expect("Shelf interface object should be emitted");

    let names: Vec<&str> = iface.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        ["label", "key", "count"],
        "every declared field must keep its declared position",
    );

    // The `Self.Key` field stays symbolic — a declaration has no implementor, so it
    // is resolved against the receiver's impl at run time rather than erased here.
    assert!(
        matches!(
            iface.fields[1].ty,
            baml_type::RuntimeTy::AssociatedTypeProjection { .. }
        ),
        "`key: Self.Key` should stay an associated-type projection, got {:?}",
        iface.fields[1].ty,
    );
    assert!(
        matches!(iface.fields[2].ty, baml_type::RuntimeTy::Int),
        "field after the projection must keep its own type, got {:?}",
        iface.fields[2].ty,
    );
}

/// `RuntimeImplRule::field_links` is the interface-field-index → class-slot table a
/// virtual field access indexes. It must be ordered by the *interface's* field
/// declarations — not by the class's field order, and not by the order the
/// `field as class_field` links happen to be written.
#[test]
fn impl_rule_field_links_are_ordered_by_the_interface() {
    let mut db = make_db();
    db.file(
        "test.baml",
        r#"
interface Shelf {
  label: string
  count: int
}

class Book {
  // Deliberately declared in a different order than `Shelf` lists them, and
  // with an unrelated field first, so a table built from the class's layout or
  // from the link order would disagree with one built from the interface's.
  isbn: string
  count: int
  title: string

  implements Shelf {
    count as count
    label as title
  }
}

function main() -> int { 0 }
"#,
    );
    let program = compile(&db);

    let class = (*program.objects)
        .iter()
        .find_map(|obj| match obj {
            bex_vm_types::Object::Class(c) if c.path.name.as_str() == "Book" => Some(c),
            _ => None,
        })
        .expect("Book class object should be emitted");
    let slot = |name: &str| {
        class
            .fields
            .iter()
            .position(|f| f.name == name)
            .unwrap_or_else(|| panic!("Book should have a `{name}` field"))
    };

    let rules: Vec<_> = program
        .packages
        .iter()
        .flat_map(|pkg| pkg.impl_rules.values().flatten())
        .filter(|rule| !rule.field_links.is_empty())
        .collect();
    assert_eq!(
        rules.len(),
        1,
        "expected exactly one field-bearing impl rule"
    );

    // Positional over `Shelf`'s declarations: index 0 is `label` (linked to
    // `title`), index 1 is `count` (same-named).
    assert_eq!(
        rules[0].field_links.as_ref(),
        [slot("title") as u32, slot("count") as u32],
        "field_links must be indexed by the interface's field order",
    );
}

/// The same-name default is applied at bake time, so an `implements` block that
/// writes no links at all still produces a complete table.
#[test]
fn impl_rule_field_links_fill_the_same_name_default() {
    let mut db = make_db();
    db.file(
        "test.baml",
        r#"
interface Named {
  name: string
}

class Person {
  age: int
  name: string

  implements Named {}
}

function main() -> int { 0 }
"#,
    );
    let program = compile(&db);

    let rule = program
        .packages
        .iter()
        .flat_map(|pkg| pkg.impl_rules.values().flatten())
        .find(|rule| !rule.field_links.is_empty())
        .expect("expected a field-bearing impl rule");
    // `name` is `Person`'s second field, so an unlinked interface field must still
    // resolve to slot 1 rather than defaulting to 0.
    assert_eq!(rule.field_links.as_ref(), [1]);
}

/// BAML runtime assertions cannot observe handler identities or PC ranges.
/// Keep every clause distinct through optimized and compact bytecode.
#[test]
fn chained_catches_keep_separate_bytecode_handlers() {
    use bex_vm_types::Instruction;

    for opt in [OptLevel::Zero, OptLevel::Two] {
        for first in ["catch", "catch_all", "catch_all_panics"] {
            for second in ["catch", "catch_all", "catch_all_panics"] {
                let mut db = ProjectDatabase::new();
                db.workspace(std::path::Path::new("."));
                db.set_seeded_stdlib_interface(prefix(opt).interfaces.clone());
                db.file(
                    "test.baml",
                    &format!(
                        r#"
function fail(n: int) -> int throws string {{
    if (n == 0) {{ throw "original" }}
    n
}}
function main(n: int) -> int {{
    fail(n) {first} (original) {{
        _ => throw 42,
    }} {second} (replacement) {{
        int => 7,
        baml.panics.Panic => -1,
    }}
}}
"#
                    ),
                );
                let package = db.workspace_root().expect("workspace root");
                let program = CheckedWorld::collect(&db, package)
                    .expect("the world checks")
                    .compile_with(opt, prefix(opt))
                    .expect("valid chained catches");
                let index = callable(&program, "user.main");
                let bex_vm_types::Object::Function(main) = &(*program.objects)[index] else {
                    panic!("expected function");
                };
                let code = &main.bytecode;
                let handlers: std::collections::BTreeSet<_> = code
                    .exception_table
                    .iter()
                    .map(|entry| entry.handler_pc)
                    .collect();
                assert_eq!(handlers.len(), 2, "{first} then {second} at {opt:?}");
                let inner = handlers
                    .iter()
                    .copied()
                    .find(|&pc| {
                        code.exception_handlers_for_pc(pc)
                            .any(|entry| entry.handler_pc != pc)
                    })
                    .expect("the first handler is protected by the second handler");
                let outer = code
                    .exception_handlers_for_pc(inner)
                    .next()
                    .expect("outer handler entry")
                    .handler_pc;
                assert_ne!(inner, outer);
                assert!(
                    code.exception_handlers_for_pc(outer).next().is_none(),
                    "the last handler must not catch its own failures"
                );
                // `fail`'s own throw may be inlined ahead of the first arm's.
                assert!(
                    code.instructions
                        .iter()
                        .enumerate()
                        .filter(|(_, op)| matches!(op, Instruction::Throw))
                        .any(|(throw_pc, _)| code
                            .exception_handlers_for_pc(throw_pc)
                            .any(|entry| entry.handler_pc == outer)),
                    "throws in the first arm must enter the second handler"
                );
                let compact = code.lower_to_compact();
                let handlers: std::collections::BTreeSet<_> = compact
                    .exception_table
                    .iter()
                    .map(|entry| entry.handler_pc)
                    .collect();
                assert_eq!(
                    handlers.len(),
                    2,
                    "compact {first} then {second} at {opt:?}"
                );
                assert!(
                    handlers.iter().any(|&pc| compact
                        .exception_handlers_for_pc(pc)
                        .any(|entry| entry.handler_pc != pc)),
                    "compact code preserves nesting"
                );
            }
        }
    }
}
