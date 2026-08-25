//! Unified test infrastructure for bytecode snapshots + BexExternalValue execution.
//!
//! Combines bytecode compilation display (via `display_program`) with VM execution
//! through `BexEngine` (which handles `BexExternalValue` ↔ VM value conversions).
//!
//! # Usage
//!
//! ```ignore
//! use baml_tests::baml_test;
//! use bex_engine::BexExternalValue;
//!
//! #[tokio::test]
//! async fn my_test() {
//!     let output = baml_test!("
//!         function main() -> int { 42 }
//!     ");
//!
//!     insta::assert_snapshot!(output.bytecode, @"...");
//!     assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
//! }
//! ```

use std::sync::Arc;

use bex_engine::{BexCallArg, BexEngine, BexExternalValue, FunctionCallContextBuilder};
use bex_vm::debug::{BytecodeFormat, display_program};
use bex_vm_types::{Function, Object, Program};
pub use indexmap::IndexMap;
#[cfg(test)]
use insta::{assert_snapshot, with_settings};
use sys_native::SysOpsExt;

// The stdlib slice these helpers splice in is compiled once at build time
// rather than once per test; see `crate::stdlib_prefix`. Output is byte-identical
// to `baml_project::testing`'s honest helpers, pinned by the
// `stdlib_prefix_equivalence` oracle.
pub use crate::stdlib_prefix::{
    OptLevel, compile_multi_file, compile_source, compile_source_with_opt,
};

#[cfg(test)]
const SNAPSHOT_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/snapshots/engine");

#[cfg(test)]
macro_rules! engine_snapshot {
    ($name:expr, $output:expr) => {
        with_settings!({ snapshot_path => SNAPSHOT_PATH, omit_expression => true }, {
            assert_snapshot!($name, $output);
        });
    };
}

/// Output of a unified test: bytecode display + execution result.
pub struct TestOutput {
    /// Textual bytecode display of all user-defined functions (for insta snapshots).
    pub bytecode: String,
    /// VM execution result (may be an error for error-testing scenarios).
    pub result: Result<BexExternalValue, bex_engine::EngineError>,
}

/// Extract user-defined functions from a program and display them in textual format.
///
/// Strips the `"user."` package prefix from function names so snapshots show
/// `function main()` rather than `function user.main()`.
///
/// Auto-derived methods (e.g. synthesized `to_json` / `from_json` on every user
/// class) are filtered by default to keep snapshots focused on user-written
/// source. Pass `show_auto_derive: true` via the `baml_test!` macro to include
/// them when debugging the synthesizer itself.
pub fn display_user_functions(program: &Program) -> String {
    display_user_functions_with_options(program, false)
}

/// Like [`display_user_functions`], but lets the caller include auto-derived
/// methods in the bytecode output.
pub fn display_user_functions_with_options(program: &Program, show_auto_derive: bool) -> String {
    let mut functions: Vec<(String, &Function)> = program
        .function_indices
        .iter()
        .filter_map(|(name, idx)| match program.objects.get(*idx) {
            Some(Object::Function(f)) => {
                if !f.origin.is_user_callable() {
                    return None;
                }
                if !show_auto_derive && f.origin.is_auto_derived() {
                    return None;
                }
                // Strip leading "user." package prefix for display.
                let display_name = name
                    .strip_prefix("user.")
                    .unwrap_or(name.as_str())
                    .to_owned();
                Some((display_name, &**f))
            }
            _ => None,
        })
        .collect();
    functions.sort_by(|(a, _), (b, _)| a.cmp(b));
    display_program(&functions, BytecodeFormat::Textual)
}

