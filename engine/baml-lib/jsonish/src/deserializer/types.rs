use std::collections::HashSet;

use anyhow::Result;
use baml_types::{
    BamlMap, BamlMedia, BamlValue, BamlValueWithMeta, Constraint, JinjaExpression, TypeIR,
};
use serde_json::json;
use strsim::jaro;

use super::{
    coercer::ParsingError,
    deserialize_flags::{DeserializerConditions, Flag},
    score::WithScore,
};

// Recursive parity
#[derive(Clone)]
pub enum BamlValueWithFlags {
    String(ValueWithFlags<String>),
    Int(ValueWithFlags<i64>),
    Float(ValueWithFlags<f64>),
    Bool(ValueWithFlags<bool>),
    List(
        DeserializerConditions,
        baml_types::TypeIR,
        Vec<BamlValueWithFlags>,
    ),
    Map(
        DeserializerConditions,
        baml_types::TypeIR,
        BamlMap<String, (DeserializerConditions, BamlValueWithFlags)>,
    ),
    Enum(String, baml_types::TypeIR, ValueWithFlags<String>),
    Class(
        String,
        DeserializerConditions,
        baml_types::TypeIR,
        BamlMap<String, BamlValueWithFlags>,
    ),
    Null(baml_types::TypeIR, DeserializerConditions),
    Media(baml_types::TypeIR, ValueWithFlags<BamlMedia>),
}

impl std::fmt::Debug for BamlValueWithFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BamlValueWithFlags::String(v) => f.debug_tuple("String").field(&v.value).finish(),
            BamlValueWithFlags::Int(v) => f.debug_tuple("Int").field(&v.value).finish(),
            BamlValueWithFlags::Float(v) => f.debug_tuple("Float").field(&v.value).finish(),
            BamlValueWithFlags::Bool(v) => f.debug_tuple("Bool").field(&v.value).finish(),
            BamlValueWithFlags::List(flags, target, items) => f
                .debug_tuple("List")
                .field(&target.to_string())
                .field(&flags)
                .field(&items)
                .finish(),
            BamlValueWithFlags::Map(flags, target, items) => f
                .debug_tuple("Map")
                .field(&target.to_string())
                .field(&flags)
                .field(&items)
                .finish(),
            BamlValueWithFlags::Enum(v, target, flags) => f
                .debug_struct("Enum")
                .field("name", &v)
                .field("type", &target.to_string())
                .field("flags", &flags)
                .finish(),
            BamlValueWithFlags::Class(v, c, target, fields) => f
                .debug_struct("Class")
                .field("name", &v)
                .field("type", &target.to_string())
                .field("flags", &c)
                .field("fields", &fields)
                .finish(),
            BamlValueWithFlags::Null(target, flags) => f
                .debug_struct("Null")
                .field("type", &target.to_string())
                .field("flags", &flags)
                .finish(),
            BamlValueWithFlags::Media(target, flags) => f
                .debug_struct("Media")
                .field("type", &target.to_string())
                .field("flags", &flags)
                .finish(),
        }
    }
}

impl BamlValueWithFlags {
    #[cfg(test)]
    pub fn as_list(&self) -> Option<&[BamlValueWithFlags]> {
        match self {
            BamlValueWithFlags::List(_, _, v) => Some(v),
            _ => None,
        }
    }

    pub fn field_type(&self) -> &baml_types::TypeIR {
        match self {
            BamlValueWithFlags::String(v) => &v.target,
            BamlValueWithFlags::Int(v) => &v.target,
            BamlValueWithFlags::Float(v) => &v.target,
            BamlValueWithFlags::Bool(v) => &v.target,
            BamlValueWithFlags::List(_, target, _) => target,
            BamlValueWithFlags::Map(_, target, _) => target,
            BamlValueWithFlags::Enum(_, target, _) => target,
            BamlValueWithFlags::Class(_, _, target, _) => target,
            BamlValueWithFlags::Null(target, _) => target,
            BamlValueWithFlags::Media(target, _) => target,
        }
    }

