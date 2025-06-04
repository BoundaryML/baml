use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use ts_rs::TS;

// u64 are serde'd as Strings in this type, because we use this type directly in
// clickhouse, and clickhouse u64 behavior is _weird_. See
// output_format_json_quote_64bit_integers,
// https://github.com/ClickHouse/ClickHouse/issues/114 and
// https://gloo-global.slack.com/archives/C085SCFUETC/p1748989355944309
#[serde_as]
#[derive(Debug, PartialEq, Eq, Hash, Deserialize, Serialize, Clone, TS)]
#[ts(export)]
pub struct AstNodeId {
    type_name: String,
    name: String,
    #[ts(type = "string")]
    #[serde_as(as = "DisplayFromStr")]
    interface_hash: u64,
    #[ts(type = "string")]
    #[serde_as(as = "Option<DisplayFromStr>")]
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
        &self.type_name
    }

    pub fn name(&self) -> &str {
        &self.name
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

impl std::str::FromStr for AstNodeId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
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
