#[cfg(test)]
mod tests {
    use crate::{specialize_prompt, build_request};
    use bex_llm_types::{ModelFeatures, ResolvedClient, RoleConfig};
    use bex_vm_types::{PromptAst, Value};
    use indexmap::IndexMap;
    use insta::assert_snapshot;
    use std::collections::HashMap;

    fn make_prompt_ast() -> PromptAst {
        PromptAst::Vec(vec![
            PromptAst::Message {
                role: "system".to_string(),
                content: Box::new(PromptAst::String("You are a concise assistant.".to_string())),
                metadata: Value::Null,
            },
            PromptAst::Message {
                role: "user".to_string(),
                content: Box::new(PromptAst::String("Summarize BAML in one sentence.".to_string())),
                metadata: Value::Null,
            },
        ])
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
    fn snapshot_specialize_prompt() {
        let mut remap = HashMap::new();
        remap.insert("system".to_string(), "user".to_string());

        let mut client = make_client("openai", "gpt-4o-mini");
        client.roles = RoleConfig {
            remap_roles: remap,
            ..Default::default()
        };

        let ast = make_prompt_ast();
        let result = specialize_prompt(ast, &client).unwrap();
        let snap = format!("{:#?}", result);
        assert_snapshot!(snap, @r###"
        Vec(
            [
                Message {
                    role: "user",
                    content: String(
                        "You are a concise assistant.",
                    ),
                    metadata: Null,
                },
                Message {
                    role: "user",
                    content: String(
                        "Summarize BAML in one sentence.",
                    ),
                    metadata: Null,
                },
            ],
        )
        "###);
    }

    #[test]
    fn snapshot_build_request() {
        let client = make_client("openai", "gpt-4o-mini");
        let prompt = make_prompt_ast();
        let request = build_request(&prompt, &client).unwrap();
        let snap = format!("{:#?}", request);
        assert_snapshot!(snap, @r###"
        HttpRequest {
            url: "https://api.openai.com/v1/chat/completions",
            method: Post,
            headers: {
                "Authorization": "Bearer test-key",
                "Content-Type": "application/json",
            },
            query_params: {},
            body: Json(
                Object {
                    "messages": Array [
                        Object {
                            "content": String("You are a concise assistant."),
                            "role": String("system"),
                        },
                        Object {
                            "content": String("Summarize BAML in one sentence."),
                            "role": String("user"),
                        },
                    ],
                    "model": String("gpt-4o-mini"),
                },
            ),
        }
        "###);
    }
}