    pub fn with_target(self, target: &baml_types::TypeIR) -> Self {
        match self {
            BamlValueWithFlags::String(v) => BamlValueWithFlags::String(v.with_target(target)),
            BamlValueWithFlags::Int(v) => BamlValueWithFlags::Int(v.with_target(target)),
            BamlValueWithFlags::Float(v) => BamlValueWithFlags::Float(v.with_target(target)),
            BamlValueWithFlags::Bool(v) => BamlValueWithFlags::Bool(v.with_target(target)),
            BamlValueWithFlags::List(v, _, items) => {
                BamlValueWithFlags::List(v, target.clone(), items)
            }
            BamlValueWithFlags::Map(v, _, items) => {
                BamlValueWithFlags::Map(v, target.clone(), items)
            }
            BamlValueWithFlags::Enum(v, _, f) => BamlValueWithFlags::Enum(v, target.clone(), f),
            BamlValueWithFlags::Class(v, c, _, f) => {
                BamlValueWithFlags::Class(v, c, target.clone(), f)
            }
            BamlValueWithFlags::Null(_, f) => BamlValueWithFlags::Null(target.clone(), f),
            BamlValueWithFlags::Media(_, f) => BamlValueWithFlags::Media(target.clone(), f),
        }
    }

    pub fn is_composite(&self) -> bool {
        match self {
            BamlValueWithFlags::String(..)
            | BamlValueWithFlags::Int(..)
            | BamlValueWithFlags::Float(..)
            | BamlValueWithFlags::Bool(..)
            | BamlValueWithFlags::Null(..)
            | BamlValueWithFlags::Enum(..) => false,

            BamlValueWithFlags::List(..)
            | BamlValueWithFlags::Map(..)
            | BamlValueWithFlags::Class(..)
            | BamlValueWithFlags::Media(..) => true,
        }
    }

    pub fn score(&self) -> i32 {
        match self {
            BamlValueWithFlags::String(f) => f.score(),
            BamlValueWithFlags::Int(f) => f.score(),
            BamlValueWithFlags::Float(f) => f.score(),
            BamlValueWithFlags::Bool(f) => f.score(),
            BamlValueWithFlags::List(f, target, items) => {
                f.score() + items.iter().map(|i| i.score()).sum::<i32>()
            }
            BamlValueWithFlags::Map(f, target, kv) => {
                f.score()
                    + kv.iter()
                        .map(|(_, (f, v))| f.score() + v.score())
                        .sum::<i32>()
            }
            BamlValueWithFlags::Enum(_, target, f) => f.score(),
            BamlValueWithFlags::Class(_, f, target, items) => {
                f.score() + items.iter().map(|(_, v)| v.score()).sum::<i32>()
            }
            BamlValueWithFlags::Null(target, f) => f.score(),
            BamlValueWithFlags::Media(target, f) => f.score(),
        }
    }

    pub fn conditions(&self) -> &DeserializerConditions {
        match self {
            BamlValueWithFlags::String(v) => &v.flags,
            BamlValueWithFlags::Int(v) => &v.flags,
            BamlValueWithFlags::Float(v) => &v.flags,
            BamlValueWithFlags::Bool(v) => &v.flags,
            BamlValueWithFlags::List(v, _, _) => v,
            BamlValueWithFlags::Map(v, _, _) => v,
            BamlValueWithFlags::Enum(_, _, v) => &v.flags,
            BamlValueWithFlags::Class(_, v, _, _) => v,
            BamlValueWithFlags::Null(_, v) => v,
            BamlValueWithFlags::Media(_, v) => &v.flags,
        }
    }
}

