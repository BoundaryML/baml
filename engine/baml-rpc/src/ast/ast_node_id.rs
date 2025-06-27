use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(
    Debug, PartialEq, Eq, Hash, Deserialize, Serialize, Clone, TS, strum::Display, strum::EnumString,
)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
// To ensure that from_str works correctly, we need to use snake_case for the enum values
#[strum(serialize_all = "snake_case")]
pub enum AstNodeIdType {
    Ast,
    Function,
    TypeAlias,
    Enum,
    Class,
}

// u64 are serde'd as Strings in this type, because we use this type directly in
// clickhouse, and clickhouse u64 behavior is _weird_. See
// output_format_json_quote_64bit_integers,
// https://github.com/ClickHouse/ClickHouse/issues/114 and
// https://gloo-global.slack.com/archives/C085SCFUETC/p1748989355944309
// #[serde_as]
#[derive(Debug, PartialEq, Eq, Hash, Deserialize, Serialize, Clone, TS)]
#[ts(export)]
pub struct AstNodeId {
    type_name: AstNodeIdType,
    name: String,
    #[ts(type = "string")]
    #[serde(
        serialize_with = "serialize_u64_to_string",
        deserialize_with = "deserialize_string_to_u64"
    )]
    interface_hash: u64,
    #[ts(type = "string | null")]
    #[serde(
        serialize_with = "serialize_optional_u64_to_string",
        deserialize_with = "deserialize_optional_string_to_optional_u64"
    )]
    impl_hash: Option<u64>,
}

impl AstNodeId {
    pub fn interface_hash(&self) -> u64 {
        self.interface_hash
    }

    pub fn impl_hash(&self) -> Option<u64> {
        self.impl_hash
    }

    pub fn type_name(&self) -> &str {
        match self.type_name {
            AstNodeIdType::Ast => "ast",
            AstNodeIdType::Function => "function",
            AstNodeIdType::TypeAlias => "type_alias",
            AstNodeIdType::Enum => "enum",
            AstNodeIdType::Class => "class",
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn new_ast(interface_hash: u64, impl_hash: Option<u64>) -> Self {
        Self {
            type_name: AstNodeIdType::Ast,
            name: "root".to_string(),
            interface_hash,
            impl_hash,
        }
    }
    pub fn new_type_alias(name: String, interface_hash: u64, impl_hash: Option<u64>) -> Self {
        Self {
            type_name: AstNodeIdType::TypeAlias,
            name,
            interface_hash,
            impl_hash,
        }
    }
    pub fn new_function(name: String, interface_hash: u64, impl_hash: Option<u64>) -> Self {
        Self {
            type_name: AstNodeIdType::Function,
            name,
            interface_hash,
            impl_hash,
        }
    }
    pub fn new_enum(name: String, interface_hash: u64, impl_hash: Option<u64>) -> Self {
        Self {
            type_name: AstNodeIdType::Enum,
            name,
            interface_hash,
            impl_hash,
        }
    }
    pub fn new_class(name: String, interface_hash: u64, impl_hash: Option<u64>) -> Self {
        Self {
            type_name: AstNodeIdType::Class,
            name,
            interface_hash,
            impl_hash,
        }
    }
}

impl std::fmt::Display for AstNodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}##{}##{}##{}",
            self.type_name,
            self.name,
            self.interface_hash,
            self.impl_hash.unwrap_or(0)
        )
    }
}

impl std::str::FromStr for AstNodeId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts = s.split("##").collect::<Vec<_>>();

        if parts.len() != 4 {
            return Err(anyhow::anyhow!("Invalid unique id: {}", s));
        }
        Ok(AstNodeId {
            type_name: parts[0].parse::<AstNodeIdType>()?,
            name: parts[1].to_string(),
            interface_hash: match parts[2].parse() {
                Ok(interface_hash) => interface_hash,
                Err(_) => return Err(anyhow::anyhow!("Invalid unique id: {}", s)),
            },
            impl_hash: match parts[3].parse() {
                Ok(0) => None,
                Ok(impl_hash) => Some(impl_hash),
                Err(_) => return Err(anyhow::anyhow!("Invalid unique id: {}", s)),
            },
        })
    }
}

// Helper function to deserialize string to u64
fn deserialize_string_to_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNum {
        String(String),
        Num(u64),
    }

    match StringOrNum::deserialize(deserializer)? {
        StringOrNum::String(s) => s.parse::<u64>().map_err(serde::de::Error::custom),
        StringOrNum::Num(i) => Ok(i),
    }
}

// Helper function to deserialize optional string to Option<u64>
fn deserialize_optional_string_to_optional_u64<'de, D>(
    deserializer: D,
) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumOrNull {
        String(String),
        Num(u64),
        Null,
    }

    match StringOrNumOrNull::deserialize(deserializer)? {
        StringOrNumOrNull::String(s) => {
            s.parse::<u64>().map(Some).map_err(serde::de::Error::custom)
        }
        StringOrNumOrNull::Num(i) => Ok(Some(i)),
        StringOrNumOrNull::Null => Ok(None),
    }
}

// Helper function to serialize u64 to string
fn serialize_u64_to_string<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&value.to_string())
}

// Helper function to serialize Option<u64> to string
fn serialize_optional_u64_to_string<S>(
    value: &Option<u64>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match value {
        Some(v) => serializer.serialize_str(&v.to_string()),
        None => serializer.serialize_none(),
    }
}
