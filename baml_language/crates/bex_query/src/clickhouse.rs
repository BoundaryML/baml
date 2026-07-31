//! Launch-time compiler for the BQL aggregate subset.
//!
//! The compiler intentionally targets only tables that can preserve local CCT
//! semantics. Exact calls, values, and events fail closed instead of silently
//! compiling against an approximate cloud table.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    QueryError,
    bql::{CompareOp, Pipeline, StageCall, Value, parse_and_plan},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClickHouseParamType {
    String,
    UInt64,
    Int64,
}

impl ClickHouseParamType {
    const fn sql_name(&self) -> &'static str {
        match self {
            Self::String => "String",
            Self::UInt64 => "UInt64",
            Self::Int64 => "Int64",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClickHouseParam {
    pub name: String,
    pub kind: ClickHouseParamType,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledClickHouseQuery {
    pub statement_name: Option<String>,
    pub sql: String,
    pub params: Vec<ClickHouseParam>,
    /// Storage tables whose schemas the cloud API must preserve.
    pub required_tables: Vec<String>,
}

#[derive(Clone, Debug, Default)]
struct SelectShape {
    rollup: bool,
    series_bucket_ns: Option<u64>,
    selected_fields: Option<Vec<String>>,
}

#[derive(Clone, Debug)]
struct SqlBuilder {
    source: Source,
    filters: Vec<String>,
    having: Vec<String>,
    order_by: Vec<String>,
    limit: usize,
    params: Vec<ClickHouseParam>,
    shape: SelectShape,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Runs,
    Contexts,
}

/// Compile all BQL statements in `source` to parameterized ClickHouse SQL.
pub fn compile_clickhouse(
    source: &str,
    bind_params: &BTreeMap<String, String>,
) -> Result<Vec<CompiledClickHouseQuery>, QueryError> {
    let (script, _) = parse_and_plan(source, bind_params)?;
    script
        .statements
        .iter()
        .map(|statement| {
            compile_pipeline(&statement.pipeline).map(|mut compiled| {
                compiled.statement_name.clone_from(&statement.name);
                compiled
            })
        })
        .collect()
}

fn compile_pipeline(pipeline: &Pipeline) -> Result<CompiledClickHouseQuery, QueryError> {
    let first = pipeline
        .stages
        .first()
        .ok_or_else(|| QueryError::invalid_request("empty BQL pipeline"))?;
    let source = match first.name.as_str() {
        "ctx" => Source::Contexts,
        "runs" | "run" => Source::Runs,
        other => {
            return Err(unsupported(
                other,
                "aggregate source must be ctx(), runs(), or run()",
            ));
        }
    };
    let mut builder = SqlBuilder {
        source,
        filters: Vec::new(),
        having: Vec::new(),
        order_by: Vec::new(),
        limit: 1_000,
        params: Vec::new(),
        shape: SelectShape::default(),
    };
    compile_source(first, &mut builder)?;
    for stage in pipeline.stages.iter().skip(1) {
        compile_stage(stage, &mut builder)?;
    }
    builder.finish()
}

fn compile_source(stage: &StageCall, builder: &mut SqlBuilder) -> Result<(), QueryError> {
    match stage.name.as_str() {
        "ctx" | "runs" => {
            if let Some(last) = stage.named("last").and_then(Value::as_str) {
                let duration_ns = parse_duration_ns(last)?;
                let parameter = builder.param(ClickHouseParamType::UInt64, duration_ns.to_string());
                let clock = if builder.source == Source::Runs {
                    "r.created_ms * 1000000"
                } else {
                    "c.window_end_ns"
                };
                builder.filters.push(format!(
                    "{clock} >= toUnixTimestamp64Nano(now64(9)) - {parameter}"
                ));
            }
            if let Some(revision) = stage.named("rev").and_then(Value::as_str) {
                if builder.source == Source::Runs {
                    let parameter = builder.param(ClickHouseParamType::String, revision.to_owned());
                    builder.filters.push(format!("r.revision_id = {parameter}"));
                } else {
                    let parameter = builder.param(ClickHouseParamType::String, revision.to_owned());
                    builder.filters.push(format!("c.revision_id = {parameter}"));
                }
            }
            if let Some(limit) = stage.named("limit").and_then(Value::as_u64) {
                builder.limit = checked_limit(limit)?;
            }
        }
        "run" => {
            let id = stage
                .named("id")
                .or_else(|| stage.positional(0))
                .and_then(Value::as_str)
                .ok_or_else(|| QueryError::invalid_request("run() requires a boundary id"))?;
            let parameter = builder.param(ClickHouseParamType::String, id.to_owned());
            builder.filters.push(format!("r.boundary_id = {parameter}"));
            builder.limit = 1;
        }
        _ => unreachable!("source checked by caller"),
    }
    Ok(())
}

fn compile_stage(stage: &StageCall, builder: &mut SqlBuilder) -> Result<(), QueryError> {
    match stage.name.as_str() {
        "calls" => {
            builder.promote_runs_to_contexts();
            if let Some(function) = stage.named("fn").and_then(Value::as_str) {
                let pattern = glob_to_re2(function);
                let parameter = builder.param(ClickHouseParamType::String, pattern);
                builder.filters.push(format!("match(d.fqn, {parameter})"));
            }
            if let Some(path) = stage.named("path").and_then(Value::as_str) {
                let parameter = builder.param(ClickHouseParamType::String, path.to_owned());
                builder
                    .filters
                    .push(format!("has(c.canonical_path, {parameter})"));
            }
            if let Some(kind) = stage.named("kind").and_then(Value::as_str) {
                if kind != "llm" {
                    return Err(unsupported(
                        "calls",
                        "ClickHouse aggregate kind supports only llm",
                    ));
                }
                builder.filters.push("c.llm_calls > 0".to_owned());
            }
        }
        "errors" => match builder.source {
            Source::Runs => builder
                .filters
                .push("r.completion_status IN ('errored', 'crashed')".to_owned()),
            Source::Contexts => builder
                .filters
                .push("(c.ends_err + c.ends_cancel + c.ends_exit) > 0".to_owned()),
        },
        "where" => compile_where(stage, builder)?,
        "rollup" => {
            builder.promote_runs_to_contexts();
            builder.shape.rollup = true;
        }
        "series" => {
            builder.promote_runs_to_contexts();
            let bucket = stage
                .named("bucket")
                .or_else(|| stage.positional(0))
                .and_then(Value::as_str)
                .ok_or_else(|| QueryError::invalid_request("series() requires bucket"))?;
            builder.shape.series_bucket_ns = Some(parse_duration_ns(bucket)?);
        }
        "top" => {
            let limit = stage
                .named("k")
                .or_else(|| stage.positional(0))
                .and_then(Value::as_u64)
                .ok_or_else(|| QueryError::invalid_request("top() requires integer k"))?;
            builder.limit = checked_limit(limit)?;
            let metric = stage
                .named("by")
                .and_then(Value::as_str)
                .unwrap_or("total_ns");
            builder
                .order_by
                .push(format!("{} DESC", output_field(metric)?));
        }
        "sort" => {
            let field = stage
                .named("by")
                .or_else(|| stage.positional(0))
                .and_then(Value::as_str)
                .ok_or_else(|| QueryError::invalid_request("sort() requires by"))?;
            let order = stage
                .named("order")
                .and_then(Value::as_str)
                .unwrap_or("desc");
            let direction = match order {
                "asc" => "ASC",
                "desc" => "DESC",
                _ => {
                    return Err(QueryError::invalid_request(
                        "sort order must be asc or desc",
                    ));
                }
            };
            builder
                .order_by
                .push(format!("{} {direction}", output_field(field)?));
        }
        "limit" => {
            let limit = stage
                .named("rows")
                .or_else(|| stage.positional(0))
                .and_then(Value::as_u64)
                .ok_or_else(|| QueryError::invalid_request("limit() requires rows"))?;
            builder.limit = builder.limit.min(checked_limit(limit)?);
        }
        "select" => {
            let fields = stage
                .arguments
                .iter()
                .flat_map(|argument| match &argument.value {
                    Value::List(values) => {
                        values.iter().filter_map(Value::as_str).collect::<Vec<_>>()
                    }
                    value => value.as_str().into_iter().collect::<Vec<_>>(),
                })
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if fields.is_empty() {
                return Err(QueryError::invalid_request(
                    "select() requires at least one field",
                ));
            }
            for field in &fields {
                output_field(field)?;
            }
            builder.shape.selected_fields = Some(fields);
        }
        "table" | "flame" | "tree" | "completeness" => {}
        "compare" | "diff" | "vdiff" | "values" | "get" | "instances" | "events" | "dumps"
        | "trace" => {
            return Err(unsupported(
                &stage.name,
                "stage requires exact/value or cross-query semantics",
            ));
        }
        other => {
            return Err(unsupported(
                other,
                "stage is outside the launch aggregate subset",
            ));
        }
    }
    Ok(())
}

fn compile_where(stage: &StageCall, builder: &mut SqlBuilder) -> Result<(), QueryError> {
    let expression = stage
        .arguments
        .iter()
        .find_map(|argument| match &argument.value {
            Value::Expr(expression) => Some(expression),
            _ => None,
        })
        .ok_or_else(|| QueryError::invalid_request("where() requires a comparison"))?;
    let field = storage_field(builder.source, &expression.field)?;
    let operator = match expression.op {
        CompareOp::Eq => "=",
        CompareOp::Ne => "!=",
        CompareOp::Gt => ">",
        CompareOp::Ge => ">=",
        CompareOp::Lt => "<",
        CompareOp::Le => "<=",
    };
    let parameter = match expression.value.as_ref() {
        Value::Integer(value) => builder.param(ClickHouseParamType::Int64, value.to_string()),
        Value::String(value) | Value::Identifier(value) | Value::Human(value) => {
            builder.param(ClickHouseParamType::String, value.clone())
        }
        Value::Bool(value) => {
            builder.param(ClickHouseParamType::UInt64, u8::from(*value).to_string())
        }
        _ => {
            return Err(QueryError::invalid_request(
                "ClickHouse where() supports scalar values",
            ));
        }
    };
    builder
        .filters
        .push(format!("{field} {operator} {parameter}"));
    Ok(())
}

impl SqlBuilder {
    fn param(&mut self, kind: ClickHouseParamType, value: String) -> String {
        let name = format!("p{}", self.params.len());
        let placeholder = format!("{{{name}:{}}}", kind.sql_name());
        self.params.push(ClickHouseParam { name, kind, value });
        placeholder
    }

    fn promote_runs_to_contexts(&mut self) {
        if self.source == Source::Runs {
            self.source = Source::Contexts;
            for filter in &mut self.filters {
                *filter = filter
                    .replace("r.revision_id", "c.revision_id")
                    .replace("r.boundary_id", "c.boundary_id")
                    .replace("r.created_ms * 1000000", "c.window_end_ns");
            }
        }
    }

    fn finish(self) -> Result<CompiledClickHouseQuery, QueryError> {
        let (default_fields, from, group_by, required_tables) = match self.source {
            Source::Runs => (
                vec![
                    "r.boundary_id AS run_id".to_owned(),
                    "r.created_ms AS created_ms".to_owned(),
                    "r.completion_status AS status".to_owned(),
                    "r.revision_id AS revision_id".to_owned(),
                ],
                "obs_runs AS r".to_owned(),
                Vec::new(),
                vec!["obs_runs".to_owned()],
            ),
            Source::Contexts => {
                let identity_fields = if self.shape.rollup {
                    vec![
                        "d.definition_key AS definition_key".to_owned(),
                        "any(d.fqn) AS fqn".to_owned(),
                    ]
                } else {
                    vec![
                        "c.boundary_id AS run_id".to_owned(),
                        "c.partition_id AS partition_id".to_owned(),
                        "c.node_id AS node_id".to_owned(),
                        "d.definition_key AS definition_key".to_owned(),
                        "any(d.fqn) AS fqn".to_owned(),
                    ]
                };
                let mut fields = identity_fields;
                if let Some(bucket) = self.shape.series_bucket_ns {
                    fields.push(format!(
                        "intDiv(c.window_start_ns, {bucket}) * {bucket} AS bucket_ns"
                    ));
                }
                fields.extend(
                    [
                        "sum(c.enters) AS calls",
                        "sum(c.ends_err + c.ends_cancel + c.ends_exit) AS errors",
                        "sum(c.total_ns) AS total_ns",
                        "sum(c.self_ns) AS self_ns",
                        "sum(c.await_ns) AS awaiting_ns",
                        "sum(c.llm_calls) AS llm_calls",
                        "sum(c.tokens_in) AS tokens_in",
                        "sum(c.tokens_out) AS tokens_out",
                    ]
                    .into_iter()
                    .map(str::to_owned),
                );
                let fields = if let Some(bucket) = self.shape.series_bucket_ns {
                    fields
                        .into_iter()
                        .map(|field| field.replace("{bucket}", &bucket.to_string()))
                        .collect()
                } else {
                    fields
                };
                let mut groups = if self.shape.rollup {
                    vec!["d.definition_key".to_owned()]
                } else {
                    vec![
                        "c.boundary_id".to_owned(),
                        "c.partition_id".to_owned(),
                        "c.node_id".to_owned(),
                        "d.definition_key".to_owned(),
                    ]
                };
                if let Some(bucket) = self.shape.series_bucket_ns {
                    groups.push(format!("intDiv(c.window_start_ns, {bucket}) * {bucket}"));
                }
                (
                    fields,
                    "obs_cct_aggregate AS c ANY LEFT JOIN obs_function_dictionary AS d ON \
                     d.revision_id = c.revision_id AND d.function_id = c.function_id"
                        .to_owned(),
                    groups,
                    vec![
                        "obs_cct_aggregate".to_owned(),
                        "obs_function_dictionary".to_owned(),
                    ],
                )
            }
        };
        let selected = if let Some(fields) = self.shape.selected_fields {
            fields
                .iter()
                .map(|field| output_field(field).map(str::to_owned))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            default_fields
        };
        let mut sql = format!("SELECT\n  {}\nFROM {from}", selected.join(",\n  "));
        if !self.filters.is_empty() {
            sql.push_str("\nWHERE ");
            sql.push_str(&self.filters.join("\n  AND "));
        }
        if !group_by.is_empty() {
            sql.push_str("\nGROUP BY ");
            sql.push_str(&group_by.join(", "));
        }
        if !self.having.is_empty() {
            sql.push_str("\nHAVING ");
            sql.push_str(&self.having.join(" AND "));
        }
        let order = if self.order_by.is_empty() && self.source == Source::Contexts {
            vec!["total_ns DESC".to_owned()]
        } else {
            self.order_by
        };
        if !order.is_empty() {
            sql.push_str("\nORDER BY ");
            sql.push_str(&order.join(", "));
        }
        sql.push_str(&format!("\nLIMIT {}", self.limit));
        Ok(CompiledClickHouseQuery {
            statement_name: None,
            sql,
            params: self.params,
            required_tables,
        })
    }
}

fn storage_field(source: Source, field: &str) -> Result<&'static str, QueryError> {
    let result = match (source, field) {
        (Source::Runs, "run_id") => "r.boundary_id",
        (Source::Runs, "created_ms") => "r.created_ms",
        (Source::Runs, "status") => "r.completion_status",
        (Source::Runs, "revision_id") => "r.revision_id",
        (Source::Contexts, "run_id") => "c.boundary_id",
        (Source::Contexts, "node_id") => "c.node_id",
        (Source::Contexts, "function_id") => "c.function_id",
        (Source::Contexts, "calls") => "c.enters",
        (Source::Contexts, "errors") => "(c.ends_err + c.ends_cancel + c.ends_exit)",
        (Source::Contexts, "total_ns") => "c.total_ns",
        (Source::Contexts, "self_ns") => "c.self_ns",
        (Source::Contexts, "awaiting_ns") => "c.await_ns",
        _ => {
            return Err(QueryError::invalid_request(format!(
                "field `{field}` is not available in the ClickHouse aggregate subset"
            )));
        }
    };
    Ok(result)
}

fn output_field(field: &str) -> Result<&'static str, QueryError> {
    match field {
        "run_id" => Ok("run_id"),
        "created_ms" => Ok("created_ms"),
        "status" => Ok("status"),
        "revision_id" => Ok("revision_id"),
        "partition_id" => Ok("partition_id"),
        "node_id" => Ok("node_id"),
        "definition_key" => Ok("definition_key"),
        "fqn" => Ok("fqn"),
        "bucket_ns" => Ok("bucket_ns"),
        "calls" => Ok("calls"),
        "errors" => Ok("errors"),
        "total_ns" => Ok("total_ns"),
        "self_ns" => Ok("self_ns"),
        "awaiting_ns" => Ok("awaiting_ns"),
        "llm_calls" => Ok("llm_calls"),
        "tokens_in" => Ok("tokens_in"),
        "tokens_out" => Ok("tokens_out"),
        _ => Err(QueryError::invalid_request(format!(
            "field `{field}` is not a ClickHouse result column"
        ))),
    }
}

fn checked_limit(limit: u64) -> Result<usize, QueryError> {
    if limit == 0 || limit > 100_000 {
        return Err(QueryError::invalid_request(
            "ClickHouse row limit must be in 1..=100000",
        ));
    }
    usize::try_from(limit).map_err(|_| QueryError::invalid_request("row limit does not fit usize"))
}

fn parse_duration_ns(value: &str) -> Result<u64, QueryError> {
    let split = value
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(value.len());
    let amount = value[..split]
        .parse::<u64>()
        .map_err(|_| QueryError::invalid_request(format!("invalid duration `{value}`")))?;
    let multiplier = match &value[split..] {
        "ns" => 1,
        "us" => 1_000,
        "ms" => 1_000_000,
        "s" => 1_000_000_000,
        "m" => 60 * 1_000_000_000,
        "h" => 60 * 60 * 1_000_000_000,
        "d" => 24 * 60 * 60 * 1_000_000_000,
        _ => {
            return Err(QueryError::invalid_request(format!(
                "unsupported duration unit in `{value}`"
            )));
        }
    };
    amount
        .checked_mul(multiplier)
        .ok_or_else(|| QueryError::invalid_request("duration overflows nanoseconds"))
}

fn glob_to_re2(value: &str) -> String {
    let mut output = String::from("^");
    for character in value.chars() {
        match character {
            '*' => output.push_str(".*"),
            '.' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\' => {
                output.push('\\');
                output.push(character);
            }
            _ => output.push(character),
        }
    }
    output.push('$');
    output
}

fn unsupported(stage: &str, reason: &str) -> QueryError {
    QueryError::invalid_request(format!(
        "E_CLICKHOUSE_SUBSET: `{stage}` cannot compile: {reason}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_query_is_parameterized_and_bounded() {
        let compiled = compile_clickhouse(
            "ctx(last=24h, rev=\"r2\") | calls(fn=\"extract_*\") | rollup() | top(5, by=errors)",
            &BTreeMap::new(),
        )
        .unwrap();
        let query = &compiled[0];
        assert!(query.sql.contains("{p0:UInt64}"));
        assert!(query.sql.contains("{p1:String}"));
        assert!(query.sql.contains("{p2:String}"));
        assert!(query.sql.ends_with("LIMIT 5"));
        assert_eq!(
            query.required_tables,
            ["obs_cct_aggregate", "obs_function_dictionary"]
        );
    }

    #[test]
    fn exact_stage_fails_closed() {
        let error = compile_clickhouse(
            "ctx(last=1h) | instances(source=trace) | events(around=call)",
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("E_CLICKHOUSE_SUBSET")
                || error.to_string().contains("E_NO_EXACT_SOURCE")
        );
    }
}