/// Build the pool into an (unsealed) heap and bind every head, so rendered
/// metadata reflects what the runtime shows: the loader always binds before
/// anything can display a type, and an unbound head renders as its tag
/// (`<unresolved type #…>`).
///
/// Float constants are boxed into the pool first, exactly as the engine's
/// load path does — `resolve_function_constants` (inside the heap build)
/// refuses a raw `ConstValue::Float`.
pub fn bound_pool(program: &Program) -> bex_heap::BexHeap {
    let mut objects = program.objects.0.clone();
    for index in 0..objects.len() {
        let Object::Function(function) = &objects[index] else {
            continue;
        };
        let floats: Vec<(usize, f64)> = function
            .bytecode
            .constants
            .iter()
            .enumerate()
            .filter_map(|(slot, constant)| match constant {
                bex_vm_types::ConstValue::Float(value) => Some((slot, *value)),
                _ => None,
            })
            .collect();
        for (slot, value) in floats {
            let boxed = objects.len();
            objects.push(Object::Float(value));
            let Object::Function(function) = &mut objects[index] else {
                unreachable!("the object at `index` was a function above");
            };
            function.bytecode.constants[slot] =
                bex_vm_types::ConstValue::Object(bex_vm_types::ObjectIndex::from_raw(boxed));
        }
    }
    let mut heap = bex_heap::BexHeap::build_unsealed_default(objects);
    heap.bind_type_heads();
    heap
}

/// Read the function at pool index `idx` out of a heap built by
/// [`bound_pool`], or `None` if the slot holds something else.
pub fn bound_function(heap: &bex_heap::BexHeap, idx: usize) -> Option<&Function> {
    let ptr = heap.compile_time_ptr(idx);
    // SAFETY: `ptr` indexes the pool the heap was built from, and the returned
    // borrow is tied to `heap`, which owns that pool for the borrow's lifetime.
    match unsafe { ptr.get() } {
        Object::Function(f) => Some(&**f),
        _ => None,
    }
}

/// [`display_user_functions`] over a [`bound_pool`], so type positions in the
/// rendered bytecode show declaration names instead of raw head tags.
pub fn display_user_functions_bound(program: &Program) -> String {
    let heap = bound_pool(program);
    let mut functions: Vec<(String, &Function)> = program
        .function_indices
        .iter()
        .filter_map(|(name, idx)| {
            let f = bound_function(&heap, *idx)?;
            if !f.origin.is_user_callable() {
                return None;
            }
            let display_name = name.strip_prefix("user.").unwrap_or(name).to_owned();
            Some((display_name, f))
        })
        .collect();
    functions.sort_by(|(a, _), (b, _)| a.cmp(b));
    display_program(&functions, BytecodeFormat::Textual)
}

/// Resolve a user-provided entry name to the fully-qualified name used in the program.
///
/// Compiler2 qualifies function names with their package (e.g. `"user.main"`).
/// Test code passes bare names (`"main"`), so we try both the bare name and the
/// `"user.<name>"` qualified form, returning whichever is present.
fn resolve_entry_name(program: &Program, entry: &str) -> String {
    // Try exact match first.
    if program.function_index(entry).is_some() {
        return entry.to_owned();
    }
    // Try with "user." prefix (compiler2 qualifies user functions).
    let qualified = format!("user.{entry}");
    if program.function_indices.contains_key(qualified.as_str()) {
        return qualified;
    }
    panic!("function '{entry}' not found in program (tried '{entry}' and 'user.{entry}')")
}

/// Resolve named arguments to positional order using function parameter names.
fn resolve_args(
    program: &Program,
    entry: &str,
    args: IndexMap<&str, BexExternalValue>,
) -> Vec<BexCallArg> {
    let resolved_entry = resolve_entry_name(program, entry);
    let function_idx = program
        .function_index(&resolved_entry)
        .unwrap_or_else(|| panic!("function '{entry}' not found in program"));

    let function = match program.objects.get(function_idx) {
        Some(Object::Function(f)) => f,
        other => panic!(
            "expected Function object for '{entry}', got {:?}",
            other.map(std::mem::discriminant)
        ),
    };

    for provided in args.keys() {
        if !function.param_names.iter().any(|p| p == provided) {
            panic!("unexpected argument '{provided}' for function '{entry}'");
        }
    }

    let required_count = function
        .param_names
        .iter()
        .enumerate()
        .filter(|(idx, _)| {
            !function
                .param_has_default
                .get(*idx)
                .copied()
                .unwrap_or(false)
        })
        .count();

    if args.len() < required_count || args.len() > function.param_names.len() {
        panic!(
            "argument count mismatch for function '{entry}': expected between {} and {}, got {}",
            required_count,
            function.param_names.len(),
            args.len()
        );
    }

    function
        .param_names
        .iter()
        .enumerate()
        .map(|(idx, param_name)| {
            if let Some(value) = args.get(param_name.as_str()).cloned() {
                BexCallArg::Provided(Box::new(value))
            } else if function
                .param_has_default
                .get(idx)
                .copied()
                .unwrap_or(false)
            {
                BexCallArg::OmittedDefault
            } else {
                panic!("missing argument '{param_name}' for function '{entry}'")
            }
        })
        .collect()
}

