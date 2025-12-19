use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
};

use anyhow::Result;
use indexmap::{IndexMap, IndexSet};
use itertools::Itertools;
use secrecy::{ExposeSecret, SecretString};

use crate::JinjaExpression;

#[derive(Debug, Clone)]
pub enum Resolvable<Id, Meta> {
    // Enums go into here.
    String(Id, Meta),
    // Repred as a string, but guaranteed to be a number.
    Numeric(String, Meta),
    Bool(bool, Meta),
    Array(Vec<Resolvable<Id, Meta>>, Meta),
    // This includes key-value pairs for classes
    Map(IndexMap<String, (Meta, Resolvable<Id, Meta>)>, Meta),
    // The class name and list of fields as resolvable values.
    ClassConstructor(String, Vec<(String, Resolvable<Id, Meta>)>, Meta),
    Null(Meta),
}

impl<Id: std::hash::Hash, Meta: std::hash::Hash> std::hash::Hash for Resolvable<Id, Meta> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // include the discriminant
        match self {
            Self::String(..) => state.write_u8(0),
            Self::Numeric(..) => state.write_u8(1),
            Self::Bool(..) => state.write_u8(2),
            Self::Array(..) => state.write_u8(3),
            Self::Map(..) => state.write_u8(4),
            Self::ClassConstructor(..) => state.write_u8(5),
            Self::Null(..) => state.write_u8(6),
        }

        match self {
            Self::String(s, meta) => {
                s.hash(state);
                meta.hash(state);
            }
            Self::Numeric(n, meta) => {
                n.hash(state);
                meta.hash(state);
            }
            Self::Bool(b, meta) => {
                b.hash(state);
                meta.hash(state);
            }
            Self::Array(a, meta) => {
                a.hash(state);
                meta.hash(state);
            }
            Self::Map(m, meta) => {
                let sorted_keys = m.keys().sorted().collect::<Vec<_>>();
                for k in sorted_keys {
                    k.hash(state);
                    m[k].hash(state);
                }
                meta.hash(state);
            }
            Self::ClassConstructor(c, fields, meta) => {
                c.hash(state);
                fields.hash(state);
                meta.hash(state);
            }
            Self::Null(meta) => {
                meta.hash(state);
            }
        }
    }
}

impl<Id: PartialEq, Meta: PartialEq> PartialEq for Resolvable<Id, Meta> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::String(s1, m1), Self::String(s2, m2)) => s1 == s2 && m1 == m2,
            (Self::Numeric(n1, m1), Self::Numeric(n2, m2)) => n1 == n2 && m1 == m2,
            (Self::Bool(b1, m1), Self::Bool(b2, m2)) => b1 == b2 && m1 == m2,
            (Self::Array(a1, m1), Self::Array(a2, m2)) => a1 == a2 && m1 == m2,
            (Self::Map(map1, m1), Self::Map(map2, m2)) => {
                // Compare maps by checking all key-value pairs match
                m1 == m2
                    && map1.len() == map2.len()
                    && map1.iter().all(|(k, (meta1, v1))| {
                        map2.get(k)
                            .map(|(meta2, v2)| meta1 == meta2 && v1 == v2)
                            .unwrap_or(false)
                    })
            }
            (Self::ClassConstructor(n1, f1, m1), Self::ClassConstructor(n2, f2, m2)) => {
                n1 == n2 && f1 == f2 && m1 == m2
            }
            (Self::Null(m1), Self::Null(m2)) => m1 == m2,
            _ => false,
        }
    }
}

impl<Id: Eq, Meta: Eq> Eq for Resolvable<Id, Meta> {}

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
            Resolvable::ClassConstructor(_, _, meta) => meta,
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
            Resolvable::ClassConstructor(class_name, _, _) => class_name.to_string(),
            Resolvable::Null(..) => String::from("null"),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum StringOr {
    EnvVar(String),
    Value(String),
    JinjaExpression(JinjaExpression),
    /// A template_string invocation with positional arguments.
    TemplateStringCall {
        name: String,
        args: Vec<Resolvable<StringOr, ()>>,
    },
}

