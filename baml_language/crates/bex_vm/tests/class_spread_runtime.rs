//! Runtime regressions for exact-class spread.
//!
//! BEPv2 constructs providers, hooks, tasks, and options atomically by
//! spreading a value of the exact destination class and overriding selected
//! fields. These tests keep the VM stack behavior covered for the shapes used
//! by that reference code.

use std::sync::{Arc, atomic::AtomicBool};

use baml_project::testing::{compile_multi_file, compile_source};
use bex_vm::{BexVm, VmExecState};
use bex_vm_types::Program;

const MAX_EXEC_CALLS: usize = 256;

fn run_bool(src: &str, fn_name: &str) -> bool {
    run_bool_program(compile_source(src), fn_name)
}

fn run_bool_program(program: Program, fn_name: &str) -> bool {
    let idx = program
        .function_index(fn_name)
        .unwrap_or_else(|| panic!("function {fn_name:?} not found"));
    let function_debug = match &(*program.objects)[idx] {
        bex_vm_types::Object::Function(function) => bex_vm::debug::display_program(
            &[(fn_name.to_string(), function)],
            bex_vm::debug::BytecodeFormat::Expanded,
        ),
        _ => String::new(),
    };
    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program");
    let fptr = vm.heap.compile_time_ptr(idx);
    vm.set_entry_point(fptr, &[]);
    for _ in 0..MAX_EXEC_CALLS {
        match vm
            .exec()
            .unwrap_or_else(|error| panic!("exec: {error:?}\n\n{function_debug}"))
        {
            VmExecState::Complete(v) => return v.as_bool().expect("entry returns bool"),
            VmExecState::EarlyYield => {}
            other => panic!("unexpected VM state: {other:?}"),
        }
    }
    panic!("vm did not complete within {MAX_EXEC_CALLS} exec() calls");
}

#[test]
fn exact_class_spread_from_function_result() {
    const SRC: &str = r#"
        class ProviderConfig {
            provider_name: string,
            failures_remaining: int,
            calls: int,
        }

        function defaults() -> ProviderConfig {
            ProviderConfig {
                provider_name: "fake",
                failures_remaining: 0,
                calls: 0,
            }
        }

        function main() -> bool {
            let config = ProviderConfig {
                ...defaults(),
                provider_name: "flaky",
                failures_remaining: 1,
            };
            config.provider_name == "flaky" &&
                config.failures_remaining == 1 &&
                config.calls == 0
        }
    "#;

    assert!(run_bool(SRC, "user.main"));
}

#[test]
fn exact_class_spread_preserves_callable_field() {
    const SRC: &str = r#"
        class Options {
            budget: int,
            dispatch: (int) -> int throws never,
            observers: string[],
        }

        function defaults(dispatch: (int) -> int throws never) -> Options {
            Options { budget: 5, dispatch: dispatch, observers: [] }
        }

        function main() -> bool {
            let options = Options {
                ...defaults((value: int) -> int { value + 1 }),
                budget: 1,
                observers: ["events"],
            };
            options.budget == 1 &&
                options.dispatch(41) == 42 &&
                options.observers == ["events"]
        }
    "#;

    assert!(run_bool(SRC, "user.main"));
}

#[test]
fn exact_generic_class_spread_preserves_callable_field() {
    const SRC: &str = r#"
        class Task<T> {
            value: T,
            transform: (T) -> T throws never,
            tags: map<string, string>,
        }

        function defaults<T>(value: T, transform: (T) -> T throws never) -> Task<T> {
            Task<T> { value: value, transform: transform, tags: {} }
        }

        function main() -> bool {
            let task = Task<int> {
                ...defaults<int>(1, (value: int) -> int { value + 1 }),
                tags: { "tier": "pro" },
            };
            task.transform(task.value) == 2 && task.tags.get("tier") == "pro"
        }
    "#;

    assert!(run_bool(SRC, "user.main"));
}

#[test]
fn exact_class_spread_preserves_last_field_of_interface_implementor() {
    const SRC: &str = r#"
        interface Hooks {
            function record(self, value: string) -> null throws never
        }

        class GuideHooks {
            tool: string?,
            block_name: string?,
            replace_name: string?,
            provider: string?,
            after_calls: string[],
            hook_events: string[],

            implements Hooks {
                function record(self, value: string) -> null throws never {
                    self.hook_events.push(value);
                    null
                }
            }
        }

        function defaults() -> GuideHooks throws never {
            GuideHooks {
                tool: null,
                block_name: null,
                replace_name: null,
                provider: null,
                after_calls: [],
                hook_events: [],
            }
        }

        function main() -> bool {
            let hooks = GuideHooks { ...defaults(), tool: "account" };
            hooks.record("model_started");
            hooks.hook_events == ["model_started"]
        }
    "#;

    assert!(run_bool(SRC, "user.main"));
}

#[test]
fn namespaced_exact_class_spread_preserves_last_field() {
    const API: &str = r#"
        class Tool {
            name: string,
        }

        interface Provider {
            function name(self) -> string throws never
        }

        interface AgentHooks {
            function record(self, value: string) -> null throws never
        }
    "#;

    const SRC: &str = r#"
        class GuideHooks {
            tool: root.ai.Tool?,
            block_name: string?,
            replace_name: string?,
            provider: root.ai.Provider?,
            after_calls: string[],
            hook_events: string[],

            implements root.ai.AgentHooks {
                function record(self, value: string) -> null throws never {
                    self.hook_events.push(value);
                    null
                }
            }
        }

        function defaults() -> GuideHooks throws never {
            GuideHooks {
                tool: null,
                block_name: null,
                replace_name: null,
                provider: null,
                after_calls: [],
                hook_events: [],
            }
        }

        function account_tool() -> root.ai.Tool throws never {
            root.ai.Tool { name: "account" }
        }

        function main() -> bool {
            (GuideHooks {
                ...defaults(),
                tool: account_tool(),
            }).hook_events == []
        }
    "#;

    let program = compile_multi_file(&[
        ("ns_ai/core.baml", API),
        ("ns_ai_scenarios/guide.baml", SRC),
    ]);
    assert!(run_bool_program(program, "user.ai_scenarios.main"));
}

#[test]
fn named_default_parameters_construct_options_atomically() {
    const SRC: &str = r#"
        class Options {
            registry: string?,
            observers: string[],
        }

        function options(
            registry: string? = null,
            observers: string[] = [],
        ) -> Options {
            Options { registry: registry, observers: observers }
        }

        function main() -> bool {
            let configured = options(
                registry = "tools",
                observers = ["events"],
            );
            configured.registry == "tools" && configured.observers == ["events"]
        }
    "#;

    assert!(run_bool(SRC, "user.main"));
}

#[test]
fn class_spread_factory_call_with_omitted_defaults_preserves_caller_frame() {
    const SRC: &str = r#"
        class Tool {
            name: string,
        }

        class Budget {
            max_steps: int?,
        }

        class Options {
            budget: Budget?,
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
                budget: Budget { max_steps: 5 },
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
                budget: Budget { max_steps: 1 },
            };
            options.dispatch([1]).length() == 1
        }
    "#;

    assert!(run_bool(SRC, "user.main"));
}
