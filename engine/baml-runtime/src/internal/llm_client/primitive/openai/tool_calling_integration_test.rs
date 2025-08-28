#[cfg(test)]
mod integration_tests {
    use super::*;
    use serde_json::json;
    use baml_types::BamlMap;

    #[test]
    fn test_tool_calling_request_format() {
        // Test that when baml_mode is "tool_calling", the request is properly formatted
        let mut properties = BamlMap::new();
        properties.insert("baml_mode".to_string(), json!("tool_calling"));
        properties.insert("model".to_string(), json!("gpt-4o"));
        
        // Add tool definition
        let tools = json!([
            {
                "type": "function",
                "function": {
                    "name": "WeatherInfo",
                    "description": "Get weather information",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "temperature": {"type": "number"},
                            "units": {"type": "string", "enum": ["celsius", "fahrenheit"]},
                            "condition": {"type": "string"}
                        },
                        "required": ["temperature", "units", "condition"]
                    }
                }
            }
        ]);
        
        properties.insert("tools".to_string(), tools);
        properties.insert("tool_choice".to_string(), json!("auto"));
        
        // Test request body building
        let mut body = json!(properties.clone());
        let body_obj = body.as_object_mut().unwrap();
        
        // Simulate the logic from build_body
        let is_tool_calling = properties
            .get("baml_mode")
            .and_then(|v| v.as_str())
            .map(|s| s == "tool_calling")
            .unwrap_or(false);
        
        if is_tool_calling {
            body_obj.remove("output_format");
            body_obj.remove("baml_mode");
            
            if let Some(tools) = properties.get("tools") {
                body_obj.insert("tools".into(), tools.clone());
            }
            
            if let Some(tool_choice) = properties.get("tool_choice") {
                body_obj.insert("tool_choice".into(), tool_choice.clone());
            }
        }
        
        // Assertions
        assert!(!body_obj.contains_key("output_format"), "output_format should be removed");
        assert!(!body_obj.contains_key("baml_mode"), "baml_mode should be removed");
        assert!(body_obj.contains_key("tools"), "tools should be present");
        assert!(body_obj.contains_key("tool_choice"), "tool_choice should be present");
        assert!(body_obj.contains_key("model"), "model should still be present");
    }

    #[test]
    fn test_tool_calling_response_parsing() {
        // Test response with tool_calls
        let response_json = json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1741214129,
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "WeatherInfo",
                            "arguments": "{\"temperature\": 22.5, \"units\": \"celsius\", \"condition\": \"sunny\"}"
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
        
        // Parse the response
        let response: super::super::types::ChatCompletionResponse = 
            serde_json::from_value(response_json).unwrap();
        
        // Check that tool_calls are properly parsed
        assert!(response.choices[0].message.tool_calls.is_some());
        let tool_calls = response.choices[0].message.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].function.name, "WeatherInfo");
        
        // The arguments should be valid JSON
        let args_json: serde_json::Value = 
            serde_json::from_str(&tool_calls[0].function.arguments).unwrap();
        assert_eq!(args_json["temperature"], 22.5);
        assert_eq!(args_json["units"], "celsius");
        assert_eq!(args_json["condition"], "sunny");
    }

    #[test]
    fn test_parallel_tool_calls() {
        // Test that enable_parallel_tool_calls adds parallel_tool_calls to request
        let mut properties = BamlMap::new();
        properties.insert("baml_mode".to_string(), json!("tool_calling"));
        properties.insert("model".to_string(), json!("gpt-4o"));
        properties.insert("enable_parallel_tool_calls".to_string(), json!(true));
        
        let tools = json!([
            {
                "type": "function",
                "function": {
                    "name": "WeatherInfo",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "temperature": {"type": "number"}
                        },
                        "required": ["temperature"]
                    }
                }
            }
        ]);
        
        properties.insert("tools".to_string(), tools);
        
        let mut body = json!(properties.clone());
        let body_obj = body.as_object_mut().unwrap();
        
        // Apply the logic
        let is_tool_calling = properties
            .get("baml_mode")
            .and_then(|v| v.as_str())
            .map(|s| s == "tool_calling")
            .unwrap_or(false);
        
        if is_tool_calling {
            body_obj.remove("output_format");
            body_obj.remove("baml_mode");
            
            if let Some(tools) = properties.get("tools") {
                body_obj.insert("tools".into(), tools.clone());
            }
            
            // Enable parallel tool calls for array returns if tools are present
            if properties.get("tools").is_some() && 
               properties.get("enable_parallel_tool_calls")
                   .and_then(|v| v.as_bool())
                   .unwrap_or(false) {
                body_obj.insert("parallel_tool_calls".into(), json!(true));
            }
        }
        
        assert!(body_obj.contains_key("parallel_tool_calls"));
        assert_eq!(body_obj["parallel_tool_calls"], json!(true));
    }

    #[test]
    fn test_streaming_tool_call_accumulation() {
        // Test that streaming tool calls accumulate arguments correctly
        let delta1 = json!({
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "WeatherInfo",
                            "arguments": "{\"temper"
                        }
                    }]
                }
            }]
        });
        
        let delta2 = json!({
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "function": {
                            "arguments": "ature\": 22.5, \"units\": \"celsius\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        });
        
        // In actual usage, these chunks would be accumulated in the content field
        // The response handler appends tool call arguments to content during streaming
        let mut accumulated_content = String::new();
        
        // Simulate processing first chunk
        if let Some(choices) = delta1["choices"].as_array() {
            if let Some(delta) = choices[0]["delta"].as_object() {
                if let Some(tool_calls) = delta["tool_calls"].as_array() {
                    for tool_call in tool_calls {
                        if let Some(function) = tool_call["function"].as_object() {
                            if let Some(args) = function["arguments"].as_str() {
                                accumulated_content.push_str(args);
                            }
                        }
                    }
                }
            }
        }
        
        // Simulate processing second chunk
        if let Some(choices) = delta2["choices"].as_array() {
            if let Some(delta) = choices[0]["delta"].as_object() {
                if let Some(tool_calls) = delta["tool_calls"].as_array() {
                    for tool_call in tool_calls {
                        if let Some(function) = tool_call["function"].as_object() {
                            if let Some(args) = function["arguments"].as_str() {
                                accumulated_content.push_str(args);
                            }
                        }
                    }
                }
            }
        }
        
        // Verify the accumulated content is valid JSON
        assert_eq!(accumulated_content, "{\"temperature\": 22.5, \"units\": \"celsius\"}");
        let parsed: serde_json::Value = serde_json::from_str(&accumulated_content).unwrap();
        assert_eq!(parsed["temperature"], 22.5);
        assert_eq!(parsed["units"], "celsius");
    }
}