use std::collections::HashSet;

use baml_types::{BamlMap, BamlMedia, BamlValue};

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
    Media(ValueWithFlags<BamlMedia>),
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
            BamlValueWithFlags::Media(f) => f.score(),
        }
    }

    pub fn conditions(&self) -> &DeserializerConditions {
        match self {
            BamlValueWithFlags::String(v) => &v.flags,
            BamlValueWithFlags::Int(v) => &v.flags,
            BamlValueWithFlags::Float(v) => &v.flags,
            BamlValueWithFlags::Bool(v) => &v.flags,
            BamlValueWithFlags::List(v, _) => &v,
            BamlValueWithFlags::Map(v, _) => &v,
            BamlValueWithFlags::Enum(_, v) => &v.flags,
            BamlValueWithFlags::Class(_, v, _) => &v,
            BamlValueWithFlags::Null(v) => &v,
            BamlValueWithFlags::Media(v) => &v.flags,
        }
    }

    pub fn explanation(&self) -> String {
        let mut expl = vec![];
        self.explanation_impl(vec!["".to_string()], &mut expl);
        match expl.len() {
            0 => "".to_string(),
            1 => format!("Error while parsing:\n\n{}", expl[0]),
            _ => format!("Errors while parsing:\n\n{}", expl.join("\n")),
        }
    }

    pub fn explanation_impl(&self, scope: Vec<String>, expls: &mut Vec<String>) {
        // String(ValueWithFlags<String>),
        // Int(ValueWithFlags<i64>),
        // Float(ValueWithFlags<f64>),
        // Bool(ValueWithFlags<bool>),
        // List(DeserializerConditions, Vec<BamlValueWithFlags>),
        // Map(
        //     DeserializerConditions,
        //     BamlMap<String, (DeserializerConditions, BamlValueWithFlags)>,
        // ),
        // Enum(String, ValueWithFlags<String>),
        // Class(
        //     String,
        //     DeserializerConditions,
        //     BamlMap<String, BamlValueWithFlags>,
        // ),
        // Null(DeserializerConditions),
        // Image(ValueWithFlags<BamlMedia>),
        match self {
            BamlValueWithFlags::String(v) => match v.flags.explanation() {
                Some(expl) => expls.push(format!(
                    "{} - error while parsing string: {}",
                    scope.join("."),
                    expl
                )),
                None => {}
            },
            BamlValueWithFlags::Int(v) => match v.flags.explanation() {
                Some(expl) => expls.push(format!(
                    "{} - error while parsing int: {}",
                    scope.join("."),
                    expl
                )),
                None => {}
            },
            BamlValueWithFlags::Float(v) => match v.flags.explanation() {
                Some(expl) => expls.push(format!(
                    "{} - error while parsing float: {}",
                    scope.join("."),
                    expl
                )),
                None => {}
            },
            BamlValueWithFlags::Bool(v) => match v.flags.explanation() {
                Some(expl) => expls.push(format!(
                    "{} - error while parsing bool: {}",
                    scope.join("."),
                    expl
                )),
                None => {}
            },

            BamlValueWithFlags::List(flags, values) => {
                match flags.explanation() {
                    Some(expl) => expls.push(format!(
                        "{} - error while parsing list: {}",
                        scope.join("."),
                        expl
                    )),
                    None => {}
                }
                for (i, value) in values.iter().enumerate() {
                    let mut scope = scope.clone();
                    scope.push(format!("{}", i));
                    value.explanation_impl(scope, expls);
                }
            }
            BamlValueWithFlags::Map(flags, kv) => {
                match flags.explanation() {
                    Some(expl) => expls.push(format!(
                        "{} - error while parsing map: {}",
                        scope.join("."),
                        expl
                    )),
                    None => {}
                }
                for (k, (v_flags, v)) in kv.iter() {
                    let mut scope = scope.clone();
                    scope.push(format!("{}", k));
                    match v_flags.explanation() {
                        Some(expl) => expls.push(format!(
                            "{} - error while parsing map: {}",
                            scope.join("."),
                            expl
                        )),
                        None => {}
                    }
                    v.explanation_impl(scope, expls);
                }
            }
            BamlValueWithFlags::Enum(_, v) => match v.flags.explanation() {
                Some(expl) => expls.push(format!(
                    "{} - error while parsing enum: {}",
                    scope.join("."),
                    expl
                )),
                None => {}
            },
            BamlValueWithFlags::Class(class_name, v, fields) => {
                match v.explanation() {
                    Some(expl) => expls.push(format!(
                        "{} - error while parsing class {}: {}",
                        scope.join("."),
                        class_name,
                        expl
                    )),
                    None => {}
                }
                for (k, v) in fields.iter() {
                    let mut scope = scope.clone();
                    scope.push(format!("{}", k));
                    v.explanation_impl(scope, expls);
                }
            }

            BamlValueWithFlags::Null(v) => match v.explanation() {
                Some(expl) => expls.push(format!(
                    "{} - error while parsing null: {}",
                    scope.join("."),
                    expl
                )),
                None => {}
            },
            BamlValueWithFlags::Media(v) => match v.flags.explanation() {
                Some(expl) => expls.push(format!(
                    "{} - error while parsing media: {}",
                    scope.join("."),
                    expl
                )),
                None => {}
            },
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
            BamlValueWithFlags::Media(i) => BamlValue::Media(i.value),
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
            BamlValueWithFlags::Media(i) => BamlValue::Media(i.value.clone()),
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
            BamlValueWithFlags::Media(v) => v.flags.add_flag(flag),
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
                format!("List[{}:{inner}]", i.len())
            }
            BamlValueWithFlags::Map(_, _) => "Map".to_string(),
            BamlValueWithFlags::Enum(n, _) => format!("Enum {n}"),
            BamlValueWithFlags::Class(c, _, _) => format!("Class {c}"),
            BamlValueWithFlags::Null(_) => "Null".to_string(),
            BamlValueWithFlags::Media(_) => "Image".to_string(),
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
            BamlValueWithFlags::Media(v) => {
                write!(f, "{:#?}", v.value)?;
                if !v.flags.flags.is_empty() {
                    write!(f, "\n  {}", v.flags.to_string().replace("\n", "\n  "))?;
                }
            }
        };

        Ok(())
    }
}
