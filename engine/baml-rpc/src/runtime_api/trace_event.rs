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

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::*;

    #[test]
    fn test_deserialize_trace_events_debug_json() {
        // Make sure the file exists
        let path = Path::new(
            "/Users/aaronvillalpando/Projects/baml/integ-tests/python/trace_events_debug_vaibhav.json",
        );
        assert!(path.exists(), "Test data file does not exist: {:?}", path);

        // Read the file contents
        let contents = fs::read_to_string(path).expect("Failed to read trace_events_debug.json");

        // Deserialize each line as a separate BackendTraceEvent (NDJSON format)
        let mut events = Vec::new();
        let mut original_lines = Vec::new();
        for (line_num, line) in contents.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }

            original_lines.push(line);
            let event: BackendTraceEvent = serde_json::from_str(line).unwrap_or_else(|e| {
                panic!(
                    "Failed to deserialize line {}: {:?}\nLine content: {}",
                    line_num + 1,
                    e,
                    line
                )
            });
            events.push(event);
        }

        assert!(
            !events.is_empty(),
            "Deserialized events should not be empty"
        );

        // Serialize events back to JSON and compare with original
        for (idx, event) in events.iter().enumerate() {
            let serialized = serde_json::to_string(&event)
                .unwrap_or_else(|e| panic!("Failed to serialize event {}: {:?}", idx, e));

            // Parse both as serde_json::Value for normalization (handles field order differences)
            let original_value: serde_json::Value = serde_json::from_str(original_lines[idx])
                .unwrap_or_else(|e| {
                    panic!("Failed to parse original line {} as JSON: {:?}", idx, e)
                });
            let serialized_value: serde_json::Value = serde_json::from_str(&serialized)
                .unwrap_or_else(|e| {
                    panic!("Failed to parse serialized line {} as JSON: {:?}", idx, e)
                });

            assert_eq!(
                original_value, serialized_value,
                "Serialized event {} does not match original.\nOriginal: {}\nSerialized: {}",
                idx, original_lines[idx], serialized
            );
        }
    }

    #[test]
    fn test_roundtrip_serialize_deserialize() {
        // Read from original file
        let original_path = Path::new(
            "/Users/aaronvillalpando/Projects/baml/integ-tests/python/trace_events_debug_213.json",
        );
        assert!(
            original_path.exists(),
            "Test data file does not exist: {:?}",
            original_path
        );

        let contents =
            fs::read_to_string(original_path).expect("Failed to read trace_events_debug_213.json");

        // Deserialize from original file
        let mut original_events = Vec::new();
        for (line_num, line) in contents.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }

            let event: BackendTraceEvent = serde_json::from_str(line).unwrap_or_else(|e| {
                panic!(
                    "Failed to deserialize line {}: {:?}\nLine content: {}",
                    line_num + 1,
                    e,
                    line
                )
            });
            original_events.push(event);
        }

        assert!(
            !original_events.is_empty(),
            "Deserialized events should not be empty"
        );

        // Serialize to a new file
        let temp_path = Path::new(
            "/Users/aaronvillalpando/Projects/baml/integ-tests/python/trace_events_debug_213_roundtrip.json",
        );
        let mut serialized_content = String::new();
        for event in &original_events {
            let line = serde_json::to_string(&event).expect("Failed to serialize event");
            serialized_content.push_str(&line);
            serialized_content.push('\n');
        }
        fs::write(temp_path, &serialized_content).expect("Failed to write serialized file");

        // Deserialize from the new file
        let roundtrip_contents =
            fs::read_to_string(temp_path).expect("Failed to read roundtrip file");
        let mut roundtrip_events = Vec::new();
        for (line_num, line) in roundtrip_contents.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }

            let event: BackendTraceEvent = serde_json::from_str(line).unwrap_or_else(|e| {
                panic!(
                    "Failed to deserialize roundtrip line {}: {:?}\nLine content: {}",
                    line_num + 1,
                    e,
                    line
                )
            });
            roundtrip_events.push(event);
        }

        // Clean up temp file
        fs::remove_file(temp_path).ok();

        // Compare original and roundtrip events
        assert_eq!(
            original_events.len(),
            roundtrip_events.len(),
            "Number of events should match"
        );

        for (idx, (original, roundtrip)) in original_events
            .iter()
            .zip(roundtrip_events.iter())
            .enumerate()
        {
            let original_json = serde_json::to_value(original).unwrap_or_else(|e| {
                panic!("Failed to convert original event {} to JSON: {:?}", idx, e)
            });
            let roundtrip_json = serde_json::to_value(roundtrip).unwrap_or_else(|e| {
                panic!("Failed to convert roundtrip event {} to JSON: {:?}", idx, e)
            });

            assert_eq!(
                original_json,
                roundtrip_json,
                "Event {} does not match after roundtrip.\nOriginal: {}\nRoundtrip: {}",
                idx,
                serde_json::to_string_pretty(&original_json).unwrap(),
                serde_json::to_string_pretty(&roundtrip_json).unwrap()
            );
        }
    }

    // #[test]
    // fn test_serialize_trace_event_batch_to_newfile_and_gz() {
    //     use std::fs;
    //     // use std::io::Write;
    //     // use std::time::Instant;

    //     // You'll want to change these paths for your own testing/environment.
    //     let orig_path =
    //         "/Users/aaronvillalpando/Downloads/tracebatch_01kab00gkeeky8qfssrp43b0w5.json";
    //     let new_path =
    //         "/Users/aaronvillalpando/Downloads/newfile2-tracebatch_01kab00gkeeky8qfssrp43b0w5.json";
    //     // let gz_path =
    //     //     "/Users/aaronvillalpando/Downloads/newfile-tracebatch_01kab00gkeeky8qfssrp43b0w5.json.gz";

    //     // Read source .json
    //     let file_contents =
    //         fs::read_to_string(orig_path).expect("Failed to read original tracebatch json file");

    //     // Parse as TraceEventBatch
    //     let batch: TraceEventBatch = serde_json::from_str(&file_contents)
    //         .expect("Failed to parse TraceEventBatch from json");

    //     // Serialize back to json
    //     let json_str =
    //         serde_json::to_string(&batch).expect("Failed to serialize TraceEventBatch to json");

    //     // Write to new file in same dir, with newfile-<orig_filename>
    //     fs::write(new_path, &json_str).expect("Failed to write new tracebatch json file");

    //     // Optionally, assert the file exists and is not empty
    //     let metadata = fs::metadata(new_path).expect("New file should exist");
    //     assert!(metadata.len() > 0, "New file is empty");

    //     // Now write a gzipped version and time it
    //     // let start = Instant::now();

    //     // let gz_file = fs::File::create(gz_path).expect("Failed to create gz output file");
    //     // let mut encoder = flate2::write::GzEncoder::new(gz_file, flate2::Compression::default());
    //     // encoder
    //     //     .write_all(json_str.as_bytes())
    //     //     .expect("Failed to write gzipped json");
    //     // encoder
    //     //     .finish()
    //     //     .expect("Failed to finish writing gzipped json");

    //     // let duration = start.elapsed();
    //     // let gz_metadata = fs::metadata(gz_path).expect("Gzipped file should exist");

    //     // assert!(gz_metadata.len() > 0, "Gzipped file is empty");
    //     // println!(
    //     //     "Gzipped JSON tracebatch written to {} in {:?} ({} bytes)",
    //     //     gz_path,
    //     //     duration,
    //     //     gz_metadata.len()
    //     // );
    // }
}