impl From<BamlValueWithFlags> for BamlValueWithMeta<TypeIR> {
    fn from(baml_value: BamlValueWithFlags) -> BamlValueWithMeta<TypeIR> {
        let field_type = baml_value.field_type().clone();
        match baml_value {
            BamlValueWithFlags::String(v) => BamlValueWithMeta::String(v.value, field_type),
            BamlValueWithFlags::Int(v) => BamlValueWithMeta::Int(v.value, field_type),
            BamlValueWithFlags::Float(v) => BamlValueWithMeta::Float(v.value, field_type),
            BamlValueWithFlags::Bool(v) => BamlValueWithMeta::Bool(v.value, field_type),
            BamlValueWithFlags::List(conditions, target, items) => BamlValueWithMeta::List(
                items.into_iter().map(BamlValueWithMeta::from).collect(),
                field_type,
            ),
            BamlValueWithFlags::Map(conditions, target, fields) => BamlValueWithMeta::Map(
                // NOTE: For some reason, Map is map<key, (conds, v)>, even though `v` contains conds.
                // Maybe the extra conds are for the field, not the value?
                fields
                    .into_iter()
                    .map(|(k, v)| (k, BamlValueWithMeta::from(v.1)))
                    .collect(),
                field_type,
            ),
            BamlValueWithFlags::Enum(n, target, v) => {
                BamlValueWithMeta::Enum(n, v.value, field_type)
            }
            BamlValueWithFlags::Class(name, conds, target, fields) => BamlValueWithMeta::Class(
                name,
                fields
                    .into_iter()
                    .map(|(k, v)| (k, BamlValueWithMeta::from(v)))
                    .collect(),
                field_type,
            ),
            BamlValueWithFlags::Null(target, v) => BamlValueWithMeta::Null(field_type),
            BamlValueWithFlags::Media(target, v) => BamlValueWithMeta::Media(v.value, field_type),
        }
    }
}

impl From<BamlValueWithFlags> for BamlValueWithMeta<Vec<Flag>> {
    fn from(baml_value: BamlValueWithFlags) -> BamlValueWithMeta<Vec<Flag>> {
        match baml_value {
            BamlValueWithFlags::String(v) => BamlValueWithMeta::String(v.value, v.flags.flags),
            BamlValueWithFlags::Int(v) => BamlValueWithMeta::Int(v.value, v.flags.flags),
            BamlValueWithFlags::Float(v) => BamlValueWithMeta::Float(v.value, v.flags.flags),
            BamlValueWithFlags::Bool(v) => BamlValueWithMeta::Bool(v.value, v.flags.flags),
            BamlValueWithFlags::List(conditions, target, items) => BamlValueWithMeta::List(
                items.into_iter().map(BamlValueWithMeta::from).collect(),
                conditions.flags,
            ),
            BamlValueWithFlags::Map(conditions, target, fields) => BamlValueWithMeta::Map(
                // NOTE: For some reason, Map is map<key, (conds, v)>, even though `v` contains conds.
                // Maybe the extra conds are for the field, not the value?
                fields
                    .into_iter()
                    .map(|(k, v)| (k, BamlValueWithMeta::from(v.1)))
                    .collect(),
                conditions.flags,
            ),
            BamlValueWithFlags::Enum(n, target, v) => {
                BamlValueWithMeta::Enum(n, v.value, v.flags.flags)
            }
            BamlValueWithFlags::Class(name, conds, target, fields) => BamlValueWithMeta::Class(
                name,
                fields
                    .into_iter()
                    .map(|(k, v)| (k, BamlValueWithMeta::from(v)))
                    .collect(),
                conds.flags,
            ),
            BamlValueWithFlags::Null(target, v) => BamlValueWithMeta::Null(v.flags),
            BamlValueWithFlags::Media(target, v) => {
                BamlValueWithMeta::Media(v.value, v.flags.flags)
            }
        }
    }
}

pub trait ParsingErrorToUiJson {
    fn to_ui_json(&self) -> serde_json::Value;
}

