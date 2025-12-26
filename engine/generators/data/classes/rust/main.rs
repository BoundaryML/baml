// Test file for generated BAML client
// This will be compiled against the generated baml_client module

mod baml_client;

fn main() {
    println!("Test - baml_client module loaded successfully!");
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_baml_client_compiles() {
        // This test verifies the generated code compiles
        println!("baml_client module compiles successfully");
    }
}
