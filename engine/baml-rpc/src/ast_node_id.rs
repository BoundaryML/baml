use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(into = "String", from = "String")]
pub struct AstNodeId {
    pub type_name: String,
    pub name: String,
    pub interface_hash: Option<u64>,
    pub impl_hash: Option<u64>,
}

impl std::fmt::Display for AstNodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}##{}##{}##{}",
            self.type_name,
            self.name,
            self.interface_hash.unwrap_or(0),
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
            interface_hash: parts[2].parse().ok(),
            impl_hash: parts[3].parse().ok(),
        })
    }
}

impl From<AstNodeId> for String {
    fn from(value: AstNodeId) -> Self {
        value.to_string()
    }
}

impl From<String> for AstNodeId {
    fn from(value: String) -> Self {
        value
            .parse()
            .expect("Failed to parse AstNodeId from string")
    }
}
