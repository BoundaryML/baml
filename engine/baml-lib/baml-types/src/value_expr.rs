use anyhow::Result;
use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
};

use crate::JinjaExpression;
use indexmap::{IndexMap, IndexSet};
use secrecy::{ExposeSecret, SecretString};

#[derive(Debug)]
pub enum Resolvable<Id, Meta> {
    // Enums go into here.
    String(Id, Meta),
    // Repred as a string, but guaranteed to be a number.
    Numeric(String, Meta),
    Bool(bool, Meta),
    Array(Vec<Resolvable<Id, Meta>>, Meta),
    // This includes key-value pairs for classes
    Map(IndexMap<String, (Meta, Resolvable<Id, Meta>)>, Meta),
    Null(Meta),
}

impl<Id, Meta> Resolvable<Id, Meta> {
    pub fn into_str(self) -> Result<(Id, Meta), Resolvable<Id, Meta>> {
        match self {
            Self::String(s, meta) => Ok((s, meta)),
            other => Err(other),
        }
    }

    pub fn into_array(self) -> Result<(Vec<Resolvable<Id, Meta>>, Meta), Resolvable<Id, Meta>> {
        match self {
            Self::Array(a, meta) => Ok((a, meta)),
            other => Err(other),
        }
    }

    pub fn into_map(
        self,
    ) -> Result<(IndexMap<String, (Meta, Resolvable<Id, Meta>)>, Meta), Resolvable<Id, Meta>> {
        match self {
            Self::Map(m, meta) => Ok((m, meta)),
            other => Err(other),
        }
    }

    pub fn into_bool(self) -> Result<(bool, Meta), Resolvable<Id, Meta>> {
        match self {
            Self::Bool(b, meta) => Ok((b, meta)),
            other => Err(other),
        }
    }

    pub fn into_numeric(self) -> Result<(String, Meta), Resolvable<Id, Meta>> {
        match self {
            Self::Numeric(n, meta) => Ok((n, meta)),
            other => Err(other),
        }
    }

    pub fn as_str(&self) -> Option<&Id> {
        match self {
            Self::String(s, ..) => Some(s),
            _ => None,
        }
    }

