use serde::{
    de::{DeserializeOwned, Error},
    Deserialize, Serialize,
};

/// Commands that can be triggered by a code lens. Only used in Jetbrains.
pub trait CodeLensCommand: Serialize + DeserializeOwned + Sized {
    const COMMAND_ID: &'static str;

    fn code_lens_text(&self) -> String;

    fn to_lsp_command(&self) -> Option<lsp_types::Command> {
        Some(lsp_types::Command {
            title: self.code_lens_text(),
            command: Self::COMMAND_ID.to_string(),
            arguments: Some(vec![
                serde_json::to_value(self).expect("Failed to serde_json::to_value CodeLensCommand")
            ]),
        })
    }

    fn from_execute_command_params(
        mut params: lsp_types::ExecuteCommandParams,
    ) -> Result<Self, serde_json::Error> {
        let Some(args): Option<serde_json::Value> = params.arguments.pop() else {
            return Err(serde_json::Error::custom(format!(
                "Expected one argument for command {}",
                params.command
            )));
        };
        let command: Self = serde_json::from_value(args)?;
        Ok(command)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenBamlPanel {
    pub project_id: String,
    pub function_name: String,
    pub show_tests: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunBamlTest {
    pub project_id: String,
    pub test_case_name: String,
    pub function_name: String,
    pub show_tests: bool,
}

impl CodeLensCommand for OpenBamlPanel {
    const COMMAND_ID: &'static str = "baml.openBamlPanel";

    fn code_lens_text(&self) -> String {
        "▶ Open BAML Playground 💥".to_string()
    }
}

impl CodeLensCommand for RunBamlTest {
    const COMMAND_ID: &'static str = "baml.runBamlTest";

    fn code_lens_text(&self) -> String {
        format!("▶ Test {} 💥", self.function_name)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum RegisteredCommands {
    OpenBamlPanel(OpenBamlPanel),
    RunTest(RunBamlTest),
}

impl RegisteredCommands {
    pub fn from_execute_command(
        mut params: lsp_types::ExecuteCommandParams,
    ) -> Result<Self, serde_json::Error> {
        let command = params.command;
        let Some(args): Option<serde_json::Value> = params.arguments.pop() else {
            return Err(serde_json::Error::custom(format!(
                "Expected one argument for command {command}",
            )));
        };
        match command.as_str() {
            OpenBamlPanel::COMMAND_ID => {
                let command: OpenBamlPanel = serde_json::from_value(args)?;
                Ok(RegisteredCommands::OpenBamlPanel(command))
            }
            RunBamlTest::COMMAND_ID => {
                let command: RunBamlTest = serde_json::from_value(args)?;
                Ok(RegisteredCommands::RunTest(command))
            }
            _ => Err(serde_json::Error::custom(format!(
                "Unknown command: {command}"
            ))),
        }
    }
}
