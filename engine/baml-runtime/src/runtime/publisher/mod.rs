use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    path::PathBuf,
    sync::Arc,
};

use baml_rpc::{
    ast::{ast_node_id::AstNodeId, tops::BamlFunctionId},
    BamlTypeId,
};
use baml_types::ir_type::TypeNonStreaming;
use cowstr::CowStr;
use internal_baml_core::ir::ir_hasher;
use serde::Serialize;

use crate::{
    internal::ir_features::WithInternal, tracingv2::publisher::rpc_converters::TypeLookup,
};

/// Type alias for a value with its dependencies
pub type WithDependency<T> = (Arc<T>, Arc<Vec<Arc<BamlTypeId>>>);

use super::super::tracingv2::publisher::rpc_converters::IntoRpcEvent;

#[derive(Serialize)]
pub struct TypeWithDependencies {
    pub type_id: WithDependency<BamlTypeId>,
    pub field_type: Arc<TypeNonStreaming>,
    pub class_fields: Option<Arc<Vec<(String, Arc<TypeNonStreaming>)>>>,
    pub enum_values: Option<Arc<Vec<String>>>,
}

#[derive(Serialize)]
pub struct FunctionSignatureWithDependencies {
    pub function_id: WithDependency<BamlFunctionId>,
    pub inputs: Arc<Vec<(String, TypeNonStreaming)>>,
    pub output: Arc<TypeNonStreaming>,
}

#[derive(Default, Serialize)]
pub struct AstSignatureWrapper {
    /// Path to source code
    pub source_code: HashMap<PathBuf, CowStr>,
    pub functions: HashMap<String, FunctionSignatureWithDependencies>,
    pub types: HashMap<String, TypeWithDependencies>,
    pub env_vars: HashMap<String, String>,
}

impl AstSignatureWrapper {
    pub fn env_var(&self, key: &str) -> Option<&String> {
        self.env_vars.get(key)
    }

    pub fn baml_src_hash(&self) -> Option<String> {
        let mut hasher = DefaultHasher::new();

        // Sort source files by filename for deterministic hashing
        let mut sorted_source_code: Vec<_> = self.source_code.iter().collect();
        sorted_source_code.sort_by(|a, b| a.0.cmp(b.0));

        for (source_file_path, content) in sorted_source_code {
            source_file_path.hash(&mut hasher);
            content.hash(&mut hasher);
        }

        // Return the hash as a hexadecimal string
        Some(format!("{:x}", hasher.finish()))
    }
}

impl TypeLookup for AstSignatureWrapper {
    fn type_lookup(&self, name: &str) -> Option<Arc<BamlTypeId>> {
        self.types.get(name).map(|t| t.type_id.0.clone())
    }

    fn function_lookup(&self, name: &str) -> Option<Arc<BamlFunctionId>> {
        self.functions.get(name).map(|f| f.function_id.0.clone())
    }

    fn baml_src_hash(&self) -> Option<String> {
        self.baml_src_hash()
    }
}

impl TryFrom<(Arc<crate::BamlRuntime>, HashMap<String, String>)> for AstSignatureWrapper {
    type Error = anyhow::Error;

