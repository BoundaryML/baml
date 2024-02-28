use crate::ast::WithName;
use internal_baml_parser_database::ast::Expression;
use serde::Serialize;
use std::{collections::HashMap, path::PathBuf, process::Command};

#[derive(Debug, Serialize, Clone)]
#[serde(untagged)]
pub enum GeneratorConfigValue {
    String(String),
    Array(Vec<GeneratorConfigValue>),
    Map(HashMap<String, GeneratorConfigValue>),
}

impl From<String> for GeneratorConfigValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&Expression> for GeneratorConfigValue {
    fn from(expr: &Expression) -> Self {
        match expr {
            Expression::NumericValue(val, _) => val.clone().into(),
            Expression::StringValue(val, _) => val.clone().into(),
            Expression::Array(elements, _) => {
                Self::Array(elements.iter().map(From::from).collect())
            }
            Expression::Map(elements, _) => Self::Map(
                elements
                    .iter()
                    .map(|(k, v)| (k.to_string(), From::from(v)))
                    .collect(),
            ),
            Expression::Identifier(idn) => idn.name().to_string().into(),
            Expression::RawStringValue(val) => val.value().to_string().into(),
        }
    }
}

#[derive(Debug, Serialize, Clone)]
pub enum GeneratorLanguage {
    #[serde(rename = "python")]
    Python,
    #[serde(rename = "typescript")]
    TypeScript,
}

impl GeneratorLanguage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::TypeScript => "typescript",
        }
    }

    pub fn package_name(&self) -> &'static str {
        match self {
            Self::Python => "baml",
            Self::TypeScript => "@boundaryml/baml_client",
        }
    }

    fn client_version(&self, command: &str) -> Option<String> {
        let cmd = format!("{} {}", command, self.package_name());

        // Run the command and get the version
        Command::new(cmd)
            .output()
            .map(|output| {
                // If the command fails, return None
                if !output.status.success() {
                    return None;
                }

                // If the command succeeds, return the version
                String::from_utf8(output.stdout)
                    .ok()
                    .map(|v| v.trim().to_string())
                    .map(|v| match self {
                        Self::Python => {
                            // Split the string by lines, and take the first line that has "version" in it
                            v.lines()
                                .find(|line| {
                                    // Check if the line has "version" in it case-insensitively and has a whitespace
                                    line.to_lowercase().contains("version") && line.contains(' ')
                                })
                                .map(|line| line.split_whitespace().last().unwrap().to_string())
                        }
                        Self::TypeScript => None,
                    })
                    .flatten()
            })
            .ok()
            .flatten()
    }
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Generator {
    pub name: String,
    pub language: GeneratorLanguage,
    pub test_command: String,
    pub install_command: String,
    pub package_version_command: String,

    #[serde(skip)]
    pub used_in_tests: bool,

    // This is generated from the package_version_command
    pub client_version: Option<String>,

    /// Where the language source code is located (e.g. place where pyproject.toml is located or package.json is located)
    pub project_root: PathBuf,

    /// Where the generated code should be placed (usually a subdirectory of the project_root)
    pub output_path: PathBuf,

    #[serde(skip)]
    pub(crate) span: crate::ast::Span,
}

impl Generator {
    pub fn new(
        name: String,
        project_root: PathBuf,
        language: GeneratorLanguage,
        test_command: String,
        install_command: String,
        package_version_command: String,
        output_path: Option<PathBuf>,
        used_in_tests: Option<bool>,
        span: crate::ast::Span,
    ) -> Result<Self, std::io::Error> {
        Ok(Self {
            name,
            output_path: output_path
                .as_ref()
                .unwrap_or(&project_root)
                .join("baml_client"),
            project_root,
            client_version: language.client_version(&package_version_command),
            language,
            test_command,
            install_command,
            package_version_command,
            span,
            used_in_tests: used_in_tests.unwrap_or(false),
        })
    }

    pub fn cli_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}
