use std::path::PathBuf;

use crate::{errors::CliError, init_command::interact::get_multi_selection_or_default};

use super::traits::{ToBamlSrc, WithLoader, Writer};

pub(super) struct ClientConfig<T: AsRef<str>> {
    pub comment: Option<String>,
    pub name: T,
    pub provider: T,
    pub params: Vec<(&'static str, &'static str)>,
}

impl<T: AsRef<str>> ToBamlSrc for ClientConfig<T> {
    fn to_baml(&self) -> String {
        format!(
            r#"
{}
client<llm> {} {{
  provider {}
  options {{
{}
  }}
}}
        "#,
            self.comment
                .as_ref()
                .map(|c|
                    // Prefix each line of the comment with a `//`
                    c.trim().lines()
                        .map(|l| format!("// {}", l.trim()))
                        .collect::<Vec<String>>()
                        .join("\n"))
                .unwrap_or_default(),
            self.name.as_ref(),
            self.provider.as_ref(),
            self.params
                .iter()
                .map(|(k, v)| format!("    {} {}", k, v))
                .collect::<Vec<String>>()
                .join("\n")
        )
        .trim()
        .into()
    }
}

impl<T: AsRef<str> + From<&'static str>> WithLoader<Vec<ClientConfig<T>>> for ClientConfig<T> {
    fn from_dialoguer(
        no_prompt: bool,
        _: &PathBuf,
        _writer: &mut Writer,
    ) -> Result<Vec<ClientConfig<T>>, CliError> {
        const CLIENT_PROVIDERS: [&str; 3] = ["OpenAI", "OpenAI (Azure)", "Anthropic"];

        let providers = get_multi_selection_or_default(
            "What llm provider do you want to use?",
            &CLIENT_PROVIDERS,
            &[true],
            no_prompt,
        )?;

        Ok(providers
            .iter()
            .flat_map(|&provider| match provider {
                0 => openai_clients(),
                1 => openai_azure_clients(),
                2 => anthropic_clients(),
                _ => unreachable!("Invalid provider selection"),
            })
            .collect())
    }
}

fn openai_clients<T: From<&'static str> + AsRef<str>>() -> Vec<ClientConfig<T>> {
    vec![
        ClientConfig {
            comment: None,
            provider: "baml-openai-chat".into(),
            name: "GPT4".into(),
            params: vec![("model", "gpt-4"), ("api_key", "env.OPENAI_API_KEY")],
        },
        ClientConfig {
            comment: None,
            provider: "baml-openai-chat".into(),
            name: "GPT4Turbo".into(),
            params: vec![
                ("model", "gpt-4-1106-preview"),
                ("api_key", "env.OPENAI_API_KEY"),
            ],
        },
        ClientConfig {
            comment: None,
            provider: "baml-openai-chat".into(),
            name: "GPT3".into(),
            params: vec![
                ("model", "gpt-3.5-turbo"),
                ("api_key", "env.OPENAI_API_KEY"),
            ],
        },
    ]
}

fn openai_azure_clients<T: From<&'static str> + AsRef<str>>() -> Vec<ClientConfig<T>> {
    vec![ClientConfig {
        comment: None,
        provider: "baml-azure-chat".into(),
        name: "Azure".into(),
        params: vec![
            ("model", "env.AZURE_OPENAI_DEPLOYMENT_NAME"),
            ("api_key", "env.AZURE_OPENAI_API_KEY"),
            ("azure_endpoint", "env.AZURE_OPENAI_ENDPOINT"),
        ],
    }]
}

fn anthropic_clients<T: From<&'static str> + AsRef<str>>() -> Vec<ClientConfig<T>> {
    vec![
        ClientConfig {
            comment: None,
            provider: "baml-anthropic-chat".into(),
            name: "Claude".into(),
            params: vec![
                ("model_name", "claude-2.1"),
                ("api_key", "env.ANTHROPIC_API_KEY"),
            ],
        },
        ClientConfig {
            comment: None,
            provider: "baml-anthropic-chat".into(),
            name: "ClaudeInstant".into(),
            params: vec![
                ("model_name", "claude-instant-1.2"),
                ("api_key", "env.ANTHROPIC_API_KEY"),
            ],
        },
    ]
}

fn google_clients<T: From<&'static str> + AsRef<str>>() -> Vec<ClientConfig<T>> {
    vec![ClientConfig {
        comment: None,
        provider: "google-ai".into(),
        name: "GoogleAI".into(),
        params: vec![("model_name", "gemini"), ("api_key", "env.GOOGLE_API_KEY")],
    }]
}

fn vertex_clients<T: From<&'static str> + AsRef<str>>() -> Vec<ClientConfig<T>> {
    vec![ClientConfig {
        comment: None,
        provider: "vertex-ai".into(),
        name: "Vertex".into(),
        params: vec![("model_name", "vertex"), ("api_key", "env.GOOGLE_API_KEY")],
    }]
}
