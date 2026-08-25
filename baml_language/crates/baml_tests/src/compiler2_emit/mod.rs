//! Integration tests for `baml_compiler2_emit`.
//!
//! Each test creates a minimal DB, adds a `.baml` file, runs the full
//! compiler2 pipeline through `generate_project_bytecode`, and verifies
//! the resulting `Program` has the expected structure.

use baml_compiler2_emit::{CompileOptions, generate_project_bytecode};
use baml_db::ProjectDatabase;

use crate::engine::TestDbExt;

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
    db
}

fn compile(db: &ProjectDatabase) -> bex_vm_types::Program {
    generate_project_bytecode(
        db,
        &CompileOptions {
            emit_test_cases: false,
        },
    )
    .expect("compilation should succeed")
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
    let main_idx = program.function_indices["user.main"];
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
                Instruction::LoadVar(slot) | Instruction::StoreVarLoadVar(slot) => {
                    *slot == destination
                }
                Instruction::LoadVar2(first, second) => {
                    *first == destination || *second == destination
                }
                _ => false,
            }),
        "{:?}",
        main.bytecode.instructions
    );
}

#[test]
fn explicit_local_id_selects_runtime_id_bytecodes_only_for_tagged_calls() {
    use bex_vm_types::Instruction;

    let mut db = make_db();
    db.file(
        "test.baml",
        r#"
function leaf(n: int) -> int { n }

function main(call_id: boundary.LocalId, sysop_id: boundary.LocalId) -> int throws baml.errors.Io {
  let plain = leaf(0)
  let tagged = leaf(1, $id = call_id)
  baml.sys.sleep(baml.time.Duration.from_milliseconds(0n), $id = sysop_id)
  plain + tagged
}
"#,
    );
    let program = compile(&db);
    let main_idx = program.function_indices["user.main"];
    let bex_vm_types::Object::Function(main) = &(*program.objects)[main_idx] else {
        panic!("expected user.main to be a function")
    };

    assert!(
        main.bytecode
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::Call { .. }))
    );
    assert!(
        main.bytecode
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::CallWithRuntimeId { .. }))
    );
    assert!(
        main.bytecode
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::SysOpWithRuntimeId(_)))
    );
}

#[test]
fn explicit_local_id_selects_indirect_optional_and_virtual_bytecodes() {
    use bex_vm_types::Instruction;

    let mut db = make_db();
    db.file(
        "test.baml",
        r#"
interface Speaker {
  function speak(self) -> int throws never
}

class Dog {
  function speak(self) -> int { 1 }
}

implements Speaker for Dog {}

function indirect(callback: (int) -> int throws never, id: boundary.LocalId) -> int {
  callback(1, $id = id)
}

function optional(callback: ((int) -> int throws never)?, id: boundary.LocalId) -> int? {
  callback?.(1, $id = id)
}

function virtual(speaker: Speaker, id: boundary.LocalId) -> int {
  speaker.speak($id = id)
}
"#,
    );
    let program = compile(&db);

    for name in ["user.indirect", "user.optional"] {
        let idx = program.function_indices[name];
        let bex_vm_types::Object::Function(function) = &(*program.objects)[idx] else {
            panic!("expected {name} to be a function")
        };
        assert!(
            function
                .bytecode
                .instructions
                .iter()
                .any(|instruction| matches!(instruction, Instruction::CallIndirectWithRuntimeId)),
            "{name} did not emit CALL_INDIRECT_WITH_RUNTIME_ID"
        );
    }

    let virtual_idx = program.function_indices["user.virtual"];
    let bex_vm_types::Object::Function(virtual_function) = &(*program.objects)[virtual_idx] else {
        panic!("expected user.virtual to be a function")
    };
    assert!(
        virtual_function
            .bytecode
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::VirtualCallWithRuntimeId { .. })),
        "virtual call did not emit VIRTUAL_CALL_WITH_RUNTIME_ID"
    );
}