    fn try_from(
        (runtime, env_vars): (Arc<crate::BamlRuntime>, HashMap<String, String>),
    ) -> Result<Self, Self::Error> {
        let ir_signature = ir_hasher::IRSignature::new_from_ir(runtime.ir())?;

        let name_to_baml_type_id_map: HashMap<String, Arc<BamlTypeId>> = ir_signature
            .classes
            .iter()
            .map(|(name, (type_node_sig, _class_details))| {
                (
                    name.clone(),
                    Arc::new(BamlTypeId(type_node_sig.signature.clone_into_ast_node_id())),
                )
            })
            .chain(
                ir_signature
                    .enums
                    .iter()
                    .map(|(name, (type_node_sig, _enum_details))| {
                        (
                            name.clone(),
                            Arc::new(BamlTypeId(type_node_sig.signature.clone_into_ast_node_id())),
                        )
                    }),
            )
            .chain(
                ir_signature
                    .type_aliases
                    .iter()
                    .map(|(name, type_node_sig)| {
                        (
                            name.clone(),
                            Arc::new(BamlTypeId(type_node_sig.signature.clone_into_ast_node_id())),
                        )
                    }),
            )
            .collect();

        let functions: HashMap<String, FunctionSignatureWithDependencies> = ir_signature
            .functions
            .into_iter()
            .map(|(name, func_sig)| {
                let dep_names_vec: Vec<String> = func_sig.signature.dependency_names().clone();
                let dependencies = dep_names_vec
                    .iter()
                    .filter_map(|dep_name| name_to_baml_type_id_map.get(dep_name).cloned())
                    .collect::<Vec<Arc<BamlTypeId>>>();
                (
                    name,
                    FunctionSignatureWithDependencies {
                        function_id: (
                            Arc::new(BamlFunctionId(func_sig.signature.clone_into_ast_node_id())),
                            Arc::new(dependencies),
                        ),
                        inputs: func_sig.inputs.clone(),
                        output: func_sig.output.clone(),
                    },
                )
            })
            .collect();

        let types: HashMap<String, TypeWithDependencies> = ir_signature
            .classes
            .into_iter()
            .map(|(name, (type_node_sig, class_details))| {
                let dep_names_vec: Vec<String> = type_node_sig.signature.dependency_names().clone();
                let dependencies = dep_names_vec
                    .iter()
                    .filter_map(|dep_name| name_to_baml_type_id_map.get(dep_name).cloned())
                    .collect::<Vec<Arc<BamlTypeId>>>();
                (
                    name.clone(),
                    TypeWithDependencies {
                        type_id: (
                            Arc::new(BamlTypeId(type_node_sig.signature.clone_into_ast_node_id())),
                            Arc::new(dependencies),
                        ),
                        field_type: type_node_sig.field_type.clone(),
                        class_fields: Some(class_details.fields.clone()),
                        enum_values: None,
                    },
                )
            })
            .chain(
                ir_signature
                    .enums
                    .into_iter()
                    .map(|(name, (type_node_sig, enum_details))| {
                        let dep_names_vec: Vec<String> =
                            type_node_sig.signature.dependency_names().clone();
                        let dependencies = dep_names_vec
                            .iter()
                            .filter_map(|dep_name| name_to_baml_type_id_map.get(dep_name).cloned())
                            .collect::<Vec<Arc<BamlTypeId>>>();
                        (
                            name.clone(),
                            TypeWithDependencies {
                                type_id: (
                                    Arc::new(BamlTypeId(
                                        type_node_sig.signature.clone_into_ast_node_id(),
                                    )),
                                    Arc::new(dependencies),
                                ),
                                field_type: type_node_sig.field_type.clone(),
                                class_fields: None,
                                enum_values: Some(enum_details.values.clone()),
                            },
                        )
                    }),
            )
            .chain(
                ir_signature
                    .type_aliases
                    .into_iter()
                    .map(|(name, type_node_sig)| {
                        let dep_names_vec: Vec<String> =
                            type_node_sig.signature.dependency_names().clone();
                        let dependencies = dep_names_vec
                            .iter()
                            .filter_map(|dep_name| name_to_baml_type_id_map.get(dep_name).cloned())
                            .collect::<Vec<Arc<BamlTypeId>>>();
                        (
                            name.clone(),
                            TypeWithDependencies {
                                type_id: (
                                    Arc::new(BamlTypeId(
                                        type_node_sig.signature.clone_into_ast_node_id(),
                                    )),
                                    Arc::new(dependencies),
                                ),
                                field_type: type_node_sig.field_type.clone(),
                                class_fields: None,
                                enum_values: None,
                            },
                        )
                    }),
            )
            .collect();

        let source_code = runtime
            .source_files
            .iter()
            .map(|file| (file.path_buf().clone(), CowStr::from(file.as_str())))
            .collect();

        Ok(Self {
            env_vars,
            functions,
            types,
            source_code,
        })
    }
}

// Helper extension trait to convert ir_hasher::Signature to AstNodeId
trait SignatureExt {
    fn clone_into_ast_node_id(&self) -> AstNodeId;
}

