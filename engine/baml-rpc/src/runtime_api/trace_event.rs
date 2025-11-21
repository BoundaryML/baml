use std::{borrow::Cow, collections::HashMap};

use baml_ids::{FunctionCallId, FunctionEventId};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{
    baml_function_call_error::BamlFunctionCallError,
    baml_value::{BamlValue, Media},
};
use crate::{
    ast::{evaluation_context::TypeBuilderValue, tops::BamlFunctionId},
    base::EpochMsTimestamp,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct TraceEventBatch<'a> {
    pub events: Vec<BackendTraceEvent<'a>>,
}

/// This is intentionally VERY similar to TraceEvent in
/// baml-lib/baml-types/src/tracing/events.rs
/// If the convertion from baml-types to baml-rpc is not possible,
/// WE HAVE A BREAKING CHANGE.
#[derive(Debug, Serialize, Deserialize)]
pub struct BackendTraceEvent<'a> {
    /*
     * (call_id, content_event_id) is a unique identifier for a log event
     * The query (call_id, *) gets all logs for a function call
     */
    pub call_id: FunctionCallId,

    // a unique identifier for this particular content
    pub function_event_id: FunctionEventId,

    // The chain of calls that lead to this log event
    // Includes call_id at the last position (content_event_id is not included)
    pub call_stack: Vec<FunctionCallId>,

    // The timestamp of the log
    #[serde(rename = "timestamp_epoch_ms")]
    pub timestamp: EpochMsTimestamp,

    // The content of the log
    pub content: TraceData<'a>,
}

