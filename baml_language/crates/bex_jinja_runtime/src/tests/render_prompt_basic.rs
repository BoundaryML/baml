use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

use crate::{render_prompt, LlmClientSpec, OutputFormatContent, PromptAst, PromptAstNode, RenderContext};
use bex_vm_types::{MediaContent, MediaValue};
use ir_stub::{BamlMap, BamlValue};

#[derive(Deserialize)]
struct TestCase {
    template: String,
    args: JsonValue,
    ctx: TestContext,
}

#[derive(Deserialize)]
struct TestContext {
    client: LlmClientSpec,
    tags: HashMap<String, JsonValue>,
    output_format: OutputFormatContent,
}

#[test]
fn render_prompt_basic_chat() {
    let tc: TestCase = serde_json::from_value(json!({
        "template": r#"
            {{ _.chat('user') }}
            Hello {{ name }}
        "#,
        "args": { "name": "Sam" },
        "ctx": {
            "client": {
                "name": "test",
                "provider": "openai",
                "default_role": "user",
                "allowed_roles": ["system", "user", "assistant"],
                "remap_role": {},
                "options": { "model": "gpt-4" }
            },
            "tags": {},
            "output_format": {
                "enums": {},
                "classes": {}
            }
        }
    }))
    .expect("failed to parse test case JSON");

    let args = json_to_baml_value(&tc.args);
    let tags = tc
        .ctx
        .tags
        .into_iter()
        .map(|(k, v)| (k, json_to_baml_value(&v)))
        .collect();

    let ctx = RenderContext {
        client: tc.ctx.client,
        tags,
        output_format: OutputFormatContent::test_only_defaults(tc.ctx.output_format),
    };

    let rendered = render_prompt(&tc.template, &args, ctx).expect("render_prompt failed");
    let expected = json!({
        "type": "vec",
        "value": [
            {
                "type": "message",
                "value": {
                    "role": "user",
                    "content": {
                        "type": "str",
                        "value": "Hello Sam"
                    },
                    "metadata": {}
                }
            }
        ]
    });

    assert_eq!(prompt_ast_to_json(&rendered), expected);
}

fn json_to_baml_value(value: &JsonValue) -> BamlValue {
    match value {
        JsonValue::Null => BamlValue::Null,
        JsonValue::Bool(b) => BamlValue::Bool(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                BamlValue::Int(i)
            } else if let Some(f) = n.as_f64() {
                BamlValue::Float(f)
            } else {
                BamlValue::String(n.to_string())
            }
        }
        JsonValue::String(s) => BamlValue::String(s.clone()),
        JsonValue::Array(arr) => {
            BamlValue::List(arr.iter().map(json_to_baml_value).collect())
        }
        JsonValue::Object(obj) => {
            let map = obj.iter().map(|(k, v)| (k.clone(), json_to_baml_value(v))).collect();
            BamlValue::Map(map)
        }
    }
}

fn prompt_ast_to_json(ast: &PromptAst) -> JsonValue {
    prompt_ast_node_to_json(&ast.node)
}

fn prompt_ast_node_to_json(node: &PromptAstNode) -> JsonValue {
    match node {
        PromptAstNode::Str(value) => json!({
            "type": "str",
            "value": value,
        }),
        PromptAstNode::Vec(items) => json!({
            "type": "vec",
            "value": items.iter().map(prompt_ast_to_json).collect::<Vec<_>>(),
        }),
        PromptAstNode::Message {
            role,
            content,
            metadata,
        } => json!({
            "type": "message",
            "value": {
                "role": role,
                "content": prompt_ast_to_json(content),
                "metadata": JsonValue::Object(metadata.clone()),
            }
        }),
        PromptAstNode::Media(media) => json!({
            "type": "media",
            "value": media_to_json(media),
        }),
    }
}

fn media_to_json(media: &MediaValue) -> JsonValue {
    let content = match &media.content {
        MediaContent::Url { url, base64_data } => json!({
            "type": "url",
            "url": url,
            "base64_data": base64_data,
        }),
        MediaContent::Base64 { base64_data } => json!({
            "type": "base64",
            "base64_data": base64_data,
        }),
        MediaContent::File { file, base64_data } => json!({
            "type": "file",
            "file": file,
            "base64_data": base64_data,
        }),
    };

    json!({
        "kind": media.kind.to_string(),
        "content": content,
        "mime_type": media.mime_type,
    })
}
