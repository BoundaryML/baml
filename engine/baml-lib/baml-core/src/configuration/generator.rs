use crate::ast::WithName;
use internal_baml_parser_database::ast::Expression;
use serde::Serialize;
use std::{collections::HashMap, path::PathBuf};

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
#[serde(rename_all = "camelCase")]
pub struct Generator {
    pub name: String,
    pub language: String,
    pub pkg_manager: Option<String>,
    pub source_path: PathBuf,
    pub output: PathBuf,
    pub config: HashMap<String, GeneratorConfigValue>,
    pub client_version: Option<String>,
    pub shell_setup: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,

    #[serde(skip)]
    pub(crate) span: Option<crate::ast::Span>,
}

impl Generator {
    pub fn client_version(&self) -> Option<&str> {
        self.client_version.as_deref()
    }

    pub fn cli_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}
