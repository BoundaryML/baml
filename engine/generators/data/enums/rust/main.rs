// Test file for generated BAML client
// This will be compiled against the generated baml_client module

mod baml_client;

use baml_client::sync_client::B;
use baml_client::types::*;

fn main() {
    println!("Test - baml_client module loaded successfully!");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consume_test_enum() {
        let result = B.ConsumeTestEnum
            .call(&TestEnum::Confused)
            .expect("Failed to call ConsumeTestEnum");

        // Basic validation that we got a result (non-empty means we got something back)
        // The function returns a TestEnum, so if it succeeded, we have a valid enum value
        assert!(
            matches!(
                result,
                TestEnum::Angry
                    | TestEnum::Happy
                    | TestEnum::Sad
                    | TestEnum::Confused
                    | TestEnum::Excited
                    | TestEnum::Exclamation
                    | TestEnum::Bored
            ),
            "Expected a valid TestEnum variant"
        );
    }

    #[test]
    fn test_fn_test_aliased_enum_output() {
        // Test with "mehhhhh" input
        let result = B.FnTestAliasedEnumOutput
            .call("mehhhhh")
            .expect("Failed to call FnTestAliasedEnumOutput");

        assert_eq!(
            result,
            TestEnum::Bored,
            "Expected result to be TestEnum::Bored, got {:?}",
            result
        );
    }

    #[test]
    fn test_fn_test_aliased_enum_variants() {
        // Test different inputs to get different variants
        let test_cases = [
            ("I am so angry right now", TestEnum::Angry),      // Should map to Angry (k1)
            ("I'm feeling really happy", TestEnum::Happy),     // Should map to Happy (k22)
            ("This makes me sad", TestEnum::Sad),              // Should map to Sad (k11)
            ("I don't understand", TestEnum::Confused),        // Should map to Confused (k44)
            ("I'm so excited!", TestEnum::Excited),            // Should map to Excited (no alias)
            ("k5", TestEnum::Excited),                         // Should map to Exclamation (k5)
            ("I'm bored and this is a long message", TestEnum::Bored), // Should map to Bored (k6)
        ];

        for (input, expected) in test_cases {
            let result = B.FnTestAliasedEnumOutput
                .call(input)
                .unwrap_or_else(|e| panic!("Error testing input '{}': {:?}", input, e));

            assert_eq!(
                result, expected,
                "For input '{}': expected {:?}, got {:?}",
                input, expected, result
            );
        }
    }
}