// Same as tracing/events.rs FunctionType
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum FunctionType {
    BamlLlm,
    // BamlExternal, // extern function in baml
    // Baml // a function that is defined in baml, but not a baml llm function
    Native, // python or TS function we are @tracing.
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum TraceData<'a> {
    FunctionStart {
        function_display_name: String,
        args: Vec<(String, BamlValue<'a>)>,
        tags: TraceTags,
        function_type: FunctionType,
        is_stream: bool,
        /// Only sent for BAML defined functions
        baml_function_content: Option<BamlFunctionStart>,
    },
    /// Terminal Event
    FunctionEnd(FunctionEnd<'a>),

    /// Intermediate events between start and end
    Intermediate(IntermediateData<'a>),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BamlFunctionStart {
    pub function_id: std::sync::Arc<BamlFunctionId>,
    pub baml_src_hash: String,
    pub eval_context: EvaluationContext,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum FunctionEnd<'a> {
    Success { result: BamlValue<'a> },
    Error { error: BamlFunctionCallError<'a> },
}

pub type TraceTags = std::collections::HashMap<String, serde_json::Value>;

#[derive(Debug, Serialize, Deserialize)]
pub struct EvaluationContext {
    pub tags: TraceTags,

    pub type_builder: Option<TypeBuilderValue>,
    // TODO(hellovai): add this
    // pub client_registry: Option<ClientRegistryValue>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RpcClientDetails {
    pub name: String,
    pub provider: String,
    pub options: IndexMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum IntermediateData<'a> {
    /// These are all resolved from the client
    LLMRequest {
        client_name: String,
        client_provider: String,
        params: HashMap<String, Cow<'a, serde_json::Value>>,
        prompt: Vec<LLMChatMessage<'a>>,
    },
    RawLLMRequest {
        http_request_id: String,
        url: String,
        method: String,
        headers: HashMap<String, String>,
        client_details: RpcClientDetails,
        body: HTTPBody<'a>,
    },
    RawLLMResponse {
        http_request_id: String,
        status: u16,
        headers: Option<HashMap<String, String>>,
        body: HTTPBody<'a>,
        client_details: RpcClientDetails,
    },
    RawLLMResponseStream {
        http_request_id: String,
        event: Event<'a>,
    },
    LLMResponse {
        client_stack: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        finish_reason: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<LLMUsage>,

        #[serde(skip_serializing_if = "Option::is_none")]
        raw_text_output: Option<Cow<'a, str>>,
    },
    SetTags(TraceTags),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HTTPBody<'a> {
    #[serde(
        serialize_with = "serialize_bytes_as_string",
        deserialize_with = "deserialize_string_as_bytes"
    )]
    pub raw: Cow<'a, [u8]>,
}

fn serialize_bytes_as_string<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    // Serialize as text to avoid exploding arrays of bytes; use lossy UTF-8 if needed
    let s = String::from_utf8_lossy(bytes);
    serializer.serialize_str(&s)
}

fn deserialize_string_as_bytes<'de, D>(deserializer: D) -> Result<Cow<'static, [u8]>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct BytesVisitor;

    impl<'de> serde::de::Visitor<'de> for BytesVisitor {
        type Value = Cow<'static, [u8]>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or byte array")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Cow::Owned(value.as_bytes().to_vec()))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Cow::Owned(value.into_bytes()))
        }

        fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Cow::Owned(value.to_vec()))
        }

        fn visit_byte_buf<E>(self, value: Vec<u8>) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Cow::Owned(value))
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut bytes = Vec::new();
            while let Some(byte) = seq.next_element::<u8>()? {
                bytes.push(byte);
            }
            Ok(Cow::Owned(bytes))
        }
    }

    deserializer.deserialize_any(BytesVisitor)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Event<'a> {
    pub raw: Cow<'a, str>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LLMChatMessage<'a> {
    pub role: String,
    pub content: Vec<LLMChatMessagePart<'a>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LLMChatMessagePart<'a> {
    Text(Cow<'a, str>),
    Media(Media<'a>),
    WithMeta(
        Box<LLMChatMessagePart<'a>>,
        HashMap<String, serde_json::Value>,
    ),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LLMUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
}

// #[cfg(test)]
// mod tests {
//     use std::{fs, path::Path};

//     use super::*;

//     #[test]
//     fn test_deserialize_trace_events_debug_json() {
//         // Make sure the file exists
//         let path = Path::new(
//         "/Users/aaronvillalpando/Projects/baml/integ-tests/python/trace_events_debug_vaibhav.json",
//     );
//         assert!(path.exists(), "Test data file does not exist: {:?}", path);

//         // Read the file contents
//         let contents = fs::read_to_string(path).expect("Failed to read trace_events_debug.json");

//         // Deserialize each line as a separate BackendTraceEvent (NDJSON format)
//         let mut events = Vec::new();
//         let mut original_lines = Vec::new();
//         for (line_num, line) in contents.lines().enumerate() {
//             if line.trim().is_empty() {
//                 continue;
//             }

//             original_lines.push(line);
//             let event: BackendTraceEvent = serde_json::from_str(line).unwrap_or_else(|e| {
//                 panic!(
//                     "Failed to deserialize line {}: {:?}\nLine content: {}",
//                     line_num + 1,
//                     e,
//                     line
//                 )
//             });
//             events.push(event);
//         }

//         assert!(
//             !events.is_empty(),
//             "Deserialized events should not be empty"
//         );

//         // Serialize events back to JSON and compare with original
//         for (idx, event) in events.iter().enumerate() {
//             let serialized = serde_json::to_string(&event)
//                 .unwrap_or_else(|e| panic!("Failed to serialize event {}: {:?}", idx, e));

//             // Parse both as serde_json::Value for normalization (handles field order differences)
//             let original_value: serde_json::Value = serde_json::from_str(original_lines[idx])
//                 .unwrap_or_else(|e| {
//                     panic!("Failed to parse original line {} as JSON: {:?}", idx, e)
//                 });
//             let serialized_value: serde_json::Value = serde_json::from_str(&serialized)
//                 .unwrap_or_else(|e| {
//                     panic!("Failed to parse serialized line {} as JSON: {:?}", idx, e)
//                 });

//             assert_eq!(
//                 original_value, serialized_value,
//                 "Serialized event {} does not match original.\nOriginal: {}\nSerialized: {}",
//                 idx, original_lines[idx], serialized
//             );
//         }
//     }

//     #[test]
//     fn test_roundtrip_serialize_deserialize() {
//         // Read from original file
//         let original_path = Path::new(
//             "/Users/aaronvillalpando/Projects/baml/integ-tests/python/trace_events_debug_213.json",
//         );
//         assert!(
//             original_path.exists(),
//             "Test data file does not exist: {:?}",
//             original_path
//         );

//         let contents =
//             fs::read_to_string(original_path).expect("Failed to read trace_events_debug_213.json");

//         // Deserialize from original file
//         let mut original_events = Vec::new();
//         for (line_num, line) in contents.lines().enumerate() {
//             if line.trim().is_empty() {
//                 continue;
//             }

//             let event: BackendTraceEvent = serde_json::from_str(line).unwrap_or_else(|e| {
//                 panic!(
//                     "Failed to deserialize line {}: {:?}\nLine content: {}",
//                     line_num + 1,
//                     e,
//                     line
//                 )
//             });
//             original_events.push(event);
//         }

//         assert!(
//             !original_events.is_empty(),
//             "Deserialized events should not be empty"
//         );

//         // Serialize to a new file
//         let temp_path = Path::new(
//         "/Users/aaronvillalpando/Projects/baml/integ-tests/python/trace_events_debug_213_roundtrip.json",
//     );
//         let mut serialized_content = String::new();
//         for event in &original_events {
//             let line = serde_json::to_string(&event).expect("Failed to serialize event");
//             serialized_content.push_str(&line);
//             serialized_content.push('\n');
//         }
//         fs::write(temp_path, &serialized_content).expect("Failed to write serialized file");

//         // Deserialize from the new file
//         let roundtrip_contents =
//             fs::read_to_string(temp_path).expect("Failed to read roundtrip file");
//         let mut roundtrip_events = Vec::new();
//         for (line_num, line) in roundtrip_contents.lines().enumerate() {
//             if line.trim().is_empty() {
//                 continue;
//             }

//             let event: BackendTraceEvent = serde_json::from_str(line).unwrap_or_else(|e| {
//                 panic!(
//                     "Failed to deserialize roundtrip line {}: {:?}\nLine content: {}",
//                     line_num + 1,
//                     e,
//                     line
//                 )
//             });
//             roundtrip_events.push(event);
//         }

//         // Clean up temp file
//         fs::remove_file(temp_path).ok();

//         // Compare original and roundtrip events
//         assert_eq!(
//             original_events.len(),
//             roundtrip_events.len(),
//             "Number of events should match"
//         );

//         for (idx, (original, roundtrip)) in original_events
//             .iter()
//             .zip(roundtrip_events.iter())
//             .enumerate()
//         {
//             let original_json = serde_json::to_value(original).unwrap_or_else(|e| {
//                 panic!("Failed to convert original event {} to JSON: {:?}", idx, e)
//             });
//             let roundtrip_json = serde_json::to_value(roundtrip).unwrap_or_else(|e| {
//                 panic!("Failed to convert roundtrip event {} to JSON: {:?}", idx, e)
//             });

//             assert_eq!(
//                 original_json,
//                 roundtrip_json,
//                 "Event {} does not match after roundtrip.\nOriginal: {}\nRoundtrip: {}",
//                 idx,
//                 serde_json::to_string_pretty(&original_json).unwrap(),
//                 serde_json::to_string_pretty(&roundtrip_json).unwrap()
//             );
//         }
//     }

//     #[test]
//     fn test_serialize_trace_event_batch_to_newfile_and_gz() {
//         use std::fs;
//         // use std::io::Write;
//         // use std::time::Instant;

//         // You'll want to change these paths for your own testing/environment.
//         let orig_path =
//             "/Users/aaronvillalpando/Downloads/tracebatch_01kab00gkeeky8qfssrp43b0w5.json";
//         let new_path =
//             "/Users/aaronvillalpando/Downloads/newfile2-tracebatch_01kab00gkeeky8qfssrp43b0w5.json";
//         // let gz_path =
//         //     "/Users/aaronvillalpando/Downloads/newfile-tracebatch_01kab00gkeeky8qfssrp43b0w5.json.gz";

//         // Read source .json
//         let file_contents =
//             fs::read_to_string(orig_path).expect("Failed to read original tracebatch json file");

//         // Parse as TraceEventBatch
//         let batch: TraceEventBatch = serde_json::from_str(&file_contents)
//             .expect("Failed to parse TraceEventBatch from json");

//         // Serialize back to json
//         let json_str =
//             serde_json::to_string(&batch).expect("Failed to serialize TraceEventBatch to json");

//         // Write to new file in same dir, with newfile-<orig_filename>
//         fs::write(new_path, &json_str).expect("Failed to write new tracebatch json file");

//         // Optionally, assert the file exists and is not empty
//         let metadata = fs::metadata(new_path).expect("New file should exist");
//         assert!(metadata.len() > 0, "New file is empty");

//         // Now write a gzipped version and time it
//         // let start = Instant::now();

//         // let gz_file = fs::File::create(gz_path).expect("Failed to create gz output file");
//         // let mut encoder = flate2::write::GzEncoder::new(gz_file, flate2::Compression::default());
//         // encoder
//         //     .write_all(json_str.as_bytes())
//         //     .expect("Failed to write gzipped json");
//         // encoder
//         //     .finish()
//         //     .expect("Failed to finish writing gzipped json");

//         // let duration = start.elapsed();
//         // let gz_metadata = fs::metadata(gz_path).expect("Gzipped file should exist");

//         // assert!(gz_metadata.len() > 0, "Gzipped file is empty");
//         // println!(
//         //     "Gzipped JSON tracebatch written to {} in {:?} ({} bytes)",
//         //     gz_path,
//         //     duration,
//         //     gz_metadata.len()
//         // );
//     }

//     #[test]
//     fn test_generate_type_representation() {
//         use std::path::Path;

//         // Process multiple trace event files
//         let files = vec![
//             // "trace_events_debug_vaibhav.json",
//             "trace_events_debug.json",
//         ];

//         for filename in files {
//             let path = Path::new("/Users/aaronvillalpando/Projects/baml/integ-tests/python")
//                 .join(filename);

//             if !path.exists() {
//                 println!("Skipping {}: file does not exist", filename);
//                 continue;
//             }

//             println!("\n=== Processing {} ===", filename);
//             process_trace_file(&path);
//         }
//     }

//     fn process_trace_file(path: &std::path::Path) {
//         use crate::ast::type_reference::TypeReference;
//         use crate::runtime_api::baml_value::ValueContent;
//         use std::collections::{HashMap, HashSet};
//         use std::fs;

//         let contents = fs::read_to_string(path).expect("Failed to read trace events file");

//         // Parse events - handle both compressed and non-compressed formats
//         let mut events = Vec::new();
//         for (line_num, line) in contents.lines().enumerate() {
//             if line.trim().is_empty() {
//                 continue;
//             }

//             // First parse as generic JSON value
//             let mut value: serde_json::Value = serde_json::from_str(line).unwrap_or_else(|e| {
//                 panic!(
//                     "Failed to parse line {} as JSON: {:?}\nLine content: {}",
//                     line_num + 1,
//                     e,
//                     line
//                 )
//             });

//             // Recursively add missing "data" fields for primitives in type_ref objects
//             fn add_missing_data_fields(value: &mut serde_json::Value) {
//                 if let Some(obj) = value.as_object_mut() {
//                     // Check if this is a type_ref with a primitive type and no data field
//                     if let Some(type_val) = obj.get("type") {
//                         if let Some(type_str) = type_val.as_str() {
//                             if matches!(type_str, "string" | "int" | "float" | "bool")
//                                 && !obj.contains_key("data")
//                             {
//                                 obj.insert(
//                                     "data".to_string(),
//                                     serde_json::json!({"checks": [], "asserts": []}),
//                                 );
//                             }
//                         }
//                     }
//                     // Recurse into all values
//                     for (_, v) in obj.iter_mut() {
//                         add_missing_data_fields(v);
//                     }
//                 } else if let Some(arr) = value.as_array_mut() {
//                     for v in arr {
//                         add_missing_data_fields(v);
//                     }
//                 }
//             }

//             add_missing_data_fields(&mut value);

//             // Now deserialize as BackendTraceEvent
//             let event: BackendTraceEvent = serde_json::from_value(value)
//                 .unwrap_or_else(|e| panic!("Failed to deserialize line {}: {:?}", line_num + 1, e));
//             events.push(event);
//         }

//         fn format_type_reference(type_ref: &TypeReference) -> String {
//             use crate::ast::type_reference::TypeReferenceWithMetadata;

//             match type_ref {
//                 TypeReferenceWithMetadata::String(_) => "string".to_string(),
//                 TypeReferenceWithMetadata::Int(_) => "int".to_string(),
//                 TypeReferenceWithMetadata::Float(_) => "float".to_string(),
//                 TypeReferenceWithMetadata::Bool(_) => "bool".to_string(),
//                 TypeReferenceWithMetadata::Media(media_type, _) => format!("{:?}", media_type),
//                 TypeReferenceWithMetadata::Literal(lit, _) => format!("{:?}", lit),
//                 TypeReferenceWithMetadata::Class { type_id, .. } => type_id.0.to_string(),
//                 TypeReferenceWithMetadata::Enum { type_id, .. } => type_id.0.to_string(),
//                 TypeReferenceWithMetadata::RecursiveTypeAlias { type_id, .. } => {
//                     type_id.0.to_string()
//                 }
//                 TypeReferenceWithMetadata::List(inner, _) => {
//                     format!("List[{}]", format_type_reference(inner))
//                 }
//                 TypeReferenceWithMetadata::Map { key, value, .. } => {
//                     format!(
//                         "Map<{}, {}>",
//                         format_type_reference(key),
//                         format_type_reference(value)
//                     )
//                 }
//                 TypeReferenceWithMetadata::Union { union_type, .. } => {
//                     let types: Vec<String> = union_type
//                         .types
//                         .iter()
//                         .map(|t| format_type_reference(t))
//                         .collect();
//                     let union_str = types.join(" | ");
//                     if union_type.is_nullable {
//                         format!("({} | null)", union_str)
//                     } else {
//                         format!("({})", union_str)
//                     }
//                 }
//                 TypeReferenceWithMetadata::Tuple { items, .. } => {
//                     let types: Vec<String> =
//                         items.iter().map(|t| format_type_reference(t)).collect();
//                     format!("({})", types.join(", "))
//                 }
//                 TypeReferenceWithMetadata::Unknown => "unknown".to_string(),
//             }
//         }

//         // Function to recursively format BamlValue with indentation
//         fn format_value_recursive(value: &BamlValue, indent: usize) -> String {
//             let indent_str = "  ".repeat(indent);
//             let type_str = format_type_reference(&value.metadata.type_ref);
//             let mut result = format!("{}{}", indent_str, type_str);

//             match &value.value {
//                 ValueContent::Class { fields } => {
//                     result.push_str(" {\n");
//                     let mut sorted_fields: Vec<_> = fields.iter().collect();
//                     sorted_fields.sort_by_key(|(k, _)| k.as_str());
//                     for (field_name, field_value) in sorted_fields {
//                         result.push_str(&format!("{}{}: ", "  ".repeat(indent + 1), field_name));
//                         result.push_str(
//                             &format_value_recursive(field_value, indent + 1).trim_start(),
//                         );
//                     }
//                     result.push_str(&format!("{}}}\n", indent_str));
//                 }
//                 ValueContent::Map(map) => {
//                     if !map.is_empty() {
//                         result.push_str(" {\n");
//                         let mut sorted_map: Vec<_> = map.iter().collect();
//                         sorted_map.sort_by_key(|(k, _)| k.as_str());
//                         for (key, val) in sorted_map {
//                             result.push_str(&format!("{}{}: ", "  ".repeat(indent + 1), key));
//                             result.push_str(&format_value_recursive(val, indent + 1).trim_start());
//                         }
//                         result.push_str(&format!("{}}}\n", indent_str));
//                     } else {
//                         result.push('\n');
//                     }
//                 }
//                 ValueContent::List(items) => {
//                     if !items.is_empty() {
//                         result.push_str(" [\n");
//                         for (idx, item) in items.iter().enumerate() {
//                             result.push_str(&format!("{}[{}]: ", "  ".repeat(indent + 1), idx));
//                             result.push_str(&format_value_recursive(item, indent + 1).trim_start());
//                         }
//                         result.push_str(&format!("{}]\n", indent_str));
//                     } else {
//                         result.push('\n');
//                     }
//                 }
//                 _ => {
//                     result.push('\n');
//                 }
//             }

//             result
//         }

//         // Collect function inputs from FunctionStart events only
//         let mut output = String::new();
//         output.push_str("// Function Input Type Representation\n\n");

//         let mut function_calls: Vec<(String, Vec<(String, &BamlValue)>)> = Vec::new();

//         for event in &events {
//             if let TraceData::FunctionStart {
//                 function_display_name,
//                 args,
//                 ..
//             } = &event.content
//             {
//                 let arg_refs: Vec<(String, &BamlValue)> = args
//                     .iter()
//                     .map(|(name, value)| (name.clone(), value))
//                     .collect();
//                 function_calls.push((function_display_name.clone(), arg_refs));
//             }
//         }

//         // Group by function name
//         let mut functions_by_name: HashMap<String, Vec<Vec<(String, &BamlValue)>>> = HashMap::new();
//         for (func_name, args) in function_calls {
//             functions_by_name
//                 .entry(func_name)
//                 .or_insert_with(Vec::new)
//                 .push(args);
//         }

//         let mut sorted_functions: Vec<_> = functions_by_name.keys().cloned().collect();
//         sorted_functions.sort();

//         for function_name in sorted_functions {
//             let all_args = &functions_by_name[&function_name];

//             output.push_str(&format!("function {}(\n", function_name));

//             // Collect unique argument structures
//             let mut seen_args = HashSet::new();
//             for args in all_args {
//                 let arg_sig: String = args
//                     .iter()
//                     .map(|(name, val)| {
//                         format!(
//                             "{}: {}",
//                             name,
//                             format_type_reference(&val.metadata.type_ref)
//                         )
//                     })
//                     .collect::<Vec<_>>()
//                     .join(", ");

//                 if seen_args.insert(arg_sig.clone()) {
//                     // Print each argument with nested structure
//                     for (arg_name, arg_value) in args {
//                         output.push_str(&format!("  {}: ", arg_name));
//                         output.push_str(&format_value_recursive(arg_value, 1).trim_start());
//                     }
//                 }
//             }

//             output.push_str(");\n\n");
//         }

//         // Write to file
//         // let filename = path.file_stem().unwrap().to_str().unwrap();
//         // let output_path = path.parent().unwrap().join(format!("repr-{}.ts", filename));

//         // fs::write(&output_path, &output).expect("Failed to write type representation");

//         // println!("Type representation written to {:?}", output_path);
//         println!("Found {} unique functions", functions_by_name.len());
//         println!("Total lines: {}", output.lines().count());

//         // Print the full output
//         println!("\n{}", output);

//         assert!(
//             !functions_by_name.is_empty(),
//             "Should have collected function signatures"
//         );
//     }
// }
