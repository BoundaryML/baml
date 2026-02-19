use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub(super) trait BexLspCommand: Serialize + DeserializeOwned + Sized {
    const COMMAND_ID: &'static str;

    fn command_text(&self) -> String;

    fn to_lsp_command(&self) -> lsp_types::Command {
        lsp_types::Command {
            title: self.command_text(),
            command: Self::COMMAND_ID.to_string(),
            arguments: Some(vec![
                serde_json::to_value(self).expect("Failed to serde_json::to_value BexLspCommand"),
            ]),
        }
    }

    fn to_lsp_code_action(&self) -> lsp_types::CodeAction {
        lsp_types::CodeAction {
            title: self.command_text(),
            command: Some(self.to_lsp_command()),
            kind: Some(lsp_types::CodeActionKind::EMPTY),
            is_preferred: Some(false),
            diagnostics: None,
            edit: None,
            disabled: None,
            data: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct OpenBamlPanel {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_name: Option<String>,
}

impl BexLspCommand for OpenBamlPanel {
    const COMMAND_ID: &'static str = "baml.openBamlPanel";

    fn command_text(&self) -> String {
        "▶ Open 🐑 Playground".to_string()
    }
}