impl SignatureExt for internal_baml_core::ir::ir_hasher::Signature {
    fn clone_into_ast_node_id(&self) -> AstNodeId {
        let interface_hash = self.interface_hash();
        let impl_hash = self.implementation_hash();
        let name = self.display_name().to_string();

        match self.r#type {
            internal_baml_core::ir::ir_hasher::SignatureType::Class => {
                AstNodeId::new_class(name, interface_hash, impl_hash)
            }
            internal_baml_core::ir::ir_hasher::SignatureType::Enum => {
                AstNodeId::new_enum(name, interface_hash, impl_hash)
            }
            internal_baml_core::ir::ir_hasher::SignatureType::TypeAlias => {
                AstNodeId::new_type_alias(name, interface_hash, impl_hash)
            }
            internal_baml_core::ir::ir_hasher::SignatureType::Function => {
                AstNodeId::new_function(name, interface_hash, impl_hash)
            }
            _ => panic!(
                "Unsupported signature type for AstNodeId conversion: {:?}",
                self.r#type
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    /// Creates fake file content of specified size in bytes
    fn create_fake_content(size_bytes: usize) -> String {
        // Create content with repeated patterns to simulate real code
        let pattern = r###"
function TestFunction(input: string) -> MyClass {
  client GPT4Turbo
  prompt #"
    You are an expert assistant.
    Please process this input: {{input}}
    Return a structured response.
  "#
}

class MyClass {
  name string
  value int
  description string?
  items MyItem[]
}

class MyItem {
  id string
  data string
  nested NestedData?
}

class NestedData {
  key string
  value string
  metadata string?
}

enum Status {
  ACTIVE
  INACTIVE
  PENDING
  COMPLETED
}
"###;

        let pattern_size = pattern.len();
        let repetitions = size_bytes.div_ceil(pattern_size); // ceiling division

        let mut content = String::with_capacity(size_bytes);
        for i in 0..repetitions {
            content.push_str(pattern);
            if content.len() >= size_bytes {
                break;
            }
        }

        // Truncate to exact size
        content.truncate(size_bytes);
        content
    }

    #[test]
    fn test_baml_src_hash_performance() {
        const TOTAL_SIZE_MB: usize = 5;
        const TOTAL_SIZE_BYTES: usize = TOTAL_SIZE_MB * 1024 * 1024;
        const NUM_FILES: usize = 50; // Distribute across multiple files
        const BYTES_PER_FILE: usize = TOTAL_SIZE_BYTES / NUM_FILES;

        println!(
            "Creating fake file map with {NUM_FILES} files, {BYTES_PER_FILE} bytes each, total {TOTAL_SIZE_MB}MB"
        );

        // Create fake source code map totaling 5MB
        let mut source_code = HashMap::new();

        for i in 0..NUM_FILES {
            let file_path = PathBuf::from(format!("baml_src/file_{i:03}.baml"));
            let content = create_fake_content(BYTES_PER_FILE);
            source_code.insert(file_path, CowStr::from(content));
        }

        // Verify total size
        let actual_total_size: usize = source_code.values().map(|content| content.len()).sum();
        println!(
            "Actual total size: {} bytes ({:.2}MB)",
            actual_total_size,
            actual_total_size as f64 / (1024.0 * 1024.0)
        );

        // Create AstSignatureWrapper with the fake data
        let wrapper = AstSignatureWrapper {
            source_code,
            functions: HashMap::new(),
            types: HashMap::new(),
            env_vars: HashMap::new(),
        };

        // Warm up - run hash a few times to get consistent timing
        println!("Warming up...");
        for _ in 0..3 {
            let _ = wrapper.baml_src_hash();
        }

        // Performance test - run multiple iterations
        const ITERATIONS: usize = 10;
        let mut timings = Vec::with_capacity(ITERATIONS);

        println!("Running {ITERATIONS} performance iterations...");
        for i in 0..ITERATIONS {
            let start = Instant::now();
            let hash = wrapper.baml_src_hash();
            let duration = start.elapsed();

            timings.push(duration);

            // Verify we get a hash
            assert!(hash.is_some());
            let hash_str = hash.unwrap();
            assert!(!hash_str.is_empty());

            if i == 0 {
                println!("First hash result: {hash_str}");
            }
        }

        // Calculate statistics
        let total_time: std::time::Duration = timings.iter().sum();
        let avg_time = total_time / ITERATIONS as u32;
        let min_time = *timings.iter().min().unwrap();
        let max_time = *timings.iter().max().unwrap();

        println!("\nPerformance Results:");
        println!("Data size: {TOTAL_SIZE_MB}MB ({actual_total_size} bytes)");
        println!("Number of files: {NUM_FILES}");
        println!("Iterations: {ITERATIONS}");
        println!("Average time: {avg_time:?}");
        println!("Min time: {min_time:?}");
        println!("Max time: {max_time:?}");
        println!(
            "Throughput: {:.2} MB/s",
            (actual_total_size as f64 / (1024.0 * 1024.0)) / avg_time.as_secs_f64()
        );

        // Assert reasonable performance bounds
        // Hash should complete within a reasonable time for 5MB
        assert!(
            avg_time.as_millis() < 1000,
            "Hash took too long: {avg_time:?} for {TOTAL_SIZE_MB}MB"
        );

        // Test deterministic hashing - same input should produce same hash
        let hash1 = wrapper.baml_src_hash().unwrap();
        let hash2 = wrapper.baml_src_hash().unwrap();
        assert_eq!(hash1, hash2, "Hash should be deterministic");

        println!("✅ Performance test completed successfully!");
    }

    #[test]
    fn test_baml_src_hash_deterministic_sorting() {
        // Test that file order doesn't affect hash due to sorting
        let content1 = "function Test1() -> string { }";
        let content2 = "class TestClass { name string }";

        // Create two identical wrappers with files in different insertion order
        let mut wrapper1 = AstSignatureWrapper {
            source_code: HashMap::new(),
            functions: HashMap::new(),
            types: HashMap::new(),
            env_vars: HashMap::new(),
        };

        let mut wrapper2 = AstSignatureWrapper {
            source_code: HashMap::new(),
            functions: HashMap::new(),
            types: HashMap::new(),
            env_vars: HashMap::new(),
        };

        // Insert in different orders
        wrapper1
            .source_code
            .insert(PathBuf::from("a.baml"), CowStr::from(content1));
        wrapper1
            .source_code
            .insert(PathBuf::from("b.baml"), CowStr::from(content2));

        wrapper2
            .source_code
            .insert(PathBuf::from("b.baml"), CowStr::from(content2));
        wrapper2
            .source_code
            .insert(PathBuf::from("a.baml"), CowStr::from(content1));

        let hash1 = wrapper1.baml_src_hash().unwrap();
        let hash2 = wrapper2.baml_src_hash().unwrap();

        assert_eq!(
            hash1, hash2,
            "Hash should be deterministic regardless of insertion order"
        );
    }

    /// Standalone function to demonstrate the performance test
    /// This can be called independently to test baml_src_hash performance
    #[test]
    pub fn benchmark_baml_src_hash() {
        println!("🚀 Starting BAML Source Hash Performance Benchmark");
        println!("{}", "=".repeat(60));

        const TOTAL_SIZE_MB: usize = 5;
        const TOTAL_SIZE_BYTES: usize = TOTAL_SIZE_MB * 1024 * 1024;
        const NUM_FILES: usize = 50;
        const BYTES_PER_FILE: usize = TOTAL_SIZE_BYTES / NUM_FILES;

        // Generate realistic BAML content pattern for more accurate testing
        let baml_pattern = r###"
// Auto-generated test function
function ProcessData{{id}}(input: DataInput{{id}}) -> DataOutput{{id}} {
  client GPT4Turbo
  prompt #"
    You are a specialized data processor for type {{id}}.
    Process the following input data:
    
    Input: {{input}}
    
    Please analyze and return structured output following the DataOutput{{id}} schema.
    Ensure all required fields are populated with meaningful values.
  "#
}

class DataInput{{id}} {
  id string @description("Unique identifier")
  content string @description("Main content to process") 
  metadata Metadata{{id}}? @description("Optional metadata")
  tags string[] @description("List of tags")
  priority Priority @description("Processing priority")
}

class DataOutput{{id}} {
  processed_id string @description("Generated processing ID")
  result string @description("Processing result")
  confidence float @description("Confidence score 0-1")
  suggestions string[] @description("List of suggestions")
  metadata ProcessedMetadata{{id}} @description("Processing metadata")
}

class Metadata{{id}} {
  created_at string @description("Creation timestamp")
  author string @description("Content author")
  version int @description("Version number")  
  source string? @description("Data source")
}

class ProcessedMetadata{{id}} {
  processing_time string @description("Time taken to process")
  model_version string @description("AI model version used")
  tokens_used int @description("Number of tokens consumed")
  status ProcessingStatus @description("Processing status")
}

enum Priority {
  LOW @description("Low priority processing")
  MEDIUM @description("Medium priority processing") 
  HIGH @description("High priority processing")
  URGENT @description("Urgent processing required")
}

enum ProcessingStatus {
  SUCCESS @description("Processing completed successfully")
  PARTIAL @description("Partial processing completed")
  FAILED @description("Processing failed")
  TIMEOUT @description("Processing timed out")
}
"###;

        println!("📝 Generating {NUM_FILES} files with ~{BYTES_PER_FILE} bytes each");

        let mut source_code = HashMap::new();

        for i in 0..NUM_FILES {
            let file_path = PathBuf::from(format!("baml_src/module_{i:03}.baml"));

            // Create unique content by replacing {{id}} with the file number
            let file_specific_content = baml_pattern.replace("{{id}}", &i.to_string());

            // Repeat content to reach target size
            let pattern_size = file_specific_content.len();
            let repetitions = BYTES_PER_FILE.div_ceil(pattern_size);

            let mut content = String::with_capacity(BYTES_PER_FILE);
            for _ in 0..repetitions {
                content.push_str(&file_specific_content);
                if content.len() >= BYTES_PER_FILE {
                    break;
                }
            }
            content.truncate(BYTES_PER_FILE);

            source_code.insert(file_path, CowStr::from(content));
        }

        let actual_total_size: usize = source_code.values().map(|content| content.len()).sum();
        println!(
            "✅ Generated {} files, total size: {:.2}MB",
            NUM_FILES,
            actual_total_size as f64 / (1024.0 * 1024.0)
        );

        // Create test wrapper
        let wrapper = AstSignatureWrapper {
            source_code,
            functions: HashMap::new(),
            types: HashMap::new(),
            env_vars: HashMap::new(),
        };

        // Warmup phase
        println!("🔥 Warmup phase...");
        for i in 0..5 {
            let start = Instant::now();
            let _ = wrapper.baml_src_hash();
            let duration = start.elapsed();
            println!("  Warmup #{}: {:?}", i + 1, duration);
        }

        // Main benchmark
        println!("⚡ Performance benchmark phase...");
        const ITERATIONS: usize = 20;
        let mut timings = Vec::with_capacity(ITERATIONS);

        for i in 0..ITERATIONS {
            let start = Instant::now();
            let hash = wrapper.baml_src_hash();
            let duration = start.elapsed();

            timings.push(duration);

            if i == 0 {
                println!(
                    "  First hash: {}",
                    hash.unwrap_or_else(|| "None".to_string())
                );
            }
        }

        // Statistics
        let total_time: std::time::Duration = timings.iter().sum();
        let avg_time = total_time / ITERATIONS as u32;
        let min_time = *timings.iter().min().unwrap();
        let max_time = *timings.iter().max().unwrap();
        let median_time = {
            let mut sorted = timings.clone();
            sorted.sort();
            sorted[sorted.len() / 2]
        };

        println!("\n📊 PERFORMANCE RESULTS");
        println!(
            "📦 Data size: {:.2}MB ({} bytes)",
            actual_total_size as f64 / (1024.0 * 1024.0),
            actual_total_size
        );
        println!("📁 Number of files: {NUM_FILES}");
        println!("🔄 Iterations: {ITERATIONS}");
        println!("⏱️  Average time: {avg_time:?}");
        println!("⚡ Min time: {min_time:?}");
        println!("🐌 Max time: {max_time:?}");
        println!("📈 Median time: {median_time:?}");
        println!(
            "🚀 Throughput: {:.2} MB/s",
            (actual_total_size as f64 / (1024.0 * 1024.0)) / avg_time.as_secs_f64()
        );

        // Performance validation
        if avg_time.as_millis() > 1000 {
            println!(
                "⚠️  WARNING: Average time ({avg_time:?}) exceeds 1 second for {TOTAL_SIZE_MB}MB"
            );
        } else {
            println!("✅ Performance is within acceptable bounds!");
        }

        // Deterministic test
        let hash1 = wrapper.baml_src_hash().unwrap();
        let hash2 = wrapper.baml_src_hash().unwrap();
        if hash1 == hash2 {
            println!("✅ Hash is deterministic");
        } else {
            println!("❌ Hash is NOT deterministic!");
        }

        println!("🎉 Benchmark completed successfully!");
    }
}
