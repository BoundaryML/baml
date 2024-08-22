use anyhow::Result;
use std::collections::HashMap;

use internal_baml_core::ir::repr::Expression;
use serde_json::json;

use crate::RuntimeContext;

pub fn to_value(ctx: &RuntimeContext, expr: &Expression) -> Result<serde_json::Value> {
    // serde_json::Value::serialize(&self.into(), serializer)
    Ok(match expr {
        Expression::Identifier(idn) => match idn {
            internal_baml_core::ir::repr::Identifier::ENV(key) => match ctx.env.get(key) {
                None => anyhow::bail!("unset env variable '{}'", key),
                Some(val) => serde_json::Value::String(val.to_string()),
            },
            _ => serde_json::Value::String(idn.name()),
        },
        Expression::Bool(b) => serde_json::Value::Bool(*b),
        Expression::Numeric(n) => serde_json::Value::Number(n.parse().unwrap()),
        Expression::String(s) => serde_json::Value::String(s.clone()),
        Expression::RawString(s) => serde_json::Value::String(s.to_string()),
        Expression::List(items) => serde_json::Value::Array(
            items
                .iter()
                .map(|item| to_value(ctx, item))
                .collect::<Result<Vec<_>>>()?,
        ),
        Expression::Map(kv) => {
            let res = kv
                .iter()
                .map(|(k, v)| {
                    let k = match k {
                        Expression::String(s) => s.clone(),
                        Expression::Identifier(internal_baml_core::ir::repr::Identifier::ENV(
                            key,
                        )) => match ctx.env.get(key) {
                            None => anyhow::bail!("unset env variable '{}'", key),
                            Some(val) => val.to_string(),
                        },
                        Expression::Identifier(
                            internal_baml_core::ir::repr::Identifier::Local(key),
                        ) => key.clone(),
                        Expression::Identifier(internal_baml_core::ir::repr::Identifier::Ref(
                            key,
                        )) => key.join("."),
                        _ => anyhow::bail!("invalid key {:#?}", k),
                    };
                    let v = to_value(ctx, v)?;
                    Ok((k, v))
                })
                .collect::<Result<HashMap<_, _>>>()?;
            json!(res)
        }
        Expression::JinjaExpression(_expr) => panic!("TODO"),
    })
}
