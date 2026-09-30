//! The executable format's laws are checked at the one road from a decoded
//! `Program` into a VM (`bex_vm::convert_program`), so a corrupt or forged
//! artifact — a cache entry, an embedded SDK program — is refused there
//! instead of panicking in the loader. A compiled program satisfies the laws;
//! each corruption below breaks one and names it.

use bex_vm_types::{
    GlobalIndex, Object, ObjectIndex, Program, TyTemplate, TypeHead,
    bytecode::{SwitchDispatch, SwitchKey, SwitchTable},
    errors::VmInternalError,
};

const SOURCE: &str = r#"
interface Greeter {
    function greet(self) -> string throws never;
    function shout(self) -> string throws never { self.greet() + "!" }
}

enum Level {
    Low,
    High,
}

class Widget {
    name string
    level Level

    function of(name: string) -> Widget {
        Widget { name: name, level: Level.Low }
    }

    implements Greeter {
        function greet(self) -> string throws never { self.name }
    }
}

function describe(w: Widget) -> string {
    match (w.level) {
        Level.Low => "low",
        Level.High => "high",
    }
}

function main() -> string {
    let w = Widget.of("w");
    w.shout() + describe(w)
}

test "main" {
    assert.equal(main(), "w!low")
}
"#;

fn program() -> Program {
    baml_tests::stdlib_prefix::compile_source(SOURCE)
}

fn refused(program: Program) -> String {
    match bex_vm::convert_program(program) {
        Err(VmInternalError::InvalidProgram(error)) => error.to_string(),
        Err(other) => panic!("refused for another reason: {other}"),
        Ok(_) => panic!("a program that breaks the laws converted"),
    }
}

/// The first function of the pool: a live object to misuse in the
/// corruptions below.
fn some_function(program: &Program) -> usize {
    program
        .objects
        .iter()
        .position(|object| matches!(object, Object::Function(_)))
        .expect("a compiled program pools functions")
}

#[test]
fn a_compiled_program_satisfies_the_laws() {
    let program = program();
    program
        .validate()
        .expect("a compiled program satisfies the executable's laws");
    bex_vm::convert_program(program).expect("and converts");
}

#[test]
fn a_root_outside_the_packages_is_refused() {
    let mut program = program();
    program.root = program.packages.len() as u32;
    assert!(refused(program).contains("the root is package"));
    // The empty program has no package for its root either.
    assert!(refused(Program::default()).contains("the root is package 0 of 0"));
}

#[test]
fn a_declaration_table_naming_another_kind_is_refused() {
    let mut program = program();
    let function = some_function(&program);
    let root = program.root as usize;
    let (_, index) = program.packages[root]
        .classes
        .iter_mut()
        .find(|(item, _)| item.name.as_str() == "Widget")
        .expect("the root declares Widget");
    *index = ObjectIndex::from_raw(function);
    assert!(refused(program).contains("class `Widget` names a"));
}

#[test]
fn a_cell_outside_the_pool_is_refused() {
    let mut program = program();
    let root = program.root as usize;
    let beyond = program.globals.len();
    program.packages[root].slot_base = GlobalIndex::from_raw(beyond);
    assert!(refused(program).contains("at cell"));
}

#[test]
fn a_head_naming_no_declaration_is_refused() {
    let mut program = program();
    let function = some_function(&program);
    let Some(Object::Function(pooled)) = program.objects.get_mut(function) else {
        unreachable!("the position was of a function")
    };
    // A static tag encoding the function's own index: an object, but not a
    // declaration, so the loader could never bind it.
    let tag = baml_type::typetag::TypeTag::of_static_index(function);
    pooled
        .param_types
        .push(TyTemplate::Class(TypeHead::unresolved(tag), Box::new([])));
    assert!(refused(program).contains("names no declaration"));
}

#[test]
fn an_unsolved_switch_is_refused() {
    let mut program = program();
    let function = some_function(&program);
    let Some(Object::Function(pooled)) = program.objects.get_mut(function) else {
        unreachable!("the position was of a function")
    };
    pooled.bytecode.switch_tables.push(SwitchTable {
        dispatch: SwitchDispatch::Keys(vec![SwitchKey::Kind(baml_type::typetag::INT)]),
        key_names: vec!["int".to_string()],
    });
    assert!(refused(program).contains("is not solved"));
}

#[test]
fn an_operand_outside_the_program_is_refused() {
    let mut program = program();
    let function = some_function(&program);
    let beyond = program.objects.len();
    let Some(Object::Function(pooled)) = program.objects.get_mut(function) else {
        unreachable!("the position was of a function")
    };
    pooled
        .bytecode
        .constants
        .push(bex_vm_types::ConstValue::Object(ObjectIndex::from_raw(
            beyond,
        )));
    assert!(refused(program).contains("outside the program"));
}

#[test]
fn an_init_order_that_is_not_the_init_packages_is_refused() {
    let mut program = program();
    // The root declares no `let`, so it has no `$init` to order.
    let root = program.root;
    assert!(program.packages[root as usize].init.is_none());
    program.init_order.push(root);
    assert!(refused(program).contains("not exactly the packages with an `$init`"));
}
