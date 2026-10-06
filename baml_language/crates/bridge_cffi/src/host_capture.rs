//! Decode bounded observations copied by host adapters, never live host objects.
use btel_snapshot::{
    Limit,
    host::{HostType, HostValue, MAX_BYTES, MAX_DEPTH, MAX_VALUES},
};
use serde_json::Value;

pub fn decode(wire: &str) -> HostValue {
    if wire.len() > MAX_BYTES * 8 {
        return HostValue::Truncated(Limit::Bytes);
    }
    serde_json::from_str::<Value>(wire).map_or(HostValue::Unavailable, |value| {
        let mut remaining = MAX_VALUES;
        copy(&value, 0, &mut remaining)
    })
}

fn parts(value: &Value) -> Option<(&str, &[Value])> {
    let values = value.as_array()?;
    Some((values.first()?.as_str()?, &values[1..]))
}

fn pairs(value: &Value, depth: usize, remaining: &mut usize) -> Option<Vec<(String, HostValue)>> {
    let entries = value.as_array()?;
    if entries.len() > *remaining {
        return None;
    }
    let mut result = Vec::with_capacity(entries.len());
    for entry in entries {
        let pair = entry.as_array()?;
        let key = pair.first()?.as_str()?;
        result.push((key.into(), copy(pair.get(1)?, depth + 1, remaining)));
    }
    Some(result)
}

fn copy(value: &Value, depth: usize, remaining: &mut usize) -> HostValue {
    if depth > MAX_DEPTH {
        return HostValue::Truncated(Limit::Depth);
    }
    if *remaining == 0 {
        return HostValue::Truncated(Limit::Values);
    }
    *remaining -= 1;
    let Some((tag, payload)) = parts(value) else {
        return HostValue::Unavailable;
    };
    let first = payload.first().unwrap_or(&Value::Null);
    match tag {
        "null" => HostValue::Null,
        "bool" => first
            .as_bool()
            .map_or(HostValue::Unavailable, HostValue::Bool),
        "number" => first.as_i64().map_or_else(
            || {
                first
                    .as_f64()
                    .map_or(HostValue::Unavailable, HostValue::Float)
            },
            HostValue::Int,
        ),
        "string" => first
            .as_str()
            .map_or(HostValue::Unavailable, |v| HostValue::String(v.into())),
        "list" => first.as_array().map_or(HostValue::Unavailable, |values| {
            if values.len() > *remaining {
                return HostValue::Truncated(Limit::Values);
            }
            HostValue::List(
                values
                    .iter()
                    .map(|v| copy(v, depth + 1, remaining))
                    .collect(),
            )
        }),
        "map" => pairs(first, depth, remaining).map_or(HostValue::Unavailable, HostValue::Map),
        "class" => {
            let Some(name) = first.as_str() else {
                return HostValue::Unavailable;
            };
            let Some(fields) = payload.get(1).and_then(|v| pairs(v, depth, remaining)) else {
                return HostValue::Unavailable;
            };
            let mut type_args = Vec::new();
            if let Some(args) = payload.get(2).and_then(Value::as_array) {
                for ty in args {
                    let Some(ty) = copy_type(ty, depth + 1, remaining) else {
                        return HostValue::Truncated(Limit::Values);
                    };
                    type_args.push(ty);
                }
            }
            HostValue::Instance {
                name: name.into(),
                type_args,
                fields,
            }
        }
        "enum" => match (first.as_str(), payload.get(1).and_then(Value::as_str)) {
            (Some(name), Some(variant)) => HostValue::Enum {
                name: name.into(),
                variant: variant.into(),
            },
            _ => HostValue::Unavailable,
        },
        "depth" => HostValue::Truncated(Limit::Depth),
        "values" => HostValue::Truncated(Limit::Values),
        "bytes" => HostValue::Truncated(Limit::Bytes),
        _ => HostValue::Unavailable,
    }
}

fn copy_type(value: &Value, depth: usize, remaining: &mut usize) -> Option<HostType> {
    if depth > MAX_DEPTH || *remaining == 0 {
        return None;
    }
    *remaining -= 1;
    let (tag, payload) = parts(value)?;
    Some(match tag {
        "int" => HostType::Int,
        "float" => HostType::Float,
        "bool" => HostType::Bool,
        "string" => HostType::String,
        "null" => HostType::Null,
        "list" => HostType::List(Box::new(copy_type(payload.first()?, depth + 1, remaining)?)),
        "map" => HostType::Map {
            key: Box::new(copy_type(payload.first()?, depth + 1, remaining)?),
            value: Box::new(copy_type(payload.get(1)?, depth + 1, remaining)?),
        },
        "union" => HostType::Union(
            payload
                .first()?
                .as_array()?
                .iter()
                .map(|v| copy_type(v, depth + 1, remaining))
                .collect::<Option<Vec<_>>>()?
                .into(),
        ),
        "class" => HostType::Class(
            payload.first()?.as_str()?.into(),
            payload
                .get(1)?
                .as_array()?
                .iter()
                .map(|v| copy_type(v, depth + 1, remaining))
                .collect::<Option<Vec<_>>>()?
                .into(),
        ),
        "enum" => HostType::Enum(payload.first()?.as_str()?.into()),
        _ => HostType::Unknown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapters_share_nominal_and_nested_observations() {
        let value = decode(
            r#"["class","user.Box",[["value",["enum","user.Mood","HAPPY"]]],[["list",["int"]]]]"#,
        );
        let HostValue::Instance {
            name,
            fields,
            type_args,
        } = value
        else {
            panic!("class")
        };
        assert_eq!(name, "user.Box");
        assert_eq!(type_args, vec![HostType::List(Box::new(HostType::Int))]);
        assert!(
            matches!(&fields[0].1, HostValue::Enum { name, variant } if name == "user.Mood" && variant == "HAPPY")
        );
    }

    #[test]
    fn malformed_or_over_budget_wire_values_fail_closed() {
        for wire in ["", "[]", r#"["class",7,[]]"#, r#"["map",[[7,["null"]]]]"#] {
            assert!(matches!(decode(wire), HostValue::Unavailable));
        }
        assert!(matches!(
            decode(&"x".repeat(MAX_BYTES * 8 + 1)),
            HostValue::Truncated(Limit::Bytes)
        ));
        let wire = format!(
            "[\"list\",[{}]]",
            vec!["[\"null\"]"; MAX_VALUES + 1].join(",")
        );
        assert!(matches!(decode(&wire), HostValue::Truncated(Limit::Values)));
    }
}
