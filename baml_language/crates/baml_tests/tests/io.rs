//! Unified tests for IO operations.

use baml_tests::baml_test;

#[tokio::test]
async fn io_input_with_prompt_bytecode() {
    let output = baml_test!(
        r#"
            function main() -> string {
                io.input("Enter your name: ")
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "Enter your name: "
        dispatch_future baml.io.input
        await
        return
    }
    "#);
    // Note: runtime result is not asserted because stdin is not connected in test
}

#[tokio::test]
async fn io_input_no_prompt_bytecode() {
    let output = baml_test!(
        r#"
            function main() -> string {
                io.input(null)
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const null
        dispatch_future baml.io.input
        await
        return
    }
    "#);
}

#[tokio::test]
async fn io_input_fully_qualified_bytecode() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.io.input("Enter text: ")
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "Enter text: "
        dispatch_future baml.io.input
        await
        return
    }
    "#);
}
