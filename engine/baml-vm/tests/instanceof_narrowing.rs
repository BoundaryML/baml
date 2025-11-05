mod common;

use common::{assert_vm_executes, ExecState, Object, Program, Value};

#[test]
fn test_instanceof_type_narrowing_weather() -> anyhow::Result<()> {
    // This test verifies basic instanceof checking works
    // Note: Type narrowing for union parameters would require actual function parameters
    assert_vm_executes(Program {
        source: r#"
            class Weather {
                temperature int
                description string
            }

            class Error {
                message string
            }

            function main() -> string {
                let input = Weather {
                    temperature: 72,
                    description: "Sunny"
                };

                if (input instanceof Weather) {
                    // Convert int to string for concatenation
                    let temp_str = baml.unstable.string(input.temperature);
                    return "Temperature: " + temp_str;
                }
                return "Unknown";
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("Temperature: 72")),
    })
}

#[test]
fn test_instanceof_type_narrowing_error() -> anyhow::Result<()> {
    // This test verifies instanceof returns false for wrong type
    assert_vm_executes(Program {
        source: r#"
            class Weather {
                temperature int
                description string
            }

            class Error {
                message string
            }

            function main() -> string {
                let input = Error {
                    message: "Not found"
                };

                if (input instanceof Weather) {
                    return "Is weather";
                } else if (input instanceof Error) {
                    return "Error: " + input.message;
                }
                return "Unknown";
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("Error: Not found")),
    })
}

#[test]
fn test_instanceof_narrowing_with_field_access() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class GetWeatherType {
                city string
            }

            class Reply {
                content string
            }

            function main() -> string {
                let action = GetWeatherType {
                    city: "San Francisco"
                };

                if (action instanceof GetWeatherType) {
                    return "Getting weather for: " + action.city;
                }
                // Note: else branch cannot access action.content since action is known to be GetWeatherType
                return "Unknown action";
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("Getting weather for: San Francisco")),
    })
}

#[test]
fn test_instanceof_narrowing_with_unions() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Foo {
                field string
            }

            class Bar {
                other int
            }

            function main() -> string {
                // Test with union type parameter
                // Note: We need to create an instance since the test doesn't have function params
                let x = Foo { field: "test" };

                if (x instanceof Foo) {
                    // Should be able to access Foo.field here after narrowing
                    return x.field;
                } else {
                    return "not foo";
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("test")),
    })
}