macro_rules! emit_snapshot {
    ($name:expr, $output:expr) => {
        assert_compiler2_snapshot!(SNAPSHOT_PATH, $name, $output);
    };
}

#[test]
fn simple_function_compiles() {
    let mut db = make_db();
    db.file(
        "test.baml",
        "function greet(name: string) -> string { return name; }",
    );
    let program = compile(&db);
    assert!(
        program.function_indices.contains_key("user.greet"),
        "expected 'user.greet' in function_indices, got: {:?}",
        program.function_indices.keys().collect::<Vec<_>>()
    );
}

#[test]
fn builtin_functions_included() {
    let mut db = make_db();
    db.file("test.baml", "function f() -> string { return \"x\"; }");
    let program = compile(&db);
    // Builtins from the baml and env packages should be present
    let has_baml = program
        .function_global_indices
        .keys()
        .any(|k| k.starts_with("baml."));
    let has_baml_env = program
        .function_global_indices
        .keys()
        .any(|k| k.starts_with("baml.env."));
    assert!(
        has_baml,
        "expected at least one 'baml.*' function, got: {:?}",
        program.function_global_indices.keys().collect::<Vec<_>>()
    );
    assert!(
        has_baml_env,
        "expected at least one 'baml.env.*' function, got: {:?}",
        program.function_global_indices.keys().collect::<Vec<_>>()
    );
}

#[test]
fn enum_variant_lookup() {
    let mut db = make_db();
    db.file(
        "test.baml",
        r#"
        enum Color { Red Green Blue }
        function pick() -> Color { return Color.Red; }
        "#,
    );
    let program = compile(&db);
    assert!(
        program.function_indices.contains_key("user.pick"),
        "expected 'user.pick' in function_indices, got: {:?}",
        program.function_indices.keys().collect::<Vec<_>>()
    );
}

#[test]
fn class_field_lookup() {
    let mut db = make_db();
    db.file(
        "test.baml",
        r#"
        class Point { x int  y int }
        function origin() -> Point { return Point { x: 0, y: 0 }; }
        "#,
    );
    let program = compile(&db);
    assert!(
        program.function_indices.contains_key("user.origin"),
        "expected 'user.origin' in function_indices, got: {:?}",
        program.function_indices.keys().collect::<Vec<_>>()
    );
}

#[test]
fn optional_param_metadata_and_omitted_sentinel_emit() {
    let mut db = make_db();
    db.file("test.baml", OPTIONAL_DEFAULTS_SOURCE);
    let program = compile(&db);

    let add_idx = program.function_indices["user.add"];
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

    let main_idx = program.function_indices["user.main"];
    let bex_vm_types::Object::Function(main) = &(*program.objects)[main_idx] else {
        panic!("expected user.main to be a function");
    };
    assert!(
        main.bytecode
            .constants
            .iter()
            .any(|c| matches!(c, bex_vm_types::ConstValue::OmittedArg)),
        "expected omitted source argument to be emitted as OmittedArg"
    );
}

#[test]
fn optional_defaults_emit_snapshot() {
    let mut db = make_db();
    db.file("test.baml", OPTIONAL_DEFAULTS_SOURCE);
    let program = compile(&db);
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
/// - Produces a let binding with a global slot (appears in `let_global_indices`)
/// - Causes `$init` to appear in `program.function_indices`
/// - Causes `$init` to appear in `program.package_init_order`
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

    // The client let binding should have a global slot allocated in let_global_indices
    let has_let_slot = program
        .let_global_indices
        .keys()
        .any(|k| k.contains("MyClient"));
    assert!(
        has_let_slot,
        "expected 'MyClient' in let_global_indices, got: {:?}",
        program.let_global_indices.keys().collect::<Vec<_>>()
    );

    // $init should be synthesized
    let has_init = program.function_indices.contains_key("$init");
    assert!(
        has_init,
        "expected '$init' in function_indices, got: {:?}",
        program.function_indices.keys().collect::<Vec<_>>()
    );

    // $init should be in package_init_order
    assert!(
        program.package_init_order.contains(&"$init".to_string()),
        "expected '$init' in package_init_order, got: {:?}",
        program.package_init_order
    );
}

