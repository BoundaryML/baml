// Responsible for going from a Jsonish value to a schema value.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    hash::Hash,
};

use baml_types::BamlMap;

use crate::jsonish::Value;

#[derive(Debug, Clone)]
pub enum ValuePtr<'a> {
    Raw(&'a Value),
    Wrapped(&'a Value, &'a Value),
}

#[derive(Debug, Clone)]
pub enum RefValue<'a> {
    Single(ValuePtr<'a>),
    Multiple(Vec<ValuePtr<'a>>),
}

impl<'a> RefValue<'a> {
    fn merge(self, other: RefValue<'a>) -> Self {
        match (self, other) {
            (RefValue::Single(x), RefValue::Single(y)) => RefValue::Multiple(vec![x.clone(), y]),
            (RefValue::Single(x), RefValue::Multiple(y)) => {
                let mut y = y.clone();
                y.push(x.clone());
                RefValue::Multiple(y)
            }
            (RefValue::Multiple(mut x), RefValue::Single(y)) => {
                x.push(y);
                RefValue::Multiple(x)
            }
            (RefValue::Multiple(mut x), RefValue::Multiple(y)) => {
                x.extend(y);
                RefValue::Multiple(x)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Type<'a> {
    String(RefValue<'a>),
    Int(RefValue<'a>),
    Float(RefValue<'a>),
    Bool(RefValue<'a>),
    Null,
    EmptyArray,
    EmptyObject,
    // Containers with at least one type.
    Array(Box<Type<'a>>, RefValue<'a>),
    Object(BamlMap<String, Type<'a>>, RefValue<'a>),
    // Algebraic data types for the schema.
    Or(HashSet<Type<'a>>),
    And(HashSet<Type<'a>>),
}

impl<'a> Hash for Type<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            Type::EmptyArray
            | Type::EmptyObject
            | Type::Null
            | Type::Bool(_)
            | Type::Float(_)
            | Type::Int(_)
            | Type::String(_) => {}
            Type::Array(x, _) => x.hash(state),
            Type::Object(x, _) => x.iter().for_each(|(k, v)| {
                k.hash(state);
                v.hash(state);
            }),
            Type::Or(x) => x.iter().for_each(|v| v.hash(state)),
            Type::And(x) => x.iter().for_each(|v| v.hash(state)),
        }
    }
}

impl Eq for Type<'_> {}

impl PartialEq for Type<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Type::EmptyArray, Type::EmptyArray) => true,
            (Type::EmptyObject, Type::EmptyObject) => true,
            (Type::Null, Type::Null) => true,
            (Type::Bool(_), Type::Bool(_)) => true,
            (Type::Float(_), Type::Float(_)) => true,
            (Type::Int(_), Type::Int(_)) => true,
            (Type::String(_), Type::String(_)) => true,
            (Type::Array(x, _), Type::Array(y, _)) => x == y,
            (Type::Object(x, _), Type::Object(y, _)) => x == y,
            (Type::Or(x), Type::Or(y)) => x == y,
            (Type::And(x), Type::And(y)) => x == y,
            (y, Type::Or(x)) | (Type::Or(x), y) => x.len() == 1 && x.iter().next().unwrap() == y,
            _ => false,
        }
    }
}

impl<'a> Type<'a> {
    fn merge(self, other: Type<'a>) -> Self {
        // If both are arrays, make a new array with the merged type.
        return match (self, other) {
            (Type::Array(x, ref_val_x), Type::Array(y, ref_val_y)) => {
                // Arrays should be merged by merging their inner types.
                Type::Array(x.merge(*y).into(), ref_val_x.merge(ref_val_y))
            }
            (Type::Object(x, ref_val_x), Type::Object(y, ref_val_y)) => {
                // Objects should be merged by merging their inner types.
                let shared_keys = x.keys().filter(|k| y.contains_key(*k)).collect::<Vec<_>>();
                let x_only_keys = x.keys().filter(|k| !y.contains_key(*k)).collect::<Vec<_>>();
                let y_only_keys = y.keys().filter(|k| !x.contains_key(*k)).collect::<Vec<_>>();

                let shared = shared_keys
                    .into_iter()
                    .map(|k| {
                        (
                            k.clone(),
                            x.get(k).unwrap().clone().merge(y.get(k).unwrap().clone()),
                        )
                    })
                    .collect::<HashMap<String, Type<'a>>>();

                let x_only = x_only_keys
                    .into_iter()
                    .map(|k| (k.clone(), x.get(k).unwrap().clone()))
                    .collect::<HashMap<_, _>>();

                let y_only = y_only_keys
                    .into_iter()
                    .map(|k| (k.clone(), y.get(k).unwrap().clone()))
                    .collect::<HashMap<_, _>>();

                let ref_value_merged = ref_val_x.clone().merge(ref_val_y.clone());

                let shared = if shared.is_empty() {
                    None
                } else {
                    Some(Type::Object(shared, ref_value_merged))
                };

                let x_only = if x_only.is_empty() {
                    None
                } else {
                    Some(Type::Object(x_only, ref_val_x.clone()))
                };

                let y_only = if y_only.is_empty() {
                    None
                } else {
                    Some(Type::Object(y_only, ref_val_y.clone()))
                };

                let unique_keys = match (x_only, y_only) {
                    (Some(x_only), Some(y_only)) => Some(Type::And([x_only, y_only].into())),
                    (Some(x_only), None) => Some(x_only),
                    (None, Some(y_only)) => Some(y_only),
                    (None, None) => None,
                };

                match (shared, unique_keys) {
                    (Some(shared), Some(unique_keys)) => Type::And([shared, unique_keys].into()),
                    (Some(shared), None) => shared,
                    (None, Some(unique_keys)) => unique_keys,
                    (None, None) => {
                        unreachable!("At least one of shared or unique_keys should be Some")
                    }
                }
            }
            (Type::Or(mut x), Type::Or(y)) => {
                x.extend(y);
                Type::Or(x)
            }
            (y, Type::Or(mut x)) | (Type::Or(mut x), y) => {
                x.insert(y);
                Type::Or(x)
            }
            (Type::And(mut x), Type::And(y)) => {
                x.extend(y);
                Type::And(x)
            }
            (y, Type::And(mut x)) | (Type::And(mut x), y) => {
                x.insert(y);
                Type::And(x)
            }
            (Type::String(x), Type::String(y)) => Type::String(x.merge(y)),
            (Type::Int(x), Type::Int(y)) => Type::Int(x.merge(y)),
            (Type::Float(x), Type::Float(y)) => Type::Float(x.merge(y)),
            (Type::Bool(x), Type::Bool(y)) => Type::Bool(x.merge(y)),
            (Type::Null, Type::Null) => Type::Null,
            (Type::EmptyArray, Type::EmptyArray) => Type::EmptyArray,
            (Type::EmptyObject, Type::EmptyObject) => Type::EmptyObject,
            (s, o) => Type::Or([s, o].into()),
        };
    }
}

pub fn from_jsonish_value<'a>(value: &'a Value, original: Option<&'a Value>) -> Type<'a> {
    let ref_value = match original {
        Some(original) => RefValue::Single(ValuePtr::Wrapped(value, original)),
        None => RefValue::Single(ValuePtr::Raw(value)),
    };

    match value {
        Value::String(_) => Type::String(ref_value),
        Value::Number(x) => {
            if x.is_f64() {
                Type::Float(ref_value)
            } else {
                Type::Int(ref_value)
            }
        }
        Value::Boolean(_) => Type::Bool(ref_value),
        Value::Null => Type::Null,
        Value::Object(kv) => {
            if kv.is_empty() {
                return Type::EmptyObject;
            }
            let kv = kv
                .iter()
                .map(|(k, v)| (k.clone(), from_jsonish_value(v, original)))
                .collect();
            Type::Object(kv, ref_value)
        }
        Value::Array(items) => {
            if items.is_empty() {
                return Type::EmptyArray;
            }
            let mut items = items
                .iter()
                .map(|x| from_jsonish_value(x, original))
                .collect::<VecDeque<_>>();
            // Merge all the items into a single type.
            let mut merged = items.pop_front().unwrap();
            while let Some(item) = items.pop_front() {
                merged = merged.merge(item);
            }
            Type::Array(merged.into(), ref_value)
        }
        Value::Markdown(_, inner) => from_jsonish_value(inner, Some(original.unwrap_or(value))),
        Value::FixedJson(inner, _) => from_jsonish_value(inner, Some(original.unwrap_or(value))),
        Value::AnyOf(items, _) => {
            if items.is_empty() {
                return Type::EmptyArray;
            }
            let mut items = items
                .iter()
                .map(|x| from_jsonish_value(x, original))
                .collect::<VecDeque<_>>();
            // Merge all the items into a single type.
            let mut merged = items.pop_front().unwrap();
            while let Some(item) = items.pop_front() {
                merged = merged.merge(item);
            }
            merged
        }
    }
}