/// Helper function to recursively collect env vars from a Resolvable<StringOr, _>
fn collect_env_vars_from_resolvable<Meta>(
    resolvable: &Resolvable<StringOr, Meta>,
    env_vars: &mut HashSet<String>,
) {
    match resolvable {
        Resolvable::String(string_or, _) => {
            env_vars.extend(string_or.required_env_vars());
        }
        Resolvable::Array(items, _) => {
            for item in items {
                collect_env_vars_from_resolvable(item, env_vars);
            }
        }
        Resolvable::Map(map, _) => {
            for (_, (_, v)) in map {
                collect_env_vars_from_resolvable(v, env_vars);
            }
        }
        Resolvable::ClassConstructor(_, fields, _) => {
            for (_, v) in fields {
                collect_env_vars_from_resolvable(v, env_vars);
            }
        }
        Resolvable::Numeric(_, _) | Resolvable::Bool(_, _) | Resolvable::Null(_) => {}
    }
}

impl StringOr {
    pub fn required_env_vars(&self) -> HashSet<String> {
        match self {
            Self::EnvVar(name) => HashSet::from([name.clone()]),
            Self::Value(_) => HashSet::new(),
            Self::JinjaExpression(_) => HashSet::new(),
            Self::TemplateStringCall { args, .. } => {
                // Recursively collect env vars from all arguments
                let mut env_vars = HashSet::new();
                for arg in args {
                    collect_env_vars_from_resolvable(arg, &mut env_vars);
                }
                env_vars
            }
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
            // Template string calls could evaluate to anything, so conservatively return true
            (Self::TemplateStringCall { .. }, _) | (_, Self::TemplateStringCall { .. }) => true,
        }
    }

    pub fn resolve(&self, ctx: &impl GetEnvVar) -> Result<String> {
        match self {
            Self::EnvVar(name) => ctx.get_env_var(name),
            Self::Value(value) => Ok(value.to_string()),
            Self::JinjaExpression(_) => {
                anyhow::bail!("Jinja expressions cannot be resolved without a template context")
            }
            Self::TemplateStringCall { name, .. } => {
                anyhow::bail!(
                    "Template string call '{}' cannot be resolved without IR context. \
                     Use resolve_with_templates() instead.",
                    name
                )
            }
        }
    }

    /// Resolve this StringOr, with support for template_string calls.
    pub fn resolve_with_templates(
        &self,
        ctx: &impl GetEnvVar,
        template_renderer: &impl TemplateStringRenderer,
    ) -> Result<String> {
        match self {
            Self::EnvVar(name) => ctx.get_env_var(name),
            Self::Value(value) => Ok(value.to_string()),
            Self::JinjaExpression(_) => {
                anyhow::bail!("Jinja expressions cannot be resolved without a template context")
            }
            Self::TemplateStringCall { name, args } => {
                // First, resolve all arguments to JSON values
                let resolved_args: Vec<serde_json::Value> = args
                    .iter()
                    .map(|arg| {
                        let resolved = arg.resolve_with_templates(ctx, template_renderer)?;
                        resolved.try_into()
                    })
                    .collect::<Result<_>>()?;

                // Then render the template
                template_renderer.render_template(name, &resolved_args)
            }
        }
    }

    pub fn resolve_api_key(&self, ctx: &impl GetEnvVar) -> Result<ApiKeyWithProvenance> {
        let api_key = SecretString::from(self.resolve(ctx)?);
        let provenance = match self {
            Self::EnvVar(env_var) => Some(env_var.to_string()),
            Self::Value(_) => None,
            Self::JinjaExpression(_) => None,
            Self::TemplateStringCall { .. } => None,
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
            Self::TemplateStringCall { name, args } => {
                write!(f, "{name}(")?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                write!(f, ")")
            }
        }
    }
}

