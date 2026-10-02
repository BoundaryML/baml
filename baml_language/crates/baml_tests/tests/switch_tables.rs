//! The dispatch tables of a linked image.
//!
//! A match that dispatches through a switch table behaves the same whichever
//! way the table was solved, so BAML source cannot tell a hashed table from a
//! sorted one, nor see the tags its keys were assigned. These tests read the
//! image instead; what the matches DO is pinned by the corpus
//! (`baml_src/ns_match_optimization`, `baml_src/ns_match_wide_switch`).

use std::fmt::Write as _;

use baml_tests::stdlib_prefix::compile_source;
use bex_vm_types::{
    Function, Object, Program,
    bytecode::{Instruction, SwitchDispatch, SwitchTable},
};

/// The function of `program` whose item name is `name`.
fn function_named<'p>(program: &'p Program, name: &str) -> &'p Function {
    let suffix = format!(".{name}");
    program
        .objects
        .iter()
        .find_map(|object| match object {
            Object::Function(function) if function.name.ends_with(&suffix) => {
                Some(function.as_ref())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("the program declares no function `{name}`"))
}

/// The one switch table `function` dispatches through.
fn only_table(function: &Function) -> &SwitchTable {
    let tables: Vec<usize> = function
        .bytecode
        .instructions
        .iter()
        .filter_map(|instruction| match instruction {
            Instruction::DenseTag(table) => Some(*table),
            _ => None,
        })
        .collect();
    let [table] = tables.as_slice() else {
        panic!(
            "`{}` dispatches through {} switch tables, expected one",
            function.name,
            tables.len()
        );
    };
    &function.bytecode.switch_tables[*table]
}

#[test]
fn a_sparse_match_dispatches_through_a_hashed_table() {
    let program = compile_source(
        "function pick(x: int) -> int {\n\
           match (x) { 1 => 101, 100 => 102, 5000 => 103, 70000 => 104, 900000 => 105, _ => 0 }\n\
         }\n",
    );
    let table = only_table(function_named(&program, "pick"));
    assert!(matches!(table.dispatch, SwitchDispatch::Hash { .. }));
    for (arm, key) in [1, 100, 5_000, 70_000, 900_000].into_iter().enumerate() {
        assert_eq!(table.arm_of(key), Some(u32::try_from(arm).unwrap()));
    }
    assert_eq!(table.arm_of(2), None);
}

/// A match has no widest form. Seventy class arms are more than a hash is
/// searched for, and they still take the switch road: one table, its keys the
/// tags the link assigned the classes, in order. (Bare type patterns: an arm
/// that binds tests and binds at once, and takes no switch.)
#[test]
fn a_match_wider_than_the_hash_search_dispatches_through_sorted_keys() {
    const ARMS: usize = 70;
    let mut source = String::new();
    for arm in 0..ARMS {
        writeln!(source, "class Wide{arm} {{ weight int }}").unwrap();
    }
    let union = (0..ARMS)
        .map(|arm| format!("Wide{arm}"))
        .collect::<Vec<_>>()
        .join(" | ");
    writeln!(
        source,
        "function pick(value: {union}) -> int {{\n  match (value) {{"
    )
    .unwrap();
    for arm in 0..ARMS {
        writeln!(source, "    Wide{arm} => {arm},").unwrap();
    }
    writeln!(source, "  }}\n}}").unwrap();

    let program = compile_source(&source);
    let pick = function_named(&program, "pick");
    assert!(
        !pick
            .bytecode
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::IsType(_))),
        "a wide match tests no arm in sequence"
    );
    let table = only_table(pick);
    let SwitchDispatch::Sorted(entries) = &table.dispatch else {
        panic!("seventy keys are sorted, got {:?}", table.dispatch);
    };
    assert_eq!(entries.len(), ARMS);
    assert!(table.dispatch.is_dispatchable());
    for arm in 0..ARMS {
        let declared = format!("Wide{arm}");
        let tag = program
            .objects
            .iter()
            .find_map(|object| match object {
                Object::Class(class) if class.name.item_name().as_str() == declared => {
                    Some(class.type_tag)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("the program declares no class `{declared}`"));
        assert_eq!(
            table.arm_of(tag.as_i64()),
            Some(u32::try_from(arm).unwrap()),
            "`{declared}` selects arm {arm}"
        );
    }
}

/// The link leaves no table for the loader to refuse: every table of the
/// image, the standard library's included, dispatches.
#[test]
fn every_table_of_a_linked_image_dispatches() {
    let program = compile_source(
        "function pick(x: int) -> int {\n\
           match (x) { 0 => 1, 30 => 2, 60 => 3, 99 => 4, _ => 0 }\n\
         }\n",
    );
    let tables: Vec<&SwitchTable> = program
        .objects
        .iter()
        .filter_map(|object| match object {
            Object::Function(function) => Some(&function.bytecode.switch_tables),
            _ => None,
        })
        .flatten()
        .collect();
    assert!(!tables.is_empty());
    for table in tables {
        assert!(
            table.dispatch.is_dispatchable(),
            "the table over [{}] cannot dispatch",
            table.key_names.join(", ")
        );
    }
}