    pub fn as_null(&self) -> Option<()> {
        match self {
            Self::Null(..) => Some(()),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<Resolvable<Id, Meta>>> {
        match self {
            Self::Array(a, ..) => Some(a),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&IndexMap<String, (Meta, Resolvable<Id, Meta>)>> {
        match self {
            Self::Map(m, ..) => Some(m),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b, ..) => Some(*b),
            _ => None,
        }
    }

    pub fn as_numeric(&self) -> Option<&String> {
        match self {
            Self::Numeric(n, ..) => Some(n),
            _ => None,
        }
    }

    pub fn meta(&self) -> &Meta {
        match self {
            Resolvable::String(_, meta) => meta,
            Resolvable::Numeric(_, meta) => meta,
            Resolvable::Bool(_, meta) => meta,
            Resolvable::Array(_, meta) => meta,
            Resolvable::Map(_, meta) => meta,
            Resolvable::Null(meta) => meta,
        }
    }

    pub fn r#type(&self) -> String {
        match self {
            Resolvable::String(..) => String::from("string"),
            Resolvable::Numeric(..) => String::from("number"),
            Resolvable::Bool(..) => String::from("bool"),
            Resolvable::Array(vec, ..) => {
                let parts = vec
                    .iter()
                    .map(|v| v.r#type())
                    .collect::<IndexSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                match parts.len() {
                    0 => "<empty>[]".to_string(),
                    1 => format!("{}[]", parts[0]),
                    _ => format!("({})[]", parts.join(" | ")),
                }
            }
            Resolvable::Map(index_map, ..) => {
                let content = index_map
                    .iter()
                    .map(|(k, (_, v))| format!("{k}: {}", v.r#type()))
                    .collect::<Vec<_>>()
                    .join(",\n");
                format!("{{\n{content}\n}}")
            }
            Resolvable::Null(..) => String::from("null"),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum StringOr {
    EnvVar(String),
    Value(String),
    JinjaExpression(JinjaExpression),
}

impl StringOr {
    pub fn required_env_vars(&self) -> HashSet<String> {
        match self {
            Self::EnvVar(name) => HashSet::from([name.clone()]),
            Self::Value(_) => HashSet::new(),
            Self::JinjaExpression(_) => HashSet::new(),
        }
    }

    pub fn maybe_eq(&self, other: &StringOr) -> bool {
        match (self, other) {
            (Self::Value(s), Self::Value(o)) => s == o,
            (Self::Value(_), _) | (_, Self::Value(_)) => true,
            (Self::EnvVar(_), Self::JinjaExpression(_))
            | (Self::JinjaExpression(_), Self::EnvVar(_)) => true,
            (Self::JinjaExpression(_), Self::JinjaExpression(_)) => true,
            (Self::EnvVar(s), Self::EnvVar(o)) => s == o,
        }
    }

    pub fn resolve(&self, ctx: &impl GetEnvVar) -> Result<String> {
        match self {
            Self::EnvVar(name) => ctx.get_env_var(name),
            Self::Value(value) => Ok(value.to_string()),
            Self::JinjaExpression(_) => todo!("Jinja expressions cannot yet be resolved"),
        }
    }

    pub fn resolve_api_key(&self, ctx: &impl GetEnvVar) -> Result<ApiKeyWithProvenance> {
        let api_key = SecretString::from(self.resolve(ctx)?);
        let provenance = match self {
            Self::EnvVar(env_var) => Some(env_var.to_string()),
            Self::Value(_) => None,
            Self::JinjaExpression(_) => None,
        };
        Ok(ApiKeyWithProvenance {
            api_key,
            provenance,
        })
    }
}

impl std::fmt::Display for StringOr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Value(s) => write!(f, "{s}"),
            Self::EnvVar(s) => write!(f, "${s}"),
            Self::JinjaExpression(j) => write!(f, "{{ {j} }}"),
        }
    }
}

pub type UnresolvedValue<Meta> = Resolvable<StringOr, Meta>;
pub type ResolvedValue = Resolvable<String, ()>;

impl<Meta> UnresolvedValue<Meta> {
    pub fn without_meta(&self) -> UnresolvedValue<()> {
        match self {
            Self::String(s, ..) => Resolvable::String(s.clone(), ()),
            Self::Numeric(n, ..) => Resolvable::Numeric(n.clone(), ()),
            Self::Bool(b, ..) => Resolvable::Bool(*b, ()),
            Self::Array(a, ..) => {
                Resolvable::Array(a.iter().map(|v| v.without_meta()).collect(), ())
            }
            Self::Map(m, ..) => Resolvable::Map(
                m.iter()
                    .map(|(k, (_, v))| (k.clone(), ((), v.without_meta())))
                    .collect(),
                (),
            ),
            Self::Null(..) => Resolvable::Null(()),
        }
    }
}

pub trait GetEnvVar {
    fn get_env_var(&self, key: &str) -> Result<String>;
    fn set_allow_missing_env_var(&self, allow: bool) -> Self;
}

pub struct EvaluationContext<'a> {
    env_vars: Option<&'a HashMap<String, String>>,
    fill_missing_env_vars: bool,
}

impl<'a> GetEnvVar for EvaluationContext<'a> {
    fn get_env_var(&self, key: &str) -> Result<String> {
        match self
            .env_vars
            .as_ref()
            .and_then(|env_vars| env_vars.get(key))
        {
            Some(v) => Ok(v.to_string()),
            None => {
                if self.fill_missing_env_vars {
                    Ok(format!("${key}"))
                } else {
                    Err(anyhow::anyhow!("Environment variable {key} not set"))
                }
            }
        }
    }

    fn set_allow_missing_env_var(&self, allow: bool) -> Self {
        Self {
            env_vars: self.env_vars,
            fill_missing_env_vars: allow,
        }
    }
}

impl<'a> EvaluationContext<'a> {
    pub fn new(env_vars: &'a HashMap<String, String>, fill_missing_env_vars: bool) -> Self {
        Self {
            env_vars: Some(env_vars),
            fill_missing_env_vars,
        }
    }
}

impl<'db> Default for EvaluationContext<'db> {
    fn default() -> Self {
        Self {
            env_vars: None,
            fill_missing_env_vars: true,
        }
    }
}

impl<Meta> UnresolvedValue<Meta> {
    pub fn as_static_str(&self) -> Result<&str> {
        match self {
            Self::String(StringOr::Value(v), ..) => Ok(v.as_str()),
            Self::String(StringOr::EnvVar(..), ..) => {
                anyhow::bail!("Expected a statically defined string, not env variable")
            }
            Self::String(StringOr::JinjaExpression(..), ..) => {
                anyhow::bail!("Expected a statically defined string, not expression")
            }
            Self::Numeric(num, ..) => Ok(num.as_str()),
            Self::Array(..) => anyhow::bail!("Expected a string, not an array"),
            Self::Bool(..) => anyhow::bail!("Expected a string, not a bool"),
            Self::Map(..) => anyhow::bail!("Expected a string, not a map"),
            Self::Null(..) => anyhow::bail!("Expected a string, not null"),
        }
    }

    pub fn resolve_string(&self, ctx: &impl GetEnvVar) -> Result<String> {
        match self.resolve(ctx) {
            Ok(ResolvedValue::String(s, ..)) => Ok(s),
            _ => Err(anyhow::anyhow!("Expected a string")),
        }
    }

    pub fn resolve_bool(&self, ctx: &impl GetEnvVar) -> Result<bool> {
        match self.resolve(ctx) {
            Ok(ResolvedValue::Bool(b, ..)) => Ok(b),
            _ => Err(anyhow::anyhow!("Expected a boolean")),
        }
    }

    pub fn resolve_array(&self, ctx: &impl GetEnvVar) -> Result<Vec<ResolvedValue>> {
        match self.resolve(ctx) {
            Ok(ResolvedValue::Array(a, ..)) => Ok(a),
            _ => Err(anyhow::anyhow!("Expected an array")),
        }
    }

    pub fn resolve_map(&self, ctx: &impl GetEnvVar) -> Result<IndexMap<String, ResolvedValue>> {
        match self.resolve(ctx) {
            Ok(ResolvedValue::Map(m, ..)) => Ok(m.into_iter().map(|(k, (_, v))| (k, v)).collect()),
            _ => Err(anyhow::anyhow!("Expected a map")),
        }
    }

    pub fn resolve_numeric(&self, ctx: &impl GetEnvVar) -> Result<String> {
        match self.resolve(ctx) {
            Ok(ResolvedValue::Numeric(n, ..)) => Ok(n),
            _ => Err(anyhow::anyhow!("Expected a numeric value")),
        }
    }

    pub fn resolve_null(&self, ctx: &impl GetEnvVar) -> Result<()> {
        match self.resolve(ctx) {
            Ok(ResolvedValue::Null(..)) => Ok(()),
            _ => Err(anyhow::anyhow!("Expected a null value")),
        }
    }

    pub fn resolve_serde<T: serde::de::DeserializeOwned>(&self, ctx: &impl GetEnvVar) -> Result<T> {
        let value = self.resolve(ctx)?;
        let value: serde_json::Value = value.try_into()?;
        match serde_json::from_value(value) {
            Ok(v) => Ok(v),
            Err(e) => Err(anyhow::anyhow!("Failed to deserialize value: {e}")),
        }
    }

    /// Resolve the value to a [`ResolvedValue`].
    fn resolve(&self, ctx: &impl GetEnvVar) -> Result<ResolvedValue> {
        match self {
            Self::String(string_or, ..) => {
                string_or.resolve(ctx).map(|v| ResolvedValue::String(v, ()))
            }
            Self::Numeric(numeric, ..) => Ok(ResolvedValue::Numeric(numeric.clone(), ())),
            Self::Bool(bool, ..) => Ok(ResolvedValue::Bool(*bool, ())),
            Self::Array(array, ..) => {
                let values = array
                    .iter()
                    .map(|item| item.resolve(ctx))
                    .collect::<Result<Vec<_>>>()?;
                Ok(ResolvedValue::Array(values, ()))
            }
            Self::Map(map, ..) => {
                let values = map
                    .iter()
                    .map(|(k, (_, v))| Ok((k.to_string(), ((), v.resolve(ctx)?))))
                    .collect::<Result<_>>()?;
                Ok(ResolvedValue::Map(values, ()))
            }
            Self::Null(..) => Ok(ResolvedValue::Null(())),
        }
    }

    pub fn required_env_vars(&self) -> HashSet<String> {
        let mut env_vars = HashSet::new();
        let mut stack = vec![self];

        while let Some(current) = stack.pop() {
            match current {
                Self::String(s, ..) => {
                    env_vars.extend(s.required_env_vars());
                }
                Self::Array(array, ..) => {
                    stack.extend(array);
                }
                Self::Map(map, ..) => {
                    stack.extend(map.values().map(|(_, v)| v));
                }
                _ => {}
            }
        }

        env_vars
    }
}

// ResolvedValue -> serde_json::Value
impl TryFrom<ResolvedValue> for serde_json::Value {
    type Error = anyhow::Error;

    fn try_from(value: ResolvedValue) -> Result<Self> {
        Ok(match value {
            ResolvedValue::String(s, ..) => serde_json::Value::String(s),
            ResolvedValue::Numeric(n, ..) => {
                serde_json::Value::Number(serde_json::Number::from_str(n.as_str())?)
            }
            ResolvedValue::Bool(b, ..) => serde_json::Value::Bool(b),
            ResolvedValue::Array(a, ..) => serde_json::Value::Array(
                a.into_iter()
                    .map(serde_json::Value::try_from)
                    .collect::<Result<_>>()?,
            ),
            ResolvedValue::Map(m, ..) => serde_json::Value::Object(
                m.into_iter()
                    .map(|(k, (_, v))| Ok((k.clone(), serde_json::Value::try_from(v)?)))
                    .collect::<Result<_>>()?,
            ),
            ResolvedValue::Null(..) => serde_json::Value::Null,
        })
    }
}

impl crate::BamlValue {
    pub fn to_resolvable(&self) -> Result<Resolvable<StringOr, ()>> {
        Ok(match self {
            crate::BamlValue::Enum(_, s) | crate::BamlValue::String(s) => {
                Resolvable::String(StringOr::Value(s.clone()), ())
            }
            crate::BamlValue::Int(i) => Resolvable::Numeric(i.to_string(), ()),
            crate::BamlValue::Float(f) => Resolvable::Numeric(f.to_string(), ()),
            crate::BamlValue::Bool(b) => Resolvable::Bool(*b, ()),
            crate::BamlValue::Class(_, index_map) | crate::BamlValue::Map(index_map) => {
                let values = index_map
                    .iter()
                    .map(|(k, v)| Ok((k.clone(), ((), v.to_resolvable()?))))
                    .collect::<Result<_>>()?;
                Resolvable::Map(values, ())
            }
            crate::BamlValue::List(vec) => {
                let values = vec
                    .iter()
                    .map(|v| v.to_resolvable())
                    .collect::<Result<_>>()?;
                Resolvable::Array(values, ())
            }
            crate::BamlValue::Media(m) => m.to_resolvable()?,
            crate::BamlValue::Null => Resolvable::Null(()),
        })
    }
}

impl crate::BamlMedia {
    pub fn to_resolvable(&self) -> Result<Resolvable<StringOr, ()>> {
        let mut index_map = IndexMap::default();
        if let Some(mime_type) = &self.mime_type {
            index_map.insert(
                "mime_type".to_string(),
                (
                    (),
                    Resolvable::String(StringOr::Value(mime_type.clone()), ()),
                ),
            );
        }
        let (key, value) = match &self.content {
            crate::BamlMediaContent::File(f) => ("file", f.path()?.to_string_lossy().to_string()),
            crate::BamlMediaContent::Url(u) => ("url", u.url.clone()),
            crate::BamlMediaContent::Base64(b) => ("base64", b.base64.clone()),
        };
        index_map.insert(
            key.to_string(),
            ((), Resolvable::String(StringOr::Value(value), ())),
        );
        Ok(Resolvable::Map(index_map, ()))
    }
}

pub struct ApiKeyWithProvenance {
    /// The key itself.
    pub api_key: SecretString,
    /// The name of the environment variable from which the key was read.
    pub provenance: Option<String>,
}

impl ApiKeyWithProvenance {
    /// Print the api_key if exposing the secret is allowed.
    /// Otherwise, render the provenance as an environment variable
    /// if we have one, falling back to a default string.
    pub fn render(&self, expose_secret: bool) -> String {
        if expose_secret {
            self.api_key.expose_secret().to_string()
        } else {
            self.provenance
                .as_ref()
                .map_or("<SECRET_HIDDEN>".to_string(), |env_var| {
                    format!("${}", env_var)
                })
        }
    }
}
