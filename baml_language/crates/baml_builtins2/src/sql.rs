use num_bigint::BigInt;

/// A fully-owned SQL bind captured when a tagged statement is constructed.
/// No variant contains a VM heap reference, so statements remain valid across
/// GC and cannot observe later mutation of source arrays or JSON values.
#[derive(Clone, Debug, PartialEq)]
pub enum SqlBindValue {
    Null,
    Bool(bool),
    Int(i64),
    BigInt(BigInt),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Json(serde_json::Value),
    Instant(BigInt),
    ZonedDateTime {
        epoch_nanoseconds: BigInt,
        offset_nanoseconds: Option<i64>,
        iana: Option<String>,
    },
    PlainDateTime(BigInt),
    PlainDate(i64),
    PlainTime(i64),
    Duration(BigInt),
    Array {
        element_type: SqlArrayType,
        values: Vec<SqlBindValue>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlArrayType {
    Bool,
    Int,
    BigInt,
    Float,
    String,
    Bytes,
    Json,
    Instant,
    ZonedDateTime,
    PlainDateTime,
    PlainDate,
    PlainTime,
    Duration,
}

/// Opaque, reusable representation of exactly one tagged SQL statement.
#[derive(Clone, Debug, PartialEq)]
pub struct SqlStatement {
    pub parts: Vec<String>,
    pub values: Vec<SqlBindValue>,
}

impl SqlStatement {
    pub fn render_postgres(&self) -> String {
        render(&self.parts, |index| format!("${}", index + 1))
    }

    pub fn render_sqlite(&self) -> String {
        render(&self.parts, |_| "?".to_string())
    }
}

fn render(parts: &[String], placeholder: impl Fn(usize) -> String) -> String {
    let extra = parts.iter().map(String::len).sum::<usize>() + parts.len().saturating_mul(3);
    let mut sql = String::with_capacity(extra);
    for (index, part) in parts.iter().enumerate() {
        sql.push_str(part);
        if index + 1 < parts.len() {
            sql.push_str(&placeholder(index));
        }
    }
    sql
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_placeholders_only_between_literal_parts() {
        let statement = SqlStatement {
            parts: vec!["SELECT '$1', ? /* ".into(), " */ + ".into(), String::new()],
            values: vec![SqlBindValue::Int(1), SqlBindValue::Int(1)],
        };
        assert_eq!(statement.render_postgres(), "SELECT '$1', ? /* $1 */ + $2");
        assert_eq!(statement.render_sqlite(), "SELECT '$1', ? /* ? */ + ?");
    }
}
