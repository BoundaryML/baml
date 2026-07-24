//! Regression coverage for iterating through an interface method whose return
//! type is a transparent alias for an array.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn interface_method_array_alias_is_iterable() {
    let output = baml_test!(
        r#"
        class Event {
            value: int
        }

        type EventStream = Event[]

        interface Live {
            function events(self) -> EventStream throws never
        }

        class Fixture {
            implements Live {
                function events(self) -> EventStream throws never {
                    [Event { value: 2 }, Event { value: 3 }]
                }
            }
        }

        function total(live: Live) -> int throws never {
            let sum = 0;
            for (let event in live.events()) {
                sum += event.value;
            }
            sum
        }

        function main() -> int throws never {
            total(Fixture {})
        }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}