// ─── Phase 4.6 $init_test chainer tests ───────────────────────────────────────

/// Verify that when a file contains `test` blocks, a root `$init_test` chainer
/// is synthesized in `program.function_indices` with `arity: 1`.
#[test]
fn init_test_chainer_synthesized_when_tests_present() {
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

    // The root $init_test chainer should be present in function_indices
    assert!(
        program.function_indices.contains_key("$init_test"),
        "expected '$init_test' in function_indices after test block synthesis, got: {:?}",
        program.function_indices.keys().collect::<Vec<_>>()
    );

    // The chainer should also appear in function_global_indices
    assert!(
        program.function_global_indices.contains_key("$init_test"),
        "expected '$init_test' in function_global_indices, got: {:?}",
        program.function_global_indices.keys().collect::<Vec<_>>()
    );

    // Verify the chainer function has arity 1 (takes the registry parameter)
    let fn_obj_idx = program.function_indices["$init_test"];
    // program.objects derefs to Vec<Object> via Deref, so a plain usize index works.
    let fn_obj = &(*program.objects)[fn_obj_idx];
    let bex_vm_types::Object::Function(chainer) = fn_obj else {
        panic!("expected $init_test to be a Function object, got: {fn_obj:?}");
    };
    assert_eq!(
        chainer.arity, 1,
        "expected $init_test chainer to have arity 1, got: {}",
        chainer.arity
    );
    assert_eq!(chainer.param_names, vec!["registry"]);
    assert_eq!(chainer.param_types.len(), 1);
    assert_eq!(chainer.param_has_default, vec![false]);
}

/// Verify that when a file has NO test blocks, no `$init_test` chainer is synthesized.
#[test]
fn no_init_test_chainer_when_no_tests() {
    let mut db = make_db();
    db.file(
        "test.baml",
        "function greet(name: string) -> string { return name; }",
    );

    let program = compile(&db);

    assert!(
        !program.function_indices.contains_key("$init_test"),
        "expected no '$init_test' in function_indices when no test blocks present, got: {:?}",
        program.function_indices.keys().collect::<Vec<_>>()
    );
}

/// Verify that multiple client declarations:
/// - Both get global slots in `let_global_indices`
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

    // Both clients should have global slots in let_global_indices
    let has_a = program
        .let_global_indices
        .keys()
        .any(|k| k.contains("ClientA"));
    let has_b = program
        .let_global_indices
        .keys()
        .any(|k| k.contains("ClientB"));
    assert!(
        has_a,
        "expected 'ClientA' in let_global_indices, got: {:?}",
        program.let_global_indices.keys().collect::<Vec<_>>()
    );
    assert!(
        has_b,
        "expected 'ClientB' in let_global_indices, got: {:?}",
        program.let_global_indices.keys().collect::<Vec<_>>()
    );

    // $init should be synthesized
    assert!(
        program.function_indices.contains_key("$init"),
        "expected '$init' in function_indices"
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
            bex_vm_types::Object::Interface(i) if i.name.name().as_str() == "Shelf" => Some(i),
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
        matches!(iface.fields[2].ty, baml_type::RuntimeTy::Int { .. }),
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
            bex_vm_types::Object::Class(c) if c.name.item_name().as_str() == "Book" => Some(c),
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
        .values()
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
        .values()
        .flat_map(|pkg| pkg.impl_rules.values().flatten())
        .find(|rule| !rule.field_links.is_empty())
        .expect("expected a field-bearing impl rule");
    // `name` is `Person`'s second field, so an unlinked interface field must still
    // resolve to slot 1 rather than defaulting to 0.
    assert_eq!(rule.field_links.as_ref(), [1]);
}
