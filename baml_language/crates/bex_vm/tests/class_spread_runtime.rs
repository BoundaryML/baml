//! Runtime regressions for exact-class spread lowering.

use std::sync::{Arc, atomic::AtomicBool};

use baml_project::testing::compile_source;
use bex_vm::{BexVm, VmExecState};

fn run_bool(src: &str, fn_name: &str) -> bool {
    let program = compile_source(src);
    let idx = program
        .function_index(fn_name)
        .unwrap_or_else(|| panic!("function {fn_name:?} not found"));
    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("construct test VM");
    let fptr = vm.heap.compile_time_ptr(idx);
    vm.set_entry_point(fptr, &[]);

    for _ in 0..256 {
        match vm.exec().expect("execute test program") {
            VmExecState::Complete(value) => {
                return value.as_bool().expect("entry point returns bool");
            }
            VmExecState::EarlyYield => {}
            state => panic!("unexpected VM state: {state:?}"),
        }
    }
    panic!("VM did not complete within 256 exec calls");
}

#[test]
fn generic_spread_preserves_callable_and_last_field() {
    const SRC: &str = r#"
        class Task<T> {
            value: T,
            transform: (T) -> T throws never,
            events: string[],
        }

        function defaults<T>(value: T, transform: (T) -> T throws never) -> Task<T> {
            Task<T> { value: value, transform: transform, events: ["created"] }
        }

        function main() -> bool {
            let task = Task {
                ...defaults<int>(1, (value: int) -> int { value + 1 }),
                value: 41,
            };
            task.transform(task.value) == 42 && task.events == ["created"]
        }
    "#;

    assert!(run_bool(SRC, "user.main"));
}

#[test]
fn spread_factory_call_with_omitted_defaults_preserves_caller_frame() {
    const SRC: &str = r#"
        class Tool {
            name: string,
        }

        class Options {
            budget: int,
            tools: Tool[],
            registry: Tool?,
            hook: Tool?,
            observers: Tool[],
            dispatch: (int[]) -> int[] throws never,
        }

        function tool() -> Tool {
            Tool { name: "knowledge" }
        }

        function dispatch(values: int[]) -> int[] throws never {
            values
        }

        function defaults(
            tools: Tool[],
            callback: (int[]) -> int[] throws never,
            registry: Tool? = null,
            hook: Tool? = null,
            observers: Tool[] = [],
        ) -> Options {
            Options {
                budget: 5,
                tools: tools,
                registry: registry,
                hook: hook,
                observers: observers,
                dispatch: callback,
            }
        }

        function main() -> bool {
            let options = Options {
                ...defaults([tool()], dispatch),
                budget: 1,
            };
            options.budget == 1 &&
                options.dispatch([1]).length() == 1 &&
                options.registry == null &&
                options.hook == null
        }
    "#;

    assert!(run_bool(SRC, "user.main"));
}
