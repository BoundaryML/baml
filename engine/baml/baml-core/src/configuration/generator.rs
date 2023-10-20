use crate::{configuration::StringFromEnvVar, PreviewFeature};
use enumflags2::BitFlags;
use internal_baml_parser_database::ast::Expression;
use serde::{ser::SerializeSeq, Serialize, Serializer};
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
            Expression::ConstantValue(val, _) => val.clone().into(),
            Expression::Array(elements, _) => {
                Self::Array(elements.iter().map(From::from).collect())
            }
            Expression::Map(elements, _) => Self::Map(
                elements
                    .iter()
                    .map(|(k, v)| (k.to_string(), From::from(v)))
                    .collect(),
            ),
        }
    }
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Generator {
    pub name: String,
    pub language: String,
    pub source_path: PathBuf,
    pub output: Option<String>,
    pub config: HashMap<String, GeneratorConfigValue>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
}