impl<Meta> std::fmt::Display for Resolvable<StringOr, Meta> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String(s, _) => write!(f, "{s}"),
            Self::Numeric(n, _) => write!(f, "{n}"),
            Self::Bool(b, _) => write!(f, "{b}"),
            Self::Null(_) => write!(f, "null"),
            Self::Array(items, _) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            Self::Map(map, _) => {
                write!(f, "{{")?;
                for (i, (k, (_, v))) in map.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            Self::ClassConstructor(name, fields, _) => {
                write!(f, "{name}{{")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
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
            Self::ClassConstructor(class_name, fields, ..) => Resolvable::ClassConstructor(
                class_name.clone(),
                fields
                    .iter()
                    .map(|(k, v)| (k.clone(), v.without_meta()))
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

/// Trait for rendering template_string calls.
/// This is implemented in baml-core where the IR is available.
pub trait TemplateStringRenderer {
    /// Render a template_string with the given name and arguments.
    /// Arguments are positional and should be converted to BamlValue before rendering.
    fn render_template(&self, name: &str, args: &[serde_json::Value]) -> Result<String>;
}

pub struct EvaluationContext<'a> {
    env_vars: Option<&'a HashMap<String, String>>,
    fill_missing_env_vars: bool,
}

impl GetEnvVar for EvaluationContext<'_> {
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

impl Default for EvaluationContext<'_> {
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
            Self::String(StringOr::TemplateStringCall { .. }, ..) => {
                anyhow::bail!("Expected a statically defined string, not a template_string call")
            }
            Self::Numeric(num, ..) => Ok(num.as_str()),
            Self::Array(..) => anyhow::bail!("Expected a string, not an array"),
            Self::Bool(..) => anyhow::bail!("Expected a string, not a bool"),
            Self::Map(..) => anyhow::bail!("Expected a string, not a map"),
            Self::Null(..) => anyhow::bail!("Expected a string, not null"),
            Self::ClassConstructor(..) => {
                anyhow::bail!("Expected a string, not a class constructor")
            }
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

    /// Resolve and deserialize, with support for template_string calls.
    pub fn resolve_serde_with_templates<T: serde::de::DeserializeOwned>(
        &self,
        ctx: &impl GetEnvVar,
        template_renderer: &impl TemplateStringRenderer,
    ) -> Result<T> {
        let value = self.resolve_with_templates(ctx, template_renderer)?;
        let value: serde_json::Value = value.try_into()?;
        match serde_json::from_value(value) {
            Ok(v) => Ok(v),
            Err(e) => Err(anyhow::anyhow!("Failed to deserialize value: {e}")),
        }
    }

    /// Resolve the value to a [`ResolvedValue`], with support for template_string calls.
    pub fn resolve_with_templates(
        &self,
        ctx: &impl GetEnvVar,
        template_renderer: &impl TemplateStringRenderer,
    ) -> Result<ResolvedValue> {
        match self {
            Self::String(string_or, ..) => string_or
                .resolve_with_templates(ctx, template_renderer)
                .map(|v| ResolvedValue::String(v, ())),
            Self::Numeric(numeric, ..) => Ok(ResolvedValue::Numeric(numeric.clone(), ())),
            Self::Bool(bool, ..) => Ok(ResolvedValue::Bool(*bool, ())),
            Self::Array(array, ..) => {
                let values = array
                    .iter()
                    .map(|item| item.resolve_with_templates(ctx, template_renderer))
                    .collect::<Result<Vec<_>>>()?;
                Ok(ResolvedValue::Array(values, ()))
            }
            Self::Map(map, ..) => {
                let values = map
                    .iter()
                    .map(|(k, (_, v))| {
                        Ok((
                            k.to_string(),
                            ((), v.resolve_with_templates(ctx, template_renderer)?),
                        ))
                    })
                    .collect::<Result<_>>()?;
                Ok(ResolvedValue::Map(values, ()))
            }
            Self::ClassConstructor(class_name, fields, _meta) => {
                let new_fields = fields
                    .iter()
                    .map(|(k, v)| {
                        v.resolve_with_templates(ctx, template_renderer)
                            .map(|res| (k.clone(), res))
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(ResolvedValue::ClassConstructor(
                    class_name.clone(),
                    new_fields,
                    (),
                ))
            }
            Self::Null(..) => Ok(ResolvedValue::Null(())),
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
            Self::ClassConstructor(class_name, fields, _meta) => {
                let new_fields = fields
                    .iter()
                    .map(|(k, v)| v.resolve(ctx).map(|res| (k.clone(), res)))
                    .collect::<Result<Vec<_>>>()?;
                Ok(ResolvedValue::ClassConstructor(
                    class_name.clone(),
                    new_fields,
                    (),
                ))
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
            ResolvedValue::ClassConstructor(_class_name, fields, ..) => serde_json::Value::Object(
                fields
                    .into_iter()
                    .map(|(k, v)| Ok((k, serde_json::Value::try_from(v)?)))
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

#[derive(Clone)]
pub struct ApiKeyWithProvenance {
    /// The key itself.
    pub api_key: SecretString,
    /// The name of the environment variable from which the key was read.
    pub provenance: Option<String>,
}

impl std::fmt::Debug for ApiKeyWithProvenance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiKeyWithProvenance")
            .field("api_key", &"<no-repr-available>")
            .field("provenance", &self.provenance)
            .finish()
    }
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
                    format!("${env_var}")
                })
        }
    }
}
