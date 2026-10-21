//! Every standard-library declaration the runtime's own code names by
//! identity (`bex_vm::lang_items::runtime_items`) resolves against the
//! toolchain's own standard library. Load refuses an executable whose
//! language packages miss one; this pins that the shipped stdlib and the
//! runtime agree, so a declaration the stdlib renamed or moved fails here,
//! not in the first program that loads.

use std::sync::{Arc, atomic::AtomicBool};

use baml_base::LangPackage;
use baml_test_support::compile_source;
use bex_vm::BexVm;

#[test]
fn every_lang_item_the_runtime_names_is_declared() {
    let program = compile_source("function main() -> int { 1 }\n");
    let vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program");
    // Every language package is loaded, so no item below is skipped as
    // belonging to an absent package.
    let unloaded: Vec<&str> = LangPackage::ALL
        .into_iter()
        .filter(|package| vm.packages.lang_package(*package).is_none())
        .map(LangPackage::manifest_name)
        .collect();
    assert!(
        unloaded.is_empty(),
        "the standard library's packages are not all loaded: {unloaded:?}"
    );
    let missing: Vec<String> = bex_vm::lang_items::runtime_items()
        .filter(|item| vm.lang_declaration(*item).is_none())
        .map(|item| item.to_string())
        .collect();
    assert!(
        missing.is_empty(),
        "the runtime names declarations the standard library does not make: {missing:?}"
    );
}
