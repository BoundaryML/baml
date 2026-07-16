//! Runtime regressions for interface matching.

use std::sync::{Arc, atomic::AtomicBool};

use baml_project::testing::compile_source;
use bex_vm::{BexVm, VmExecState};

#[test]
fn concrete_out_of_body_impl_matches_interface() {
    let program = compile_source(
        r#"
        interface Provider {
            function name(self) -> string throws never
        }

        interface ToolCallingProvider requires Provider {
            function step(self) -> string throws never
        }

        class OpenAi {}

        implements Provider for OpenAi {
            function name(self) -> string throws never { "openai" }
        }

        implements ToolCallingProvider for OpenAi {
            function step(self) -> string throws never { "native-tools" }
        }

        function main() -> bool {
            let provider: Provider = OpenAi {};
            match (provider) {
                let native: ToolCallingProvider => native.step() == "native-tools",
                _ => false,
            }
        }
        "#,
    );
    let idx = program
        .function_index("user.main")
        .expect("user.main should compile");
    let mut vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false)))
        .expect("program should load");
    vm.set_entry_point(vm.heap.compile_time_ptr(idx), &[]);

    for _ in 0..64 {
        match vm.exec().expect("VM execution should succeed") {
            VmExecState::Complete(value) => {
                assert_eq!(value.as_bool(), Some(true));
                return;
            }
            VmExecState::EarlyYield => {}
            state => panic!("unexpected VM state: {state:?}"),
        }
    }
    panic!("VM did not complete");
}
