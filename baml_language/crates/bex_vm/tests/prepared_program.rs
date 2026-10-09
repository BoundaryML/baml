//! The shared loader must return boxed constants/globals and executable code.
//! Rust is needed to inspect object identity and inject float globals (including
//! distinct NaN payloads) without relying on compiler constant folding.

use std::sync::{Arc, atomic::AtomicBool};

use baml_test_support::compile_source;
use bex_vm::{BexVm, VmExecState, convert_program};
use bex_vm_types::{ConstValue, GlobalIndex, Instruction, Object};

#[test]
fn prepared_constants_and_globals_share_boxes_without_losing_float_bits() {
    let mut program = compile_source("function main() -> float { 0.5 }");
    let main_index = program.rendered_callables()["user.main"].object;
    let bits = [
        0.5_f64.to_bits(),
        0.0_f64.to_bits(),
        (-0.0_f64).to_bits(),
        f64::INFINITY.to_bits(),
        f64::NEG_INFINITY.to_bits(),
        0x7ff8_0000_0000_0001,
        0xfff8_0000_0000_0002,
        0.5_f64.to_bits(),
    ];
    let global_start = program.globals.len();
    for bits in bits {
        program.add_global(ConstValue::Float(f64::from_bits(bits)));
    }
    let Object::Function(main) = &mut program.objects[main_index] else {
        unreachable!()
    };
    let constant_start = main.bytecode.constants.len();
    main.bytecode.constants.extend(
        bits.into_iter()
            .map(|bits| ConstValue::Float(f64::from_bits(bits))),
    );
    let original = main.bytecode.lower_to_compact();
    // A stale stream must never bypass final preparation.
    main.bytecode.compact = Some(original.clone());

    let program = convert_program(program).unwrap();
    let Object::Function(main) = &program.objects[main_index] else {
        unreachable!()
    };
    let compact = main.bytecode.compact.as_ref().unwrap();
    assert_eq!(compact.code, original.code);
    assert_eq!(compact.line_table, original.line_table);
    assert_eq!(compact.exception_table, original.exception_table);
    assert_eq!(
        compact.handler_context_table,
        original.handler_context_table
    );
    for (offset, bits) in bits.into_iter().enumerate() {
        let constant = &main.bytecode.constants[constant_start + offset];
        assert_eq!(constant, &program.globals[global_start + offset]);
        let ConstValue::Object(index) = constant else {
            panic!("float was not boxed: {constant:?}");
        };
        let Object::Float(float) = &program.objects[*index] else {
            panic!("float points at another kind of object");
        };
        assert_eq!(float.to_bits(), bits);
    }
    assert_eq!(
        main.bytecode.constants[constant_start],
        main.bytecode.constants[constant_start + bits.len() - 1],
        "repeated bit patterns share an object"
    );
    assert_ne!(
        main.bytecode.constants[constant_start + 1],
        main.bytecode.constants[constant_start + 2],
        "positive and negative zero keep distinct boxes"
    );
    for object in &program.objects {
        if let Object::Function(function) = object {
            assert!(
                function
                    .bytecode
                    .constants
                    .iter()
                    .all(|value| !matches!(value, ConstValue::Float(_)))
            );
            assert!(function.bytecode.compact.is_some());
        }
    }
    assert!(
        program
            .globals
            .iter()
            .all(|value| !matches!(value, ConstValue::Float(_)))
    );
}

#[test]
fn vm_executes_a_prepared_float_global() {
    let mut program = compile_source("function main() -> float { 0.5 }");
    let main_index = program.rendered_callables()["user.main"].object;
    let global = GlobalIndex::from_raw(program.globals.len());
    let expected = (-0.0_f64).to_bits();
    program.add_global(ConstValue::Float(f64::from_bits(expected)));
    let Object::Function(main) = &mut program.objects[main_index] else {
        unreachable!()
    };
    let load = main
        .bytecode
        .instructions
        .iter_mut()
        .find(|instruction| matches!(instruction, Instruction::LoadConst(_)))
        .unwrap();
    *load = Instruction::LoadGlobal(global);

    let mut vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
    vm.set_entry_point(vm.heap.compile_time_ptr(main_index.raw()), &[]);
    loop {
        match vm.exec().unwrap() {
            VmExecState::Complete(value) => {
                let pointer = value.as_object_ptr().unwrap();
                let Object::Float(float) = vm.get_object(pointer) else {
                    panic!("expected a boxed float");
                };
                assert_eq!(float.to_bits(), expected);
                break;
            }
            VmExecState::EarlyYield => {}
            other => panic!("unexpected execution state: {other:?}"),
        }
    }
}