/// Compile BAML source, display bytecode, and execute the entry function.
///
/// This is the core function behind the `baml_test!` macro. It:
/// 1. Compiles the source to bytecode
/// 2. Displays all user-defined functions in textual format (for insta snapshots)
/// 3. Resolves named arguments to positional order
/// 4. Executes the entry function via `BexEngine` and returns the result as `Result<BexExternalValue, EngineError>`
pub async fn run_test(
    source: &str,
    entry: &str,
    args: IndexMap<&str, BexExternalValue>,
    opt: OptLevel,
) -> TestOutput {
    run_test_with_options(source, entry, args, opt, false).await
}

/// Like [`run_test`] but lets the caller include auto-derived class methods
/// (`to_json` / `from_json`) in the bytecode display.
pub async fn run_test_with_options(
    source: &str,
    entry: &str,
    args: IndexMap<&str, BexExternalValue>,
    opt: OptLevel,
    show_auto_derive: bool,
) -> TestOutput {
    let program = compile_source_with_opt(source, opt);
    run_compiled(program, entry, args, show_auto_derive).await
}

/// Run an already-compiled `program`, returning its bytecode display and the
/// engine result. Split out of [`run_test_with_options`] so timing-sensitive
/// tests can compile FIRST (compilation grows with the stdlib) and time only
/// the engine execution — see `cancel_cascade.rs`.
pub async fn run_compiled(
    program: Program,
    entry: &str,
    args: IndexMap<&str, BexExternalValue>,
    show_auto_derive: bool,
) -> TestOutput {
    // Display bytecode before the engine consumes the program.
    let bytecode = display_user_functions_with_options(&program, show_auto_derive);

    // Resolve the entry name (bare "main" → "user.main" for compiler2 output).
    let resolved_entry = resolve_entry_name(&program, entry);

    // Resolve named args to positional before the engine consumes the program.
    let positional_args = resolve_args(&program, entry, args);

    // Create engine and execute.
    let engine = BexEngine::new_with_runtime_compiler(
        program,
        Arc::new(sys_ops::SysOps::native()),
        Vec::new(),
        bex_project::runtime_compiler(),
    )
    .expect("Failed to create BexEngine");
    let engine = Arc::new(engine);

    let result = engine
        .call_function_bound_args(
            &resolved_entry,
            positional_args,
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    TestOutput { bytecode, result }
}

/// Like `run_test` but at `OptLevel::Two` (includes MIR constant folding).
pub async fn run_test_mir_optimized(
    source: &str,
    entry: &str,
    args: IndexMap<&str, BexExternalValue>,
) -> TestOutput {
    run_test(source, entry, args, OptLevel::Two).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn source_call_omitted_default_executes_in_callee_scope() {
        let output = run_test(
            r#"
            function add(base: int, amount: int = base + 2) -> int {
              base + amount
            }

            function main() -> int {
              add(5)
            }
            "#,
            "main",
            IndexMap::new(),
            OptLevel::One,
        )
        .await;

        assert_eq!(output.result, Ok(BexExternalValue::Int(12)));
        engine_snapshot!(
            "source_call_omitted_default_executes_in_callee_scope_bytecode",
            output.bytecode
        );
    }

    #[tokio::test]
    async fn host_call_omission_is_distinct_from_explicit_null() {
        let source = r#"
        function is_null(value: int? = 7) -> bool {
          value == null
        }
        "#;

        let omitted = run_test(source, "is_null", IndexMap::new(), OptLevel::One).await;
        assert_eq!(omitted.result, Ok(BexExternalValue::Bool(false)));

        let mut args = IndexMap::new();
        args.insert("value", BexExternalValue::Null);
        let explicit_null = run_test(source, "is_null", args, OptLevel::One).await;
        assert_eq!(explicit_null.result, Ok(BexExternalValue::Bool(true)));
    }

    #[tokio::test]
    async fn bound_args_reject_argument_count_mismatch() {
        let program = compile_source_with_opt(
            "function main(x: int, y: int = 1) -> int { x + y }",
            OptLevel::One,
        );
        let engine = Arc::new(
            BexEngine::new(program, Arc::new(sys_ops::SysOps::native()), Vec::new())
                .expect("engine"),
        );

        let err = engine
            .call_function_bound_args(
                "main",
                vec![BexCallArg::Provided(Box::new(BexExternalValue::Int(1)))],
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                true,
            )
            .await
            .expect_err("argument count mismatch should be rejected");

        assert!(
            matches!(&err, bex_engine::EngineError::TypeMismatch { message } if message.contains("expects 2 argument(s), got 1")),
            "{err}"
        );
    }

    #[tokio::test]
    async fn bound_args_reject_omitted_default_for_required_param() {
        let program = compile_source_with_opt("function main(x: int) -> int { x }", OptLevel::One);
        let engine = Arc::new(
            BexEngine::new(program, Arc::new(sys_ops::SysOps::native()), Vec::new())
                .expect("engine"),
        );

        let err = engine
            .call_function_bound_args(
                "main",
                vec![BexCallArg::OmittedDefault],
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                true,
            )
            .await
            .expect_err("omitted required argument should be rejected");

        assert!(
            matches!(&err, bex_engine::EngineError::TypeMismatch { message } if message.contains("cannot be omitted")),
            "{err}"
        );
    }

    #[tokio::test]
    async fn optional_dropping_adapter_preserves_source_defaults() {
        let output = run_test(
            r#"
            function combine(x: int, a: int = 10, b: int = 100) -> int {
              x + a + b
            }

            function main() -> int {
              let f: (x: int, b?: int) -> int throws never = combine;
              f(1, b = 5)
            }
            "#,
            "main",
            IndexMap::new(),
            OptLevel::One,
        )
        .await;

        assert_eq!(output.result, Ok(BexExternalValue::Int(16)));
        engine_snapshot!(
            "optional_dropping_adapter_preserves_source_defaults_bytecode",
            output.bytecode
        );
    }

    #[tokio::test]
    async fn optional_adapter_reorders_named_optional_params() {
        let output = run_test(
            r#"
            function combine(x: int, a: int = 10, b: int = 100) -> int {
              (x * 100) + (a * 10) + b
            }

            function main() -> int {
              let f: (x: int, b?: int, a?: int) -> int throws never = combine;
              f(1, b = 5, a = 2)
            }
            "#,
            "main",
            IndexMap::new(),
            OptLevel::One,
        )
        .await;

        assert_eq!(output.result, Ok(BexExternalValue::Int(125)));
    }

    #[tokio::test]
    async fn optional_adapter_applies_to_concrete_call_argument() {
        let output = run_test(
            r#"
            function combine(x: int, a: int = 10, b: int = 100) -> int {
              x + a + b
            }

            function apply(f: (x: int, b?: int) -> int) -> int {
              f(1, b = 5)
            }

            function main() -> int {
              apply(combine)
            }
            "#,
            "main",
            IndexMap::new(),
            OptLevel::One,
        )
        .await;

        assert_eq!(output.result, Ok(BexExternalValue::Int(16)));
    }

    #[tokio::test]
    async fn optional_adapter_applies_to_generic_call_argument() {
        let output = run_test(
            r#"
            function combine(x: int, a: int = 10, b: int = 100) -> int {
              x + a + b
            }

            function apply<T>(f: (x: T, b?: int) -> T, value: T) -> T {
              f(value, b = 5)
            }

            function main() -> int {
              apply(combine, 1)
            }
            "#,
            "main",
            IndexMap::new(),
            OptLevel::One,
        )
        .await;

        assert_eq!(output.result, Ok(BexExternalValue::Int(16)));
    }
}
