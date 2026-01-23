#[cfg(test)]
mod tests {
    use crate::{apply_client, build_request};
    use bex_llm_types::{ModelFeatures, PromptAst, ResolvedClient, RoleConfig};
    use indexmap::IndexMap;
    use insta::assert_snapshot;
    use std::collections::HashMap;

    fn make_prompt_ast() -> PromptAst {
        let value = serde_json::json!({
            "type": "vec",
            "value": [
                {
                    "type": "message",
                    "value": {
                        "role": "system",
                        "content": {
                            "type": "str",
                            "value": "You are a concise assistant."
                        },
                        "metadata": {
                            "source": "snapshot-test"
                        }
                    }
                },
                {
                    "type": "message",
                    "value": {
                        "role": "user",
                        "content": {
                            "type": "str",
                            "value": "Summarize BAML in one sentence."
                        },
                        "metadata": {}
                    }
                }
            ]
        });

        serde_json::from_value(value).unwrap()
    }

    fn make_client(provider: &str, model: &str) -> ResolvedClient {
        let mut options = IndexMap::new();
        options.insert("model".to_string(), serde_json::json!(model));
        options.insert("api_key".to_string(), serde_json::json!("test-key"));

        ResolvedClient {
            name: "snapshot-client".to_string(),
            provider: provider.to_string(),
            roles: RoleConfig::default(),
            features: ModelFeatures::default(),
            options,
            request_config: Default::default(),
        }
    }

    #[test]
    fn snapshot_apply_client() {
        let mut remap = HashMap::new();
        remap.insert("system".to_string(), "user".to_string());

        let mut client = make_client("openai", "gpt-4o-mini");
        client.roles = RoleConfig {
            remap_roles: remap,
            ..Default::default()
        };

        let ast = make_prompt_ast();
        let result = apply_client(ast, &client).unwrap();
        let snap = format!("{:#?}", result);
        assert_snapshot!(snap, @"PromptAst {
    span: None,
    node: Vec(
        [
            PromptAst {
                span: None,
                node: Message {
                    role: \"user\",
                    content: PromptAst {
                        span: None,
                        node: Str(
                            \"You are a concise assistant.\",
                        ),
                    },
                    metadata: {
                        \"source\": String(\"snapshot-test\"),
                    },
                },
            },
            PromptAst {
                span: None,
                node: Message {
                    role: \"user\",
                    content: PromptAst {
                        span: None,
                        node: Str(
                            \"Summarize BAML in one sentence.\",
                        ),
                    },
                    metadata: {},
                },
            },
        ],
    ),
}");
    }

    #[test]
    fn snapshot_build_request() {
        let client = make_client("openai", "gpt-4o-mini");
        let prompt = make_prompt_ast();
        let request = build_request(&prompt, &client).unwrap();
        let snap = format!("{:#?}", request);
        assert_snapshot!(snap, @"HttpRequest {
    url: \"https://api.openai.com/v1/chat/completions\",
    method: Post,
    headers: {
        \"Authorization\": \"Bearer test-key\",
        \"Content-Type\": \"application/json\",
    },
    query_params: {},
    body: Json(
        Object {
            \"messages\": Array [
                Object {
                    \"content\": String(\"You are a concise assistant.\"),
                    \"role\": String(\"system\"),
                    \"source\": String(\"snapshot-test\"),
                },
                Object {
                    \"content\": String(\"Summarize BAML in one sentence.\"),
                    \"role\": String(\"user\"),
                },
            ],
            \"model\": String(\"gpt-4o-mini\"),
        },
    ),
}");
    }
}
