//! Unified tests for IO operations.
//!
//! These tests only assert bytecode shape, so they compile without invoking the
//! VM. Running `baml.io.input` against real stdin would block on a TTY (and only
//! returns EOF in CI by accident).

use baml_tests::engine::{OptLevel, compile_source_with_opt, display_user_functions};

fn bytecode_of(source: &str) -> String {
    let program = compile_source_with_opt(source, OptLevel::One);
    display_user_functions(&program)
}

#[test]
fn io_input_with_prompt_bytecode() {
    let bytecode = bytecode_of(
        r#"
            function main() -> string {
                baml.io.input("Enter your name: ")
            }
        "#,
    );

    insta::assert_snapshot!(bytecode, @r#"
    function main() -> string {
        load_const "Enter your name: "
        dispatch_future baml.io.input
        await
        return
    }
    "#);
}

#[test]
fn io_input_no_prompt_bytecode() {
    let bytecode = bytecode_of(
        r#"
            function main() -> string {
                baml.io.input(null)
            }
        "#,
    );

    insta::assert_snapshot!(bytecode, @r#"
    function main() -> string {
        load_const null
        dispatch_future baml.io.input
        await
        return
    }
    "#);
}

#[test]
fn io_input_fully_qualified_bytecode() {
    let bytecode = bytecode_of(
        r#"
            function main() -> string {
                baml.io.input("Enter text: ")
            }
        "#,
    );

    insta::assert_snapshot!(bytecode, @r#"
    function main() -> string {
        load_const "Enter text: "
        dispatch_future baml.io.input
        await
        return
    }
    "#);
}