impl ParsingErrorToUiJson for ParsingError {
    fn to_ui_json(&self) -> serde_json::Value {
        json!({
            if self.scope.is_empty() {
                "<root>".to_string()
            } else {
                self.scope.join(".")
            }: self.reason,
            "causes": self.causes.iter().map(|c| c.to_ui_json()).collect::<Vec<_>>(),
        })
    }
}

impl BamlValueWithFlags {
    pub fn explanation_json(&self) -> Vec<serde_json::Value> {
        let mut expl = vec![];
        self.explanation_impl(vec!["<root>".to_string()], &mut expl);
        expl.into_iter().map(|e| e.to_ui_json()).collect::<Vec<_>>()
    }

    pub fn explanation_impl(&self, scope: Vec<String>, expls: &mut Vec<ParsingError>) {
        match self {
            BamlValueWithFlags::String(v) => {
                let causes = v.flags.explanation();
                if !causes.is_empty() {
                    expls.push(ParsingError {
                        scope: scope.clone(),
                        reason: "error while parsing string".to_string(),
                        causes,
                    });
                }
            }
            BamlValueWithFlags::Int(v) => {
                let causes = v.flags.explanation();
                if !causes.is_empty() {
                    expls.push(ParsingError {
                        scope: scope.clone(),
                        reason: "error while parsing int".to_string(),
                        causes,
                    });
                }
            }
            BamlValueWithFlags::Float(v) => {
                let causes = v.flags.explanation();
                if !causes.is_empty() {
                    expls.push(ParsingError {
                        scope: scope.clone(),
                        reason: "error while parsing float".to_string(),
                        causes,
                    });
                }
            }
            BamlValueWithFlags::Bool(v) => {
                let causes = v.flags.explanation();
                if !causes.is_empty() {
                    expls.push(ParsingError {
                        scope: scope.clone(),
                        reason: "error while parsing bool".to_string(),
                        causes,
                    });
                }
            }
            BamlValueWithFlags::List(flags, target, values) => {
                let causes = flags.explanation();
                if !causes.is_empty() {
                    expls.push(ParsingError {
                        scope: scope.clone(),
                        reason: "error while parsing list".to_string(),
                        causes,
                    });
                }
                for (i, value) in values.iter().enumerate() {
                    let mut scope = scope.clone();
                    scope.push(format!("parsed:{i}"));
                    value.explanation_impl(scope, expls);
                }
            }
            BamlValueWithFlags::Map(flags, target, kv) => {
                let causes = flags.explanation();
                if !causes.is_empty() {
                    expls.push(ParsingError {
                        scope: scope.clone(),
                        reason: "error while parsing map".to_string(),
                        causes,
                    });
                }
                for (k, (v_flags, v)) in kv.iter() {
                    let causes = v_flags.explanation();
                    if !causes.is_empty() {
                        expls.push(ParsingError {
                            scope: scope.clone(),
                            reason: format!("error while parsing value for map key '{k}'"),
                            causes,
                        });
                    }
                    let mut scope = scope.clone();
                    scope.push(format!("parsed:{k}"));
                    v.explanation_impl(scope, expls);
                }
            }
            BamlValueWithFlags::Enum(enum_name, target, v) => {
                let causes = v.flags.explanation();
                if !causes.is_empty() {
                    expls.push(ParsingError {
                        scope: scope.clone(),
                        reason: format!("error while parsing {enum_name} enum value"),
                        causes,
                    });
                }
            }
            BamlValueWithFlags::Class(class_name, conds, target, fields) => {
                let causes = conds.explanation();
                if !causes.is_empty() {
                    expls.push(ParsingError {
                        scope: scope.clone(),
                        reason: format!("error while parsing class {class_name}"),
                        causes,
                    });
                }
                for (k, v) in fields.iter() {
                    let mut scope = scope.clone();
                    scope.push(k.to_string());
                    v.explanation_impl(scope, expls);
                }
            }

            BamlValueWithFlags::Null(target, v) => {
                let causes = v.explanation();
                if !causes.is_empty() {
                    expls.push(ParsingError {
                        scope: scope.clone(),
                        reason: "error while parsing null".to_string(),
                        causes,
                    });
                }
            }
            BamlValueWithFlags::Media(target, v) => {
                let causes = v.flags.explanation();
                if !causes.is_empty() {
                    expls.push(ParsingError {
                        scope: scope.clone(),
                        reason: "error while parsing media".to_string(),
                        causes,
                    });
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct ValueWithFlags<T> {
    pub value: T,
    pub target: baml_types::TypeIR,
    pub flags: DeserializerConditions,
}

impl<T: std::fmt::Debug> std::fmt::Debug for ValueWithFlags<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ValueWithFlags")
            .field(&self.target.to_string())
            .field(&self.value)
            .field(&self.flags)
            .finish()
    }
}

impl<T> ValueWithFlags<T> {
    pub fn with_target(self, target: &baml_types::TypeIR) -> Self {
        ValueWithFlags {
            value: self.value,
            target: target.clone(),
            flags: self.flags,
        }
    }

    pub fn value(&self) -> &T {
        &self.value
    }
}

impl<T> From<(T, &baml_types::TypeIR)> for ValueWithFlags<T> {
    fn from((value, target): (T, &baml_types::TypeIR)) -> Self {
        ValueWithFlags {
            value,
            target: target.clone(),
            flags: DeserializerConditions::new(),
        }
    }
}

impl<T> From<(T, &baml_types::TypeIR, &[Flag])> for ValueWithFlags<T> {
    fn from((value, target, flags): (T, &baml_types::TypeIR, &[Flag])) -> Self {
        let flags = flags
            .iter()
            .fold(DeserializerConditions::new(), |acc, flag| {
                acc.with_flag(flag.to_owned())
            });
        ValueWithFlags {
            value,
            target: target.clone(),
            flags,
        }
    }
}

impl<T> From<(T, &baml_types::TypeIR, Flag)> for ValueWithFlags<T> {
    fn from((value, target, flag): (T, &baml_types::TypeIR, Flag)) -> Self {
        ValueWithFlags {
            value,
            target: target.clone(),
            flags: DeserializerConditions::new().with_flag(flag),
        }
    }
}

impl<T> From<(T, &baml_types::TypeIR, DeserializerConditions)> for ValueWithFlags<T> {
    fn from((value, target, flags): (T, &baml_types::TypeIR, DeserializerConditions)) -> Self {
        ValueWithFlags {
            value,
            target: target.clone(),
            flags,
        }
    }
}

impl From<BamlValueWithFlags> for BamlValue {
    fn from(value: BamlValueWithFlags) -> BamlValue {
        match value {
            BamlValueWithFlags::String(s) => BamlValue::String(s.value),
            BamlValueWithFlags::Int(i) => BamlValue::Int(i.value),
            BamlValueWithFlags::Float(f) => BamlValue::Float(f.value),
            BamlValueWithFlags::Bool(b) => BamlValue::Bool(b.value),
            BamlValueWithFlags::List(_, _, v) => {
                BamlValue::List(v.into_iter().map(|x| x.into()).collect())
            }
            BamlValueWithFlags::Map(_, _, m) => {
                BamlValue::Map(m.into_iter().map(|(k, (_, v))| (k, v.into())).collect())
            }
            BamlValueWithFlags::Enum(s, _, v) => BamlValue::Enum(s, v.value),
            BamlValueWithFlags::Class(s, _, _, m) => {
                BamlValue::Class(s, m.into_iter().map(|(k, v)| (k, v.into())).collect())
            }
            BamlValueWithFlags::Null(_, _) => BamlValue::Null,
            BamlValueWithFlags::Media(_, i) => BamlValue::Media(i.value),
        }
    }
}

impl From<&BamlValueWithFlags> for BamlValue {
    fn from(value: &BamlValueWithFlags) -> BamlValue {
        match value {
            BamlValueWithFlags::String(s) => BamlValue::String(s.value.clone()),
            BamlValueWithFlags::Int(i) => BamlValue::Int(i.value),
            BamlValueWithFlags::Float(f) => BamlValue::Float(f.value),
            BamlValueWithFlags::Bool(b) => BamlValue::Bool(b.value),
            BamlValueWithFlags::List(_, _, v) => {
                BamlValue::List(v.iter().map(|x| x.into()).collect())
            }
            BamlValueWithFlags::Map(_, _, m) => BamlValue::Map(
                m.into_iter()
                    .map(|(k, (_, v))| (k.clone(), v.into()))
                    .collect(),
            ),
            BamlValueWithFlags::Enum(s, _, v) => BamlValue::Enum(s.clone(), v.value.clone()),
            BamlValueWithFlags::Class(s, _, _, m) => BamlValue::Class(
                s.clone(),
                m.into_iter().map(|(k, v)| (k.clone(), v.into())).collect(),
            ),
            BamlValueWithFlags::Null(_, _) => BamlValue::Null,
            BamlValueWithFlags::Media(_, i) => BamlValue::Media(i.value.clone()),
        }
    }
}

impl BamlValueWithFlags {
    pub(super) fn add_flag(&mut self, flag: Flag) {
        match self {
            BamlValueWithFlags::String(v) => v.flags.add_flag(flag),
            BamlValueWithFlags::Int(v) => v.flags.add_flag(flag),
            BamlValueWithFlags::Float(v) => v.flags.add_flag(flag),
            BamlValueWithFlags::Bool(v) => v.flags.add_flag(flag),
            BamlValueWithFlags::List(v, _, _) => v.add_flag(flag),
            BamlValueWithFlags::Map(v, _, _) => v.add_flag(flag),
            BamlValueWithFlags::Enum(_, _, v) => v.flags.add_flag(flag),
            BamlValueWithFlags::Class(_, v, _, _) => v.add_flag(flag),
            BamlValueWithFlags::Null(_, v) => v.add_flag(flag),
            BamlValueWithFlags::Media(_, v) => v.flags.add_flag(flag),
        }
    }

    pub(super) fn r#type(&self) -> String {
        match self {
            BamlValueWithFlags::String(_) => "String".to_string(),
            BamlValueWithFlags::Int(_) => "Int".to_string(),
            BamlValueWithFlags::Float(_) => "Float".to_string(),
            BamlValueWithFlags::Bool(_) => "Bool".to_string(),
            BamlValueWithFlags::List(_, _, i) => {
                let inner = i
                    .iter()
                    .map(|i| i.r#type())
                    .collect::<HashSet<String>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(" | ");
                format!("List[{}:{inner}]", i.len())
            }
            BamlValueWithFlags::Map(_, _, _) => "Map".to_string(),
            BamlValueWithFlags::Enum(n, _, _) => format!("Enum {n}"),
            BamlValueWithFlags::Class(c, _, _, _) => format!("Class {c}"),
            BamlValueWithFlags::Null(_, _) => "Null".to_string(),
            BamlValueWithFlags::Media(_, _) => "Image".to_string(),
        }
    }
}

impl std::fmt::Display for BamlValueWithFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (Score: {}): ", self.r#type(), self.score())?;
        match self {
            BamlValueWithFlags::String(v) => {
                write!(f, "{}", v.value)?;
                if !v.flags.flags.is_empty() {
                    write!(f, "\n  {}", v.flags.to_string().replace("\n", "\n  "))?;
                }
            }
            BamlValueWithFlags::Int(v) => {
                write!(f, "{}", v.value)?;
                if !v.flags.flags.is_empty() {
                    write!(f, "\n  {}", v.flags.to_string().replace("\n", "\n  "))?;
                }
            }
            BamlValueWithFlags::Float(v) => {
                write!(f, "{}", v.value)?;
                if !v.flags.flags.is_empty() {
                    write!(f, "\n  {}", v.flags.to_string().replace("\n", "\n  "))?;
                }
            }
            BamlValueWithFlags::Bool(v) => {
                write!(f, "{}", v.value)?;
                if !v.flags.flags.is_empty() {
                    write!(f, "\n  {}", v.flags.to_string().replace("\n", "\n  "))?;
                }
            }
            BamlValueWithFlags::List(flags, _, v) => {
                writeln!(f)?;
                for (idx, item) in v.iter().enumerate() {
                    writeln!(f, "  {idx}: {}", item.to_string().replace("\n", "  \n"))?;
                }
                if !flags.flags.is_empty() {
                    writeln!(f, "  {}", flags.to_string().replace("\n", "\n  "))?;
                }
            }
            BamlValueWithFlags::Map(_, _, v) => {
                writeln!(f)?;
                for (key, value) in v {
                    writeln!(f, "{}: {}", key, value.1)?;
                }
            }
            BamlValueWithFlags::Enum(_, _, v) => {
                write!(f, "{}", v.value)?;
                if !v.flags.flags.is_empty() {
                    write!(f, "\n  {}", v.flags.to_string().replace("\n", "\n  "))?;
                }
            }
            BamlValueWithFlags::Class(_, flags, _, v) => {
                writeln!(f)?;
                for (k, v) in v.iter() {
                    writeln!(f, "  KV {}", k.to_string().replace("\n", "\n  "))?;
                    writeln!(f, "  {}", v.to_string().replace("\n", "\n  "))?;
                }
                if !flags.flags.is_empty() {
                    writeln!(f, "  {}", flags.to_string().replace("\n", "\n  "))?;
                }
            }
            BamlValueWithFlags::Null(_, flags) => {
                write!(f, "null")?;
                if !flags.flags.is_empty() {
                    write!(f, "\n  {}", flags.to_string().replace("\n", "\n  "))?;
                }
            }
            BamlValueWithFlags::Media(_, v) => {
                write!(f, "{:#?}", v.value)?;
                if !v.flags.flags.is_empty() {
                    write!(f, "\n  {}", v.flags.to_string().replace("\n", "\n  "))?;
                }
            }
        };

        Ok(())
    }
}

impl From<BamlValueWithFlags> for BamlValueWithMeta<Vec<(String, JinjaExpression, bool)>> {
    fn from(baml_value: BamlValueWithFlags) -> Self {
        use BamlValueWithFlags::*;
        let c = baml_value.conditions().constraint_results();
        match baml_value {
            String(ValueWithFlags { value, .. }) => BamlValueWithMeta::String(value, c),
            Int(ValueWithFlags { value, .. }) => BamlValueWithMeta::Int(value, c),
            Float(ValueWithFlags { value, .. }) => BamlValueWithMeta::Float(value, c),
            Bool(ValueWithFlags { value, .. }) => BamlValueWithMeta::Bool(value, c),
            Map(_, _, values) => BamlValueWithMeta::Map(
                values.into_iter().map(|(k, v)| (k, v.1.into())).collect(),
                c,
            ), // TODO: (Greg) I discard the DeserializerConditions tupled up with the value of the BamlMap. I'm not sure why BamlMap value is (DeserializerContitions, BamlValueWithFlags) in the first place.
            List(_, _, values) => {
                BamlValueWithMeta::List(values.into_iter().map(|v| v.into()).collect(), c)
            }
            Media(_, ValueWithFlags { value, .. }) => BamlValueWithMeta::Media(value, c),
            Enum(enum_name, _, ValueWithFlags { value, .. }) => {
                BamlValueWithMeta::Enum(enum_name, value, c)
            }
            Class(class_name, _, _, fields) => BamlValueWithMeta::Class(
                class_name,
                fields.into_iter().map(|(k, v)| (k, v.into())).collect(),
                c,
            ),
            Null(_, _) => BamlValueWithMeta::Null(c),
        }
    }
}
