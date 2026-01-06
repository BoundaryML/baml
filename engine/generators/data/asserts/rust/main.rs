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
    fn test_person_test() {
        let result = B.PersonTest.call().expect("Failed to call PersonTest");
        println!("{:?}", result);

        // Validate the Person struct has expected fields
        assert!(!result.name.is_empty(), "Expected name to not be empty");
        assert!(result.age > 0, "Expected age to be greater than 0");
    }

    #[test]
    fn test_person_test_stream() {
        let mut stream = B.PersonTest
            .stream()
            .expect("Failed to start PersonTest stream");

        let mut partial_count = 0;
        for partial in stream.partials() {
            let _partial = partial.expect("Error receiving partial");
            partial_count += 1;
        }

        let final_result = stream
            .get_final_response()
            .expect("Failed to get final response");

        assert!(!final_result.name.is_empty(), "Expected name to not be empty");
        assert!(final_result.age > 0, "Expected age to be greater than 0");
        println!(
            "PersonTest stream completed with {} partials: {:?}",
            partial_count, final_result
        );
    }
}
