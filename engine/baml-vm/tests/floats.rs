//! VM tests for float/int `to_fixed` behavior.

use baml_vm::RuntimeError;

mod common;
use common::{
    assert_vm_executes, assert_vm_fails, ExecState, FailingProgram, Program, Value as TestValue,
};

use crate::common::Object;

#[test]
fn to_fixed_rounding_and_padding() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string[] {
                [
                    (3.14159).to_fixed(2),
                    (3.14159).to_fixed(5),
                    (3.14159).to_fixed(0),
                    (3.14159).to_fixed(),
                    (3.7).to_fixed(0),
                    (3.2).to_fixed(0),
                    (0.9).to_fixed(0),
                    (0.5).to_fixed(0),
                    (0.4).to_fixed(0),
                    (1.5).to_fixed(3),
                    (5.0).to_fixed(4),
                    (42.1).to_fixed(5),
                    (0.0).to_fixed(2),
                    (100.0).to_fixed(3),
                    (0.000123).to_fixed(6),
                    (1234.5678).to_fixed(2),
                    (3.14159265358979).to_fixed(10)
                ]
            }
        "#,
        function: "main",
        expected: ExecState::Complete(TestValue::Object(Object::Array(vec![
            TestValue::string("3.14"),
            TestValue::string("3.14159"),
            TestValue::string("3"),
            TestValue::string("3"),
            TestValue::string("4"),
            TestValue::string("3"),
            TestValue::string("1"),
            TestValue::string("1"),
            TestValue::string("0"),
            TestValue::string("1.500"),
            TestValue::string("5.0000"),
            TestValue::string("42.10000"),
            TestValue::string("0.00"),
            TestValue::string("100.000"),
            TestValue::string("0.000123"),
            TestValue::string("1234.57"),
            TestValue::string("3.1415926536"),
        ]))),
    })
}

#[test]
fn to_fixed_negative_and_ieee754_artifacts() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string[] {
                let minus_3_14 = 0.0 - 3.14;
                let minus_1_5 = 0.0 - 1.5;
                let minus_0_4 = 0.0 - 0.4;
                let minus_100_5 = 0.0 - 100.5;
                let minus_0_0 = 0.0 - 0.0;
                let minus_999_999 = 0.0 - 999.999;
                [
                    minus_3_14.to_fixed(1),
                    minus_1_5.to_fixed(0),
                    minus_0_4.to_fixed(0),
                    minus_100_5.to_fixed(0),
                    minus_0_0.to_fixed(2),
                    minus_999_999.to_fixed(2),
                    (1.005).to_fixed(2),
                    (1.255).to_fixed(2),
                    (1.355).to_fixed(2),
                    (1.045).to_fixed(2),
                    (1.105).to_fixed(2)
                ]
            }
        "#,
        function: "main",
        expected: ExecState::Complete(TestValue::Object(Object::Array(vec![
            TestValue::string("-3.1"),
            TestValue::string("-2"),
            TestValue::string("-0"),
            TestValue::string("-101"),
            TestValue::string("0.00"),
            TestValue::string("-1000.00"),
            TestValue::string("1.00"),
            TestValue::string("1.25"),
            TestValue::string("1.35"),
            TestValue::string("1.04"),
            TestValue::string("1.10"),
        ]))),
    })
}

#[test]
fn to_fixed_large_number_fallback() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string[] {
                let negative_large = 0.0 - 20000000000000000000000000.0;
                [
                    (1000000000000000000000.0).to_fixed(2),
                    (1500000000000000000000.0).to_fixed(0),
                    negative_large.to_fixed(3),
                    (999000000000000000000.0).to_fixed(2)
                ]
            }
        "#,
        function: "main",
        expected: ExecState::Complete(TestValue::Object(Object::Array(vec![
            TestValue::string("1e+21"),
            TestValue::string("1.5e+21"),
            TestValue::string("-2e+25"),
            TestValue::string("999000000000000000000.00"),
        ]))),
    })
}

#[test]
fn to_fixed_int_coercion_and_string_context() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string[] {
                [
                    (5).to_fixed(2),
                    (0).to_fixed(3),
                    (42).to_fixed(0),
                    "Price: $" + (9.9).to_fixed(2),
                    "Tax: " + (8.5).to_fixed(1) + "%",
                    "Rating: " + (4).to_fixed(1) + " / 5.0"
                ]
            }
        "#,
        function: "main",
        expected: ExecState::Complete(TestValue::Object(Object::Array(vec![
            TestValue::string("5.00"),
            TestValue::string("0.000"),
            TestValue::string("42"),
            TestValue::string("Price: $9.90"),
            TestValue::string("Tax: 8.5%"),
            TestValue::string("Rating: 4.0 / 5.0"),
        ]))),
    })
}

#[test]
fn to_fixed_digit_boundaries() -> anyhow::Result<()> {
    let expected_100 = baml_vm::native::number_to_fixed(3.14, 100).unwrap();
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                (3.14).to_fixed(100)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(TestValue::string(&expected_100)),
    })?;

    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                (3.14).to_fixed(0)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(TestValue::string("3")),
    })
}

#[test]
fn to_fixed_validates_digit_range() -> anyhow::Result<()> {
    assert_vm_fails(FailingProgram {
        source: r#"
            function main() -> string {
                (3.14).to_fixed(-1)
            }
        "#,
        function: "main",
        expected: RuntimeError::Other("to_fixed: digits must be in [0, 100], got -1".to_string())
            .into(),
    })?;

    assert_vm_fails(FailingProgram {
        source: r#"
            function main() -> string {
                (3.14).to_fixed(101)
            }
        "#,
        function: "main",
        expected: RuntimeError::Other("to_fixed: digits must be in [0, 100], got 101".to_string())
            .into(),
    })?;

    assert_vm_fails(FailingProgram {
        source: r#"
            function main() -> string {
                (3.14).to_fixed(200)
            }
        "#,
        function: "main",
        expected: RuntimeError::Other("to_fixed: digits must be in [0, 100], got 200".to_string())
            .into(),
    })
}
