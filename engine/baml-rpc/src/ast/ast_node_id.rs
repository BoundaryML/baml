use serde::{Deserialize, Serialize};
use ts_rs::TS;

// Helper function to deserialize string to u64
// fn deserialize_string_to_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
// where
//     D: serde::Deserializer<'de>,
// {
//     #[derive(Deserialize)]
//     #[serde(untagged)]
//     enum StringOrNum {
//         String(String),
//         Num(u64),
//     }

//     match StringOrNum::deserialize(deserializer)? {
//         StringOrNum::String(s) => s.parse::<u64>().map_err(serde::de::Error::custom),
//         StringOrNum::Num(i) => Ok(i),
//     }
// }

// // Helper function to deserialize optional string to Option<u64>
// fn deserialize_optional_string_to_optional_u64<'de, D>(
//     deserializer: D,
// ) -> Result<Option<u64>, D::Error>
// where
//     D: serde::Deserializer<'de>,
// {
//     #[derive(Deserialize)]
//     #[serde(untagged)]
//     enum StringOrNumOrNull {
//         String(String),
//         Num(u64),
//         Null,
//     }

//     match StringOrNumOrNull::deserialize(deserializer)? {
//         StringOrNumOrNull::String(s) => {
//             s.parse::<u64>().map(Some).map_err(serde::de::Error::custom)
//         }
//         StringOrNumOrNull::Num(i) => Ok(Some(i)),
//         StringOrNumOrNull::Null => Ok(None),
//     }
// }

// // Helper function to serialize u64 to string
// fn serialize_u64_to_string<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
// where
//     S: serde::Serializer,
// {
//     serializer.serialize_str(&value.to_string())
// }

// // Helper function to serialize Option<u64> to string
// fn serialize_optional_u64_to_string<S>(
//     value: &Option<u64>,
//     serializer: S,
// ) -> Result<S::Ok, S::Error>
// where
//     S: serde::Serializer,
// {
//     match value {
//         Some(v) => serializer.serialize_str(&v.to_string()),
//         None => serializer.serialize_none(),
//     }
// }

#[derive(Debug, PartialEq, Eq, Hash, Deserialize, Serialize, Clone, TS)]
#[ts(export)]
pub struct AstNodeId {
    type_name: String,
    name: String,
    // For clickhouse compatibility, since it transforms these u64 to strings. TBD on how to make clickhouse Json support u64 but this is a quick fix.
    // #[serde(
    //     serialize_with = "serialize_u64_to_string",
    //     deserialize_with = "deserialize_string_to_u64"
    // )]
    interface_hash: u64,
    // #[serde(
    //     serialize_with = "serialize_optional_u64_to_string",
    //     deserialize_with = "deserialize_optional_string_to_optional_u64"
    // )]
    impl_hash: Option<u64>,
}

impl AstNodeId {
    pub fn interface_hash(&self) -> u64 {
        self.interface_hash
    }

    pub fn impl_hash(&self) -> Option<u64> {
        self.impl_hash
    }

    pub fn new_ast(interface_hash: u64, impl_hash: Option<u64>) -> Self {
        Self {
            type_name: "ast".to_string(),
            name: "root".to_string(),
            interface_hash,
            impl_hash,
        }
    }
    pub fn new_type_alias(name: String, interface_hash: u64, impl_hash: Option<u64>) -> Self {
        Self {
            type_name: "type_alias".to_string(),
            name,
            interface_hash,
            impl_hash,
        }
    }
    pub fn new_function(name: String, interface_hash: u64, impl_hash: Option<u64>) -> Self {
        Self {
            type_name: "function".to_string(),
            name,
            interface_hash,
            impl_hash,
        }
    }
    pub fn new_enum(name: String, interface_hash: u64, impl_hash: Option<u64>) -> Self {
        Self {
            type_name: "enum".to_string(),
            name,
            interface_hash,
            impl_hash,
        }
    }
    pub fn new_class(name: String, interface_hash: u64, impl_hash: Option<u64>) -> Self {
        Self {
            type_name: "class".to_string(),
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
            self.type_name.to_lowercase(),
            self.name,
            self.interface_hash,
            self.impl_hash.unwrap_or(0)
        )
    }
}

// TODO: not used right now for deserialization.
impl std::str::FromStr for AstNodeId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        log::info!("Parsing AstNodeId from string: {}", s);
        let parts = s.split("##").collect::<Vec<_>>();
        if parts.len() != 4 {
            return Err(anyhow::anyhow!("Invalid unique id: {}", s));
        }
        Ok(AstNodeId {
            type_name: parts[0].to_string(),
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

// TODO: not used right now for serialization
impl From<AstNodeId> for String {
    fn from(value: AstNodeId) -> Self {
        value.to_string()
    }
}

// TODO: not used right now for serialization
impl From<String> for AstNodeId {
    fn from(value: String) -> Self {
        value
            .parse()
            .expect(&format!("Failed to parse AstNodeId from string: {}", value))
    }
}
