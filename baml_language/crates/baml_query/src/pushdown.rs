use datafusion::logical_expr::{Expr, Operator};
use datafusion::scalar::ScalarValue;
use rusqlite::types::Value;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub(crate) struct SqlPredicate {
    pub sql: String,
    pub params: Vec<Value>,
}

pub(crate) fn to_sql_for_columns(
    expr: &Expr,
    resident_columns: &HashMap<String, String>,
) -> Option<SqlPredicate> {
    let mut params = Vec::new();
    let sql = build(expr, &mut params, resident_columns)?;
    Some(SqlPredicate { sql, params })
}

fn build(
    expr: &Expr,
    params: &mut Vec<Value>,
    resident_columns: &HashMap<String, String>,
) -> Option<String> {
    match expr {
        Expr::Alias(alias) => build(alias.expr.as_ref(), params, resident_columns),
        Expr::Column(column) => resident_columns.get(&column.name).cloned(),
        Expr::Literal(value, _) => {
            scalar_to_sqlite(value, params)?;
            Some("?".to_owned())
        }
        Expr::BinaryExpr(binary) => {
            let operator = match binary.op {
                Operator::Eq => "=",
                Operator::NotEq => "<>",
                Operator::LtEq => "<=",
                Operator::Lt => "<",
                Operator::GtEq => ">=",
                Operator::Gt => ">",
                Operator::And => "AND",
                Operator::Or => "OR",
                _ => return None,
            };
            let left = build(binary.left.as_ref(), params, resident_columns)?;
            let right = build(binary.right.as_ref(), params, resident_columns)?;
            Some(format!("({left} {operator} {right})"))
        }
        Expr::IsNull(value) => Some(format!(
            "({} IS NULL)",
            resident_operand(value, resident_columns)?
        )),
        Expr::IsNotNull(value) => Some(format!(
            "({} IS NOT NULL)",
            resident_operand(value, resident_columns)?
        )),
        Expr::InList(list) => {
            let expr = resident_operand(list.expr.as_ref(), resident_columns)?;
            if list.list.is_empty() {
                return None;
            }
            let values = list
                .list
                .iter()
                .map(|value| build(value, params, resident_columns))
                .collect::<Option<Vec<_>>>()?;
            let operator = if list.negated { "NOT IN" } else { "IN" };
            Some(format!("({expr} {operator} ({}))", values.join(", ")))
        }
        _ => None,
    }
}

fn resident_operand(expr: &Expr, resident_columns: &HashMap<String, String>) -> Option<String> {
    match expr {
        Expr::Column(column) => resident_columns.get(&column.name).cloned(),
        Expr::Alias(alias) => resident_operand(alias.expr.as_ref(), resident_columns),
        _ => None,
    }
}

fn scalar_to_sqlite(value: &ScalarValue, params: &mut Vec<Value>) -> Option<()> {
    let value = match value {
        ScalarValue::Null => Value::Null,
        ScalarValue::Boolean(Some(value)) => Value::Integer(i64::from(*value)),
        ScalarValue::Int8(Some(value)) => Value::Integer(i64::from(*value)),
        ScalarValue::Int16(Some(value)) => Value::Integer(i64::from(*value)),
        ScalarValue::Int32(Some(value)) => Value::Integer(i64::from(*value)),
        ScalarValue::Int64(Some(value)) => Value::Integer(*value),
        ScalarValue::UInt8(Some(value)) => Value::Integer(i64::from(*value)),
        ScalarValue::UInt16(Some(value)) => Value::Integer(i64::from(*value)),
        ScalarValue::UInt32(Some(value)) => Value::Integer(i64::from(*value)),
        ScalarValue::UInt64(Some(value)) => Value::Integer(i64::try_from(*value).ok()?),
        ScalarValue::Float32(Some(value)) => Value::Real(f64::from(*value)),
        ScalarValue::Float64(Some(value)) => Value::Real(*value),
        ScalarValue::Utf8(Some(value)) | ScalarValue::LargeUtf8(Some(value)) => {
            Value::Text(value.clone())
        }
        _ => return None,
    };
    params.push(value);
    Some(())
}
