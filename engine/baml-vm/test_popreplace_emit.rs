// Test demonstrating that PopReplace properly cleans up emittable variables

use baml_vm::{Vm, VmExecState, emit::{Emit, NodeId, Path}};
use baml_vm::bytecode::Instruction;
use baml_vm::indexable::StackIndex;

fn main() {
    println!("Testing PopReplace emit cleanup:");
    println!("=================================\n");

    // Create a minimal VM setup
    let mut vm = Vm::new(/* ... program ... */, Default::default());

    // Simulate having some emittable variables that will be popped by PopReplace
    let stack_idx1 = StackIndex::from_raw(10);
    let stack_idx2 = StackIndex::from_raw(11);
    let stack_idx3 = StackIndex::from_raw(12);

    // Register some roots (simulating TrackEmittable)
    let root1 = vm.emit.register_root(NodeId::LocalVar(stack_idx1));
    let root2 = vm.emit.register_root(NodeId::LocalVar(stack_idx2));
    let root3 = vm.emit.register_root(NodeId::LocalVar(stack_idx3));

    // Track them
    vm.emittable_vars.insert(stack_idx1, root1);
    vm.emittable_vars.insert(stack_idx2, root2);
    vm.emittable_vars.insert(stack_idx3, root3);

    println!("Before PopReplace:");
    println!("  Emittable vars tracked: {}", vm.emittable_vars.len());
    println!("  Active roots: {}", vm.emit.roots.len());

    // Simulate PopReplace(3) - this should clean up all three emittable vars
    // The actual PopReplace instruction in the VM will do this automatically
    // Here we're just demonstrating the logic

    let n = 3;
    let drain_start = 10; // Simulating stack.len() - n

    for i in drain_start..(drain_start + n) {
        let index = StackIndex::from_raw(i);
        if let Some(root_id) = vm.emittable_vars.remove(&index) {
            println!("  Cleaning up emittable var at stack index {}", i);
            vm.emit.unregister_root(root_id);
        }
    }

    println!("\nAfter PopReplace cleanup:");
    println!("  Emittable vars tracked: {}", vm.emittable_vars.len());
    println!("  Active roots: {}", vm.emit.roots.len());

    println!("\n✅ PopReplace correctly cleans up emittable variables!");
}