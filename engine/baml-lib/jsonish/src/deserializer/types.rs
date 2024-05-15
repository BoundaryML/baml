use std::{collections::HashSet};

use baml_types::{BamlImage, BamlMap, BamlValue};

use super::{
    deserialize_flags::{DeserializerConditions, Flag},
    score::WithScore,
};

// Recursive parity
#[derive(Clone, Debug)]
pub enum BamlValueWithFlags {
    String(ValueWithFlags<String>),
    Int(ValueWithFlags<i64>),
    Float(ValueWithFlags<f64>),
    Bool(ValueWithFlags<bool>),
    List(DeserializerConditions, Vec<BamlValueWithFlags>),
    Map(
        DeserializerConditions,
        BamlMap<String, (DeserializerConditions, BamlValueWithFlags)>,
    ),
    Enum(String, ValueWithFlags<String>),
    Class(
        String,
        DeserializerConditions,
        BamlMap<String, BamlValueWithFlags>,
    ),
    Null(DeserializerConditions),
    Image(ValueWithFlags<BamlImage>),
}

impl BamlValueWithFlags {
    pub fn score(&self) -> i32 {
        match self {
            BamlValueWithFlags::String(f) => f.score(),
            BamlValueWithFlags::Int(f) => f.score(),
            BamlValueWithFlags::Float(f) => f.score(),
            BamlValueWithFlags::Bool(f) => f.score(),
            BamlValueWithFlags::List(f, items) => {
                f.score() + items.iter().map(|i| i.score()).sum::<i32>()
            }
            BamlValueWithFlags::Map(f, kv) => {
                f.score()
                    + kv.iter()
                        .map(|(_, (f, v))| f.score() + v.score())
                        .sum::<i32>()
            }
            BamlValueWithFlags::Enum(_, f) => f.score(),
            BamlValueWithFlags::Class(_, f, items) => {
                f.score() + items.iter().map(|(_, v)| v.score()).sum::<i32>()
            }
            BamlValueWithFlags::Null(f) => f.score(),
            BamlValueWithFlags::Image(f) => f.score(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ValueWithFlags<T> {
    value: T,
    pub(super) flags: DeserializerConditions,
}

impl<T> From<T> for ValueWithFlags<T> {
    fn from(item: T) -> Self {
        ValueWithFlags {
            value: item,
            flags: DeserializerConditions::new(),
        }
    }
}

impl<T> From<(T, &[Flag])> for ValueWithFlags<T> {
    fn from((value, flags): (T, &[Flag])) -> Self {
        let flags = flags
            .iter()
            .fold(DeserializerConditions::new(), |acc, flag| {
                acc.with_flag(flag.to_owned())
            });
        ValueWithFlags { value, flags }
    }
}

impl<T> From<(T, Flag)> for ValueWithFlags<T> {
    fn from((value, flag): (T, Flag)) -> Self {
        ValueWithFlags {
            value,
            flags: DeserializerConditions::new().with_flag(flag),
        }
    }
}

impl<T> From<(T, DeserializerConditions)> for ValueWithFlags<T> {
    fn from((value, flags): (T, DeserializerConditions)) -> Self {
        ValueWithFlags { value, flags }
    }
}

impl From<BamlValueWithFlags> for BamlValue {
    fn from(value: BamlValueWithFlags) -> BamlValue {
        match value {
            BamlValueWithFlags::String(s) => BamlValue::String(s.value),
            BamlValueWithFlags::Int(i) => BamlValue::Int(i.value),
            BamlValueWithFlags::Float(f) => BamlValue::Float(f.value),
            BamlValueWithFlags::Bool(b) => BamlValue::Bool(b.value),
            BamlValueWithFlags::List(_, v) => {
                BamlValue::List(v.into_iter().map(|x| x.into()).collect())
            }
            BamlValueWithFlags::Map(_, m) => {
                BamlValue::Map(m.into_iter().map(|(k, (_, v))| (k, v.into())).collect())
            }
            BamlValueWithFlags::Enum(s, v) => BamlValue::Enum(s, v.value),
            BamlValueWithFlags::Class(s, _, m) => {
                BamlValue::Class(s, m.into_iter().map(|(k, v)| (k, v.into())).collect())
            }
            BamlValueWithFlags::Null(_) => BamlValue::Null,
            BamlValueWithFlags::Image(i) => BamlValue::Image(i.value),
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
            BamlValueWithFlags::List(_, v) => {
                BamlValue::List(v.into_iter().map(|x| x.into()).collect())
            }
            BamlValueWithFlags::Map(_, m) => BamlValue::Map(
                m.into_iter()
                    .map(|(k, (_, v))| (k.clone(), v.into()))
                    .collect(),
            ),
            BamlValueWithFlags::Enum(s, v) => BamlValue::Enum(s.clone(), v.value.clone()),
            BamlValueWithFlags::Class(s, _, m) => BamlValue::Class(
                s.clone(),
                m.into_iter().map(|(k, v)| (k.clone(), v.into())).collect(),
            ),
            BamlValueWithFlags::Null(_) => BamlValue::Null,
            BamlValueWithFlags::Image(i) => BamlValue::Image(i.value.clone()),
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
            BamlValueWithFlags::List(v, _) => v.add_flag(flag),
            BamlValueWithFlags::Map(v, _) => v.add_flag(flag),
            BamlValueWithFlags::Enum(_, v) => v.flags.add_flag(flag),
            BamlValueWithFlags::Class(_, v, _) => v.add_flag(flag),
            BamlValueWithFlags::Null(v) => v.add_flag(flag),
            BamlValueWithFlags::Image(v) => v.flags.add_flag(flag),
        }
    }

    fn r#type(&self) -> String {
        match self {
            BamlValueWithFlags::String(_) => "String".to_string(),
            BamlValueWithFlags::Int(_) => "Int".to_string(),
            BamlValueWithFlags::Float(_) => "Float".to_string(),
            BamlValueWithFlags::Bool(_) => "Bool".to_string(),
            BamlValueWithFlags::List(_, i) => {
                let inner = i
                    .iter()
                    .map(|i| i.r#type())
                    .collect::<HashSet<String>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(" | ");
                format!("List[{inner}]")
            }
            BamlValueWithFlags::Map(_, _) => "Map".to_string(),
            BamlValueWithFlags::Enum(n, _) => format!("Enum {n}"),
            BamlValueWithFlags::Class(c, _, _) => format!("Class {c}"),
            BamlValueWithFlags::Null(_) => "Null".to_string(),
            BamlValueWithFlags::Image(_) => "Image".to_string(),
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
            BamlValueWithFlags::List(flags, v) => {
                write!(f, "\n")?;
                for (idx, item) in v.iter().enumerate() {
                    writeln!(f, "  {idx}: {}", item.to_string().replace("\n", "  \n"))?;
                }
                if !flags.flags.is_empty() {
                    writeln!(f, "  {}", flags.to_string().replace("\n", "\n  "))?;
                }
            }
            BamlValueWithFlags::Map(_, v) => {
                write!(f, "\n")?;
                for (key, value) in v {
                    writeln!(f, "{}: {}", key, value.1)?;
                }
            }
            BamlValueWithFlags::Enum(_n, v) => {
                write!(f, "{}", v.value)?;
                if !v.flags.flags.is_empty() {
                    write!(f, "\n  {}", v.flags.to_string().replace("\n", "\n  "))?;
                }
            }
            BamlValueWithFlags::Class(_, flags, v) => {
                write!(f, "\n")?;
                for (_idx, (k, v)) in v.iter().enumerate() {
                    writeln!(f, "  KV {}", k.to_string().replace("\n", "\n  "))?;
                    writeln!(f, "  {}", v.to_string().replace("\n", "\n  "))?;
                }
                if !flags.flags.is_empty() {
                    writeln!(f, "  {}", flags.to_string().replace("\n", "\n  "))?;
                }
            }
            BamlValueWithFlags::Null(flags) => {
                write!(f, "null")?;
                if !flags.flags.is_empty() {
                    write!(f, "\n  {}", flags.to_string().replace("\n", "\n  "))?;
                }
            }
            BamlValueWithFlags::Image(v) => {
                write!(f, "{:#?}", v.value)?;
                if !v.flags.flags.is_empty() {
                    write!(f, "\n  {}", v.flags.to_string().replace("\n", "\n  "))?;
                }
            }
        };

        Ok(())
    }
}
