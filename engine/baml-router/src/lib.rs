use axum::{routing::{get, post}, Router, Json, http::StatusCode};
use serde::Deserialize;
use tower_service::Service;
use worker::*;

fn router() -> Router {
    Router::new()
        .route("/", get(root))
        .route("/v1/chat/completions/prompt", post(chat_completion_prompt))
}

#[event(fetch)]
async fn fetch(
    req: HttpRequest,
    _env: Env,
    _ctx: Context,
) -> Result<axum::http::Response<axum::body::Body>> {
    console_error_panic_hook::set_once();
    Ok(router().call(req).await?)
}

pub async fn root() -> &'static str {
    "Hello Axum!"
}



pub async fn chat_completion_prompt(
    Json(mut body): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Some(inner_body) = body.as_object_mut() {
        let tools = inner_body.remove("tools").map(|v| serde_json::from_value::<Vec<OpenAITool>>(v.clone()).unwrap_or_default());
        let tools = match tools {
            Some(tools) => {
                tools.into_iter().map(|tool| {
                    match tool {
                        // TODO(aaronvg): convert to type builder class
                        OpenAITool::Function(function) => function,
                    }
                }).collect::<Vec<_>>()
            },
            None => return (StatusCode::BAD_REQUEST, Json(body)),
        };

        if let Some(messages) = inner_body.get_mut("messages") {
            if let Some(messages) = messages.as_array_mut() {
                messages.push(serde_json::json!({
                    "role": "user",
                    // TODO(aaronvg): call the actual method here.
                    "content": "ctx.output_format"
                }));
            }
        }
    }

    (StatusCode::OK, Json(body))
}

#[derive(Deserialize)]
#[serde(tag = "type", content = "function", rename_all = "snake_case")]
enum OpenAITool {
    Function(OpenAIToolFunction),
}

#[derive(Deserialize)]
struct OpenAIToolFunction {
    name: String,
    description: Option<String>,
    parameters: Option<serde_json::Value>,
    strict: Option<bool>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    
    #[test]
    fn test_openai_tool() {
        let tool_str = r#"
        [
            {
                "type": "function",
                "function": {
                    "name": "get_current_weather",
                    "description": "Get the current weather in a given location",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "location": {
                                "type": "string",
                                "description": "The city and state, e.g. San Francisco, CA"
                            },
                            "unit": {"type": "string", "enum": ["celsius", "fahrenheit"]}
                        },
                        "required": ["location"]
                    }
                }   
            }   
        ]
        "#;
        let tool: Vec<OpenAITool> = serde_json::from_str(tool_str).unwrap();
        assert_eq!(tool.len(), 1);
        let tool = &tool[0];
        let function = match tool {
            OpenAITool::Function(function) => function,
        };  
        assert_eq!(function.name, "get_current_weather");
        assert_eq!(function.description, Some("Get the current weather in a given location".to_string()));
        assert_eq!(function.parameters.as_ref().unwrap(), &json!({
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "description": "The city and state, e.g. San Francisco, CA"
                },
                "unit": {"type": "string", "enum": ["celsius", "fahrenheit"]}
            },
            "required": ["location"]
        }));
    }
    
}
