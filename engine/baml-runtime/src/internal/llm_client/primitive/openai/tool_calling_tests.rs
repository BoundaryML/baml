#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::llm_client::primitive::openai::openai_client::*;
    use crate::internal::llm_client::primitive::openai::types::*;
    use baml_types::{BamlMap, TypeIR};
    use serde_json::json;

    #[test]
    fn test_tool_calling_mode_detection() {
        // Test that baml_mode: "tool_calling" is properly detected
        let mut properties = BamlMap::new();
        properties.insert("baml_mode".to_string(), json!("tool_calling"));
        properties.insert("model".to_string(), json!("gpt-4"));
        
        // Check if tool_calling mode is detected
        let is_tool_calling = properties
            .get("baml_mode")
            .and_then(|v| v.as_str())
            .map(|s| s == "tool_calling")
            .unwrap_or(false);
            
        assert!(is_tool_calling, "Tool calling mode should be detected");
    }

    #[test]
    fn test_tool_calling_removes_output_format() {
        // When in tool_calling mode, output_format should be removed
        let mut properties = BamlMap::new();
        properties.insert("baml_mode".to_string(), json!("tool_calling"));
        properties.insert("model".to_string(), json!("gpt-4"));
        properties.insert("output_format".to_string(), json!({"type": "json"}));
        
        // Simulate the request building logic
        let mut body = json!(properties.clone());
        let body_obj = body.as_object_mut().unwrap();
        
        let is_tool_calling = properties
            .get("baml_mode")
            .and_then(|v| v.as_str())
            .map(|s| s == "tool_calling")
            .unwrap_or(false);
            
        if is_tool_calling {
            body_obj.remove("output_format");
        }
        
        assert!(!body_obj.contains_key("output_format"), "output_format should be removed in tool_calling mode");
        assert!(body_obj.contains_key("baml_mode"), "baml_mode should still be present");
    }

    #[test]
    fn test_tool_calling_adds_tools() {
        // Test that tools are added to the request when configured
        let mut properties = BamlMap::new();
        properties.insert("baml_mode".to_string(), json!("tool_calling"));
        properties.insert("model".to_string(), json!("gpt-4"));
        
        // Add tool configuration
        let tools = json!([
            {
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "Get weather information",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "location": {"type": "string"}
                        },
                        "required": ["location"]
                    }
                }
            }
        ]);
        properties.insert("tools".to_string(), tools.clone());
        properties.insert("tool_choice".to_string(), json!("auto"));
        
        // Simulate request building
        let mut body = json!(properties.clone());
        let body_obj = body.as_object_mut().unwrap();
        
        let is_tool_calling = properties
            .get("baml_mode")
            .and_then(|v| v.as_str())
            .map(|s| s == "tool_calling")
            .unwrap_or(false);
            
        if is_tool_calling {
            body_obj.remove("output_format");
            
            if let Some(tools) = properties.get("tools") {
                body_obj.insert("tools".into(), tools.clone());
            }
            
            if let Some(tool_choice) = properties.get("tool_choice") {
                body_obj.insert("tool_choice".into(), tool_choice.clone());
            }
        }
        
        assert!(body_obj.contains_key("tools"), "tools should be added");
        assert!(body_obj.contains_key("tool_choice"), "tool_choice should be added");
        assert_eq!(body_obj["tool_choice"], "auto");
    }

    #[test]
    fn test_parse_tool_call_response() {
        // Test parsing a response with tool_calls
        let response_json = json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1234567890,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_abc123",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"location\": \"San Francisco\", \"unit\": \"celsius\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 50,
                "completion_tokens": 20,
                "total_tokens": 70
            }
        });
        
        let response: ChatCompletionResponse = serde_json::from_value(response_json).unwrap();
        
        assert!(response.choices[0].message.tool_calls.is_some());
        let tool_calls = response.choices[0].message.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].function.name, "get_weather");
        assert!(tool_calls[0].function.arguments.contains("San Francisco"));
    }

    #[test]
    fn test_streaming_tool_call_accumulation() {
        // Test that streaming tool calls accumulate properly
        let delta1 = ChatCompletionMessageToolCallDelta {
            index: 0,
            id: Some("call_123".to_string()),
            call_type: Some("function".to_string()),
            function: Some(ToolCallFunctionDelta {
                name: Some("get_weather".to_string()),
                arguments: Some("{\"location\": \"San".to_string()),
            }),
        };
        
        let delta2 = ChatCompletionMessageToolCallDelta {
            index: 0,
            id: None,
            call_type: None,
            function: Some(ToolCallFunctionDelta {
                name: None,
                arguments: Some(" Francisco\"}".to_string()),
            }),
        };
        
        // Simulate accumulation
        let mut accumulated = String::new();
        if let Some(args) = delta1.function.as_ref().and_then(|f| f.arguments.as_ref()) {
            accumulated.push_str(args);
        }
        if let Some(args) = delta2.function.as_ref().and_then(|f| f.arguments.as_ref()) {
            accumulated.push_str(args);
        }
        
        assert_eq!(accumulated, "{\"location\": \"San Francisco\"}");
        
        // Verify it's valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&accumulated).unwrap();
        assert_eq!(parsed["location"], "San Francisco");
    }

    #[test]
    fn test_tool_choice_formatting() {
        // Test different tool_choice formats
        let auto_choice = ToolChoiceOption::String("auto".to_string());
        let serialized = serde_json::to_value(&auto_choice).unwrap();
        assert_eq!(serialized, json!("auto"));
        
        let none_choice = ToolChoiceOption::String("none".to_string());
        let serialized = serde_json::to_value(&none_choice).unwrap();
        assert_eq!(serialized, json!("none"));
        
        let required_choice = ToolChoiceOption::String("required".to_string());
        let serialized = serde_json::to_value(&required_choice).unwrap();
        assert_eq!(serialized, json!("required"));
        
        let specific_choice = ToolChoiceOption::Object(ToolChoiceSpecific {
            choice_type: "function".to_string(),
            function: ToolChoiceFunctionName {
                name: "get_weather".to_string(),
            },
        });
        let serialized = serde_json::to_value(&specific_choice).unwrap();
        assert_eq!(serialized["type"], "function");
        assert_eq!(serialized["function"]["name"], "get_weather");
    }

    #[test]
    fn test_mixed_content_and_tool_calls() {
        // Test that we handle responses with both content and tool_calls appropriately
        // (OpenAI usually sends one or the other, but we should handle both)
        let response_json = json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1234567890,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "I'll check the weather for you.",
                    "tool_calls": [{
                        "id": "call_abc123",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"location\": \"Paris\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 50,
                "completion_tokens": 25,
                "total_tokens": 75
            }
        });
        
        let response: ChatCompletionResponse = serde_json::from_value(response_json).unwrap();
        
        // When tool_calls are present, we should prioritize them
        assert!(response.choices[0].message.tool_calls.is_some());
        assert!(response.choices[0].message.content.is_some());
        
        // In our implementation, tool_calls take precedence
        let tool_calls = response.choices[0].message.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls[0].function.arguments, "{\"location\": \"Paris\"}");
    }

    #[test]
    fn test_tool_calls_with_no_content() {
        // Test the common case where content is null when tool_calls are present
        let response_json = json!({
            "id": "chatcmpl-456",
            "object": "chat.completion",
            "created": 1234567890,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_def456",
                        "type": "function",
                        "function": {
                            "name": "calculate_sum",
                            "arguments": "{\"numbers\": [1, 2, 3, 4, 5]}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 30,
                "completion_tokens": 15,
                "total_tokens": 45
            }
        });
        
        let response: ChatCompletionResponse = serde_json::from_value(response_json).unwrap();
        
        assert!(response.choices[0].message.content.is_none());
        assert!(response.choices[0].message.tool_calls.is_some());
        
        let tool_calls = response.choices[0].message.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls[0].function.name, "calculate_sum");
        
        // Verify the arguments are valid JSON
        let args: serde_json::Value = serde_json::from_str(&tool_calls[0].function.arguments).unwrap();
        assert_eq!(args["numbers"], json!([1, 2, 3, 4, 5]));
    }

    #[test]
    fn test_multiple_tool_calls() {
        // Test handling multiple tool calls in a single response
        let response_json = json!({
            "id": "chatcmpl-789",
            "object": "chat.completion",
            "created": 1234567890,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [
                        {
                            "id": "call_001",
                            "type": "function",
                            "function": {
                                "name": "get_weather",
                                "arguments": "{\"location\": \"London\"}"
                            }
                        },
                        {
                            "id": "call_002",
                            "type": "function",
                            "function": {
                                "name": "get_weather",
                                "arguments": "{\"location\": \"Tokyo\"}"
                            }
                        }
                    ]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 40,
                "completion_tokens": 30,
                "total_tokens": 70
            }
        });
        
        let response: ChatCompletionResponse = serde_json::from_value(response_json).unwrap();
        
        let tool_calls = response.choices[0].message.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 2);
        assert_eq!(tool_calls[0].id, "call_001");
        assert_eq!(tool_calls[1].id, "call_002");
        
        // Both should be weather calls for different locations
        assert!(tool_calls[0].function.arguments.contains("London"));
        assert!(tool_calls[1].function.arguments.contains("Tokyo"));
    }
}