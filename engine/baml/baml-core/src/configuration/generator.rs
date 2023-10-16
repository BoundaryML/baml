use crate::{configuration::StringFromEnvVar, PreviewFeature};
use enumflags2::BitFlags;
use internal_baml_parser_database::ast::Expression;
use serde::{ser::SerializeSeq, Serialize, Serializer};
use std::collections::HashMap;

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
    pub provider: StringFromEnvVar,
    pub output: Option<StringFromEnvVar>,
    pub config: HashMap<String, GeneratorConfigValue>,

    #[serde(default)]
    pub binary_targets: Vec<StringFromEnvVar>,

    #[serde(default, serialize_with = "mcf_preview_features")]
    pub preview_features: Option<BitFlags<PreviewFeature>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
}

pub fn mcf_preview_features<S>(
    feats: &Option<BitFlags<PreviewFeature>>,
    s: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let feats = feats.unwrap_or_default();
    let mut seq = s.serialize_seq(Some(feats.len()))?;
    for feat in feats.iter() {
        seq.serialize_element(&feat)?;
    }
    seq.end()
}
