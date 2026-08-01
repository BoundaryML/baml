use serde::{Deserialize, Serialize};

use super::syntax::{Pipeline, Script, StageCall, Value};
use crate::QueryError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetKind {
    RunSet,
    CtxSet,
    CallSet,
    ValueSet,
    EventSet,
    SpawnSet,
    SeriesSet,
    DiffSet,
    Table,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StageCategory {
    Source,
    Filter,
    Tree,
    Time,
    Compare,
    Values,
    Events,
    Sink,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Implemented,
    TypedUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StageArgSpec {
    pub name: &'static str,
    pub value_type: &'static str,
    pub required: bool,
    pub default: Option<&'static str>,
    pub units: Option<&'static str>,
    pub enum_values: &'static [&'static str],
    pub example: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StageSpec {
    pub name: &'static str,
    pub category: StageCategory,
    pub inputs: &'static [SetKind],
    pub output: SetKind,
    pub preserves_input: bool,
    pub arguments: &'static [StageArgSpec],
    pub availability: Availability,
    pub description: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FieldSpec {
    pub set: SetKind,
    pub name: &'static str,
    pub value_type: &'static str,
    pub units: Option<&'static str>,
    pub enum_values: &'static [&'static str],
    pub id_drilldown: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BqlSchema {
    pub version: u16,
    pub default_limit: usize,
    pub hard_max_rows: usize,
    pub hard_max_bytes: usize,
    pub set_kinds: &'static [SetKind],
    pub stages: Vec<StageSpec>,
    pub fields: Vec<FieldSpec>,
}

const NONE: &[SetKind] = &[];
const ANY_ROWS: &[SetKind] = &[
    SetKind::RunSet,
    SetKind::CtxSet,
    SetKind::CallSet,
    SetKind::ValueSet,
    SetKind::EventSet,
    SetKind::SpawnSet,
    SetKind::SeriesSet,
    SetKind::DiffSet,
    SetKind::Table,
];
const RUN: &[SetKind] = &[SetKind::RunSet];
const CTX: &[SetKind] = &[SetKind::CtxSet];
const CTX_DIFF: &[SetKind] = &[SetKind::CtxSet, SetKind::DiffSet];
const CALL_CTX: &[SetKind] = &[SetKind::CtxSet, SetKind::CallSet];
const VALUE: &[SetKind] = &[SetKind::ValueSet];
const EVENT: &[SetKind] = &[SetKind::EventSet];
const DIFF: &[SetKind] = &[SetKind::DiffSet];

const NO_ARGS: &[StageArgSpec] = &[];
const RUNS_ARGS: &[StageArgSpec] = &[
    arg("last", "duration", false, None, Some("ns"), &[], "24h"),
    arg(
        "status",
        "enum",
        false,
        None,
        None,
        &["ok", "errored", "running", "crashed"],
        "errored",
    ),
    arg("rev", "string", false, None, None, &[], "\"baml_rev_1_…\""),
    arg("limit", "integer", false, Some("1000"), None, &[], "100"),
];
const CTX_ARGS: &[StageArgSpec] = &[
    arg("last", "duration", false, None, Some("ns"), &[], "24h"),
    arg(
        "status",
        "enum",
        false,
        None,
        None,
        &["ok", "errored", "running", "crashed"],
        "errored",
    ),
    arg(
        "rev",
        "string_or_list",
        false,
        None,
        None,
        &[],
        "[\"v418\", \"v419\"]",
    ),
    arg("limit", "integer", false, Some("1000"), None, &[], "100"),
    arg(
        "range",
        "time_range",
        false,
        None,
        None,
        &[],
        "03:00..03:10",
    ),
    arg("align", "enum", false, None, None, &["fqn"], "fqn"),
];
const RUN_ARGS: &[StageArgSpec] = &[arg(
    "id",
    "boundary_id",
    true,
    None,
    None,
    &[],
    "\"baml_id_1_…\"",
)];
const CALLS_ARGS: &[StageArgSpec] = &[
    arg("fn", "glob", false, None, None, &[], "\"extract_*\""),
    arg(
        "path",
        "path_pattern",
        false,
        None,
        None,
        &[],
        "\"main>>extract_*\"",
    ),
    arg("kind", "enum", false, None, None, &["llm"], "llm"),
];
const LIMIT_ARGS: &[StageArgSpec] = &[arg("rows", "integer", true, None, None, &[], "100")];
const WHERE_ARGS: &[StageArgSpec] = &[arg(
    "predicate",
    "expression",
    true,
    None,
    None,
    &[],
    "errors > 0",
)];
const TOP_ARGS: &[StageArgSpec] = &[
    arg("k", "integer", true, None, None, &[], "10"),
    arg("by", "field", false, Some("total_ns"), None, &[], "errors"),
];
const SELECT_ARGS: &[StageArgSpec] = &[arg(
    "fields",
    "field_list",
    true,
    None,
    None,
    &[],
    "path, errors",
)];
const SORT_ARGS: &[StageArgSpec] = &[
    arg("by", "field", true, None, None, &[], "total_ns"),
    arg(
        "order",
        "enum",
        false,
        Some("desc"),
        None,
        &["asc", "desc"],
        "desc",
    ),
];
const SERIES_ARGS: &[StageArgSpec] = &[
    arg("bucket", "duration", true, None, Some("ns"), &[], "15m"),
    arg(
        "metrics",
        "field_list",
        true,
        None,
        None,
        &[],
        "[calls, errors, p95(total_ns)]",
    ),
];
const DIFF_ARGS: &[StageArgSpec] = &[
    arg(
        "left",
        "pipeline",
        true,
        None,
        None,
        &[],
        "runs(rev=\"old\")",
    ),
    arg(
        "right",
        "pipeline",
        true,
        None,
        None,
        &[],
        "runs(rev=\"new\")",
    ),
    arg("align", "enum", false, Some("fqn"), None, &["fqn"], "fqn"),
];
const COMPARE_ARGS: &[StageArgSpec] = &[
    arg(
        "metrics",
        "field_list",
        false,
        Some("[calls,errors,self_ns,awaiting_ns]"),
        None,
        &[],
        "[calls, errors]",
    ),
    arg("match_io", "bool", false, Some("false"), None, &[], "true"),
];
const VALUES_ARGS: &[StageArgSpec] = &[arg(
    "role",
    "role_list",
    false,
    None,
    None,
    &[],
    "[input, output]",
)];
const GET_ARGS: &[StageArgSpec] = &[
    arg(
        "max_bytes",
        "bytes",
        false,
        Some("64kb"),
        Some("bytes"),
        &[],
        "256kb",
    ),
    arg("depth", "integer", false, Some("1"), None, &[], "2"),
    arg("as", "type", false, None, None, &[], "MyType"),
];
const INSTANCES_ARGS: &[StageArgSpec] = &[arg(
    "source",
    "enum",
    true,
    None,
    None,
    &["recent", "flight", "trace"],
    "flight",
)];
const EVENTS_ARGS: &[StageArgSpec] = &[
    arg(
        "around",
        "enum",
        true,
        None,
        None,
        &["call", "trigger"],
        "trigger",
    ),
    arg("before", "integer", false, Some("200"), None, &[], "200"),
    arg("after", "integer", false, Some("20"), None, &[], "20"),
    arg(
        "threads",
        "enum",
        false,
        Some("all"),
        None,
        &["all", "call"],
        "all",
    ),
];
const DUMPS_ARGS: &[StageArgSpec] = &[arg(
    "trigger",
    "enum",
    false,
    None,
    None,
    &["error", "slow", "manual", "panic"],
    "error",
)];
const TRACE_ARGS: &[StageArgSpec] = &[arg(
    "session",
    "session_id",
    true,
    None,
    None,
    &[],
    "\"session-id\"",
)];
const HEALTH_ARGS: &[StageArgSpec] = &[
    arg(
        "range",
        "time_range",
        false,
        None,
        None,
        &[],
        "03:00..04:00",
    ),
    arg(
        "process",
        "enum",
        false,
        Some("all"),
        None,
        &["all", "live", "dead"],
        "all",
    ),
];
const ROLLUP_ARGS: &[StageArgSpec] = &[arg(
    "by",
    "enum",
    false,
    Some("fn"),
    None,
    &["fn", "path", "file", "package"],
    "path",
)];
const CALLERS_ARGS: &[StageArgSpec] = &[arg("fn", "glob", true, None, None, &[], "\"extract_*\"")];
const CALLEES_ARGS: &[StageArgSpec] = &[arg("depth", "integer", false, Some("1"), None, &[], "3")];
const DELTA_ARGS: &[StageArgSpec] = &[arg(
    "vs",
    "enum_or_range",
    true,
    None,
    None,
    &["prev", "range", "rev"],
    "prev",
)];
const VDIFF_ARGS: &[StageArgSpec] = &[
    arg("role", "role", false, None, None, &[], "output"),
    arg(
        "max_nodes",
        "integer",
        false,
        Some("1000"),
        None,
        &[],
        "100",
    ),
];
const STATS_ARGS: &[StageArgSpec] = &[
    arg(
        "aggs",
        "aggregate_list",
        false,
        None,
        None,
        &[],
        "n=count()",
    ),
    arg(
        "by",
        "field_list",
        false,
        None,
        None,
        &[],
        "[root_fn, model]",
    ),
];
const HIST_ARGS: &[StageArgSpec] = &[arg(
    "metric",
    "field",
    false,
    Some("total_ns"),
    None,
    &[],
    "total_ns",
)];
const LOOKUP_ARGS: &[StageArgSpec] = &[
    arg("file", "path", true, None, None, &[], "\"prices.csv\""),
    arg("on", "field", true, None, None, &[], "model"),
];
const EXPORT_ARGS: &[StageArgSpec] = &[arg(
    "format",
    "enum",
    false,
    Some("jsonl"),
    None,
    &["jsonl", "parquet"],
    "parquet",
)];
const LIVE_ARGS: &[StageArgSpec] = &[arg(
    "interval",
    "duration",
    false,
    Some("1s"),
    Some("ns"),
    &[],
    "2s",
)];

const fn arg(
    name: &'static str,
    value_type: &'static str,
    required: bool,
    default: Option<&'static str>,
    units: Option<&'static str>,
    enum_values: &'static [&'static str],
    example: &'static str,
) -> StageArgSpec {
    StageArgSpec {
        name,
        value_type,
        required,
        default,
        units,
        enum_values,
        example,
    }
}

fn stage(
    name: &'static str,
    category: StageCategory,
    inputs: &'static [SetKind],
    output: SetKind,
    preserves_input: bool,
    arguments: &'static [StageArgSpec],
    availability: Availability,
    description: &'static str,
) -> StageSpec {
    StageSpec {
        name,
        category,
        inputs,
        output,
        preserves_input,
        arguments,
        availability,
        description,
    }
}

#[must_use]
pub fn stage_catalog() -> Vec<StageSpec> {
    use Availability::{Implemented as I, TypedUnavailable as U};
    use SetKind::{
        CallSet, CtxSet, DiffSet, EventSet, RunSet, SeriesSet, SpawnSet, Table, ValueSet,
    };
    use StageCategory::{Compare, Events, Filter, Sink, Source, Time, Tree, Values};
    vec![
        stage(
            "runs",
            Source,
            NONE,
            RunSet,
            false,
            RUNS_ARGS,
            I,
            "bounded recent run index",
        ),
        stage(
            "run",
            Source,
            NONE,
            RunSet,
            false,
            RUN_ARGS,
            I,
            "one boundary by public id",
        ),
        stage(
            "ctx",
            Source,
            NONE,
            CtxSet,
            false,
            CTX_ARGS,
            I,
            "folded calling contexts",
        ),
        stage(
            "dumps",
            Source,
            RUN,
            EventSet,
            false,
            DUMPS_ARGS,
            I,
            "flight-recorder dumps",
        ),
        stage(
            "trace",
            Source,
            NONE,
            EventSet,
            false,
            TRACE_ARGS,
            U,
            "full-trace session",
        ),
        stage(
            "health",
            Source,
            NONE,
            Table,
            false,
            HEALTH_ARGS,
            I,
            "completeness and loss health",
        ),
        stage(
            "storage",
            Source,
            NONE,
            Table,
            false,
            NO_ARGS,
            U,
            "storage accounting",
        ),
        stage(
            "audit",
            Source,
            NONE,
            Table,
            false,
            RUNS_ARGS,
            I,
            "capture privacy audit",
        ),
        stage(
            "revs",
            Source,
            NONE,
            Table,
            false,
            NO_ARGS,
            U,
            "revision index",
        ),
        stage(
            "triggers",
            Source,
            NONE,
            Table,
            false,
            RUNS_ARGS,
            I,
            "boundary triggers",
        ),
        stage(
            "calls",
            Filter,
            CTX_DIFF,
            CtxSet,
            true,
            CALLS_ARGS,
            I,
            "context/call filter",
        ),
        stage(
            "errors",
            Filter,
            ANY_ROWS,
            Table,
            true,
            NO_ARGS,
            I,
            "rows with error counts",
        ),
        stage(
            "failure",
            Filter,
            RUN,
            Table,
            false,
            NO_ARGS,
            I,
            "bounded failure evidence",
        ),
        stage(
            "where",
            Filter,
            ANY_ROWS,
            Table,
            true,
            WHERE_ARGS,
            I,
            "typed row predicate",
        ),
        stage(
            "limit",
            Filter,
            ANY_ROWS,
            Table,
            true,
            LIMIT_ARGS,
            I,
            "hard row limit",
        ),
        stage(
            "sort",
            Filter,
            ANY_ROWS,
            Table,
            true,
            SORT_ARGS,
            I,
            "bounded row ordering",
        ),
        stage(
            "select",
            Filter,
            ANY_ROWS,
            Table,
            true,
            SELECT_ARGS,
            I,
            "field projection",
        ),
        stage(
            "rollup",
            Tree,
            CTX,
            CtxSet,
            false,
            ROLLUP_ARGS,
            I,
            "aggregate by function",
        ),
        stage(
            "callers",
            Tree,
            CTX,
            CtxSet,
            true,
            CALLERS_ARGS,
            U,
            "caller contexts",
        ),
        stage(
            "callees",
            Tree,
            CTX,
            CtxSet,
            true,
            CALLEES_ARGS,
            U,
            "callee contexts",
        ),
        stage(
            "spawns",
            Tree,
            CTX,
            SpawnSet,
            false,
            NO_ARGS,
            I,
            "spawn-edge aggregates",
        ),
        stage(
            "tree",
            Tree,
            ANY_ROWS,
            Table,
            false,
            NO_ARGS,
            I,
            "tree table sink",
        ),
        stage(
            "series",
            Time,
            CTX,
            SeriesSet,
            false,
            SERIES_ARGS,
            I,
            "bucketed CCT series",
        ),
        stage(
            "delta",
            Time,
            &[CtxSet, SeriesSet],
            DiffSet,
            false,
            DELTA_ARGS,
            U,
            "time/revision delta",
        ),
        stage(
            "diff",
            Compare,
            NONE,
            DiffSet,
            false,
            DIFF_ARGS,
            I,
            "align two bounded sources",
        ),
        stage(
            "compare",
            Compare,
            DIFF,
            DiffSet,
            true,
            COMPARE_ARGS,
            I,
            "metric comparison",
        ),
        stage(
            "vdiff",
            Compare,
            &[ValueSet, DiffSet],
            DiffSet,
            true,
            VDIFF_ARGS,
            I,
            "Merkle value diff",
        ),
        stage(
            "values",
            Values,
            CALL_CTX,
            ValueSet,
            false,
            VALUES_ARGS,
            I,
            "captured value roots",
        ),
        stage(
            "get",
            Values,
            VALUE,
            Table,
            false,
            GET_ARGS,
            I,
            "byte-budgeted hydration",
        ),
        stage(
            "instances",
            Values,
            CTX,
            CallSet,
            false,
            INSTANCES_ARGS,
            U,
            "exact call instances",
        ),
        stage(
            "events",
            Events,
            &[CallSet, EventSet],
            EventSet,
            false,
            EVENTS_ARGS,
            I,
            "exact events",
        ),
        stage(
            "top",
            Sink,
            ANY_ROWS,
            Table,
            false,
            TOP_ARGS,
            I,
            "top-k rows",
        ),
        stage(
            "stats",
            Sink,
            ANY_ROWS,
            Table,
            false,
            STATS_ARGS,
            I,
            "grouped aggregates",
        ),
        stage(
            "hist",
            Sink,
            CTX,
            Table,
            false,
            HIST_ARGS,
            I,
            "duration histogram",
        ),
        stage(
            "lookup",
            Sink,
            ANY_ROWS,
            Table,
            true,
            LOOKUP_ARGS,
            U,
            "user lookup join",
        ),
        stage(
            "table",
            Sink,
            ANY_ROWS,
            Table,
            false,
            NO_ARGS,
            I,
            "terminal table",
        ),
        stage(
            "flame",
            Sink,
            CTX,
            Table,
            false,
            NO_ARGS,
            I,
            "flame/tree rows",
        ),
        stage(
            "export",
            Sink,
            ANY_ROWS,
            Table,
            false,
            EXPORT_ARGS,
            U,
            "external export",
        ),
        stage(
            "live",
            Sink,
            ANY_ROWS,
            Table,
            true,
            LIVE_ARGS,
            U,
            "live subscription",
        ),
        stage(
            "explain",
            Sink,
            ANY_ROWS,
            Table,
            false,
            NO_ARGS,
            I,
            "plan explanation",
        ),
        stage(
            "completeness",
            Sink,
            ANY_ROWS,
            Table,
            false,
            NO_ARGS,
            I,
            "trust footer as rows",
        ),
        stage(
            "critical_path",
            Events,
            EVENT,
            Table,
            false,
            NO_ARGS,
            U,
            "exact critical path",
        ),
    ]
}

#[must_use]
pub fn bql_schema() -> BqlSchema {
    const KINDS: &[SetKind] = &[
        SetKind::RunSet,
        SetKind::CtxSet,
        SetKind::CallSet,
        SetKind::ValueSet,
        SetKind::EventSet,
        SetKind::SpawnSet,
        SetKind::SeriesSet,
        SetKind::DiffSet,
        SetKind::Table,
    ];
    BqlSchema {
        version: 1,
        default_limit: super::DEFAULT_LIMIT,
        hard_max_rows: super::HARD_MAX_ROWS,
        hard_max_bytes: crate::HARD_MAX_BYTES,
        set_kinds: KINDS,
        stages: stage_catalog(),
        fields: field_catalog(),
    }
}

#[must_use]
pub fn schema_json() -> String {
    serde_json::to_string_pretty(&bql_schema()).expect("static BQL schema serializes")
}

fn field_catalog() -> Vec<FieldSpec> {
    use SetKind::{CtxSet, DiffSet, RunSet, Table};
    vec![
        field(
            RunSet,
            "run_id",
            "boundary_id",
            None,
            &[],
            Some("run(\"{value}\") | failure()"),
        ),
        field(RunSet, "created_ms", "timestamp", Some("ms"), &[], None),
        field(
            RunSet,
            "status",
            "enum",
            None,
            &["complete", "errored", "running", "crashed"],
            None,
        ),
        field(
            CtxSet,
            "node_id",
            "session_node_id",
            None,
            &[],
            Some("run($run) | calls() | where(node_id == {value})"),
        ),
        field(
            CtxSet,
            "function_id",
            "revision_function_id",
            None,
            &[],
            None,
        ),
        field(CtxSet, "calls", "integer", Some("calls"), &[], None),
        field(CtxSet, "errors", "integer", Some("calls"), &[], None),
        field(CtxSet, "total_ns", "duration", Some("ns"), &[], None),
        field(CtxSet, "self_ns", "duration", Some("ns"), &[], None),
        field(CtxSet, "awaiting_ns", "duration", Some("ns"), &[], None),
        field(DiffSet, "delta_errors", "integer", Some("calls"), &[], None),
        field(DiffSet, "input_cid", "cid", None, &[], None),
        field(DiffSet, "matched_input", "bool", None, &[], None),
        field(DiffSet, "output_equal", "bool?", None, &[], None),
        field(
            DiffSet,
            "verdict",
            "enum",
            None,
            &["unchanged", "changed", "left_only", "right_only"],
            None,
        ),
        field(Table, "complete", "bool", None, &[], None),
        field(
            Table,
            "snapshot",
            "snapshot",
            None,
            &[],
            Some("baml q $query --snapshot {value}"),
        ),
    ]
}

const fn field(
    set: SetKind,
    name: &'static str,
    value_type: &'static str,
    units: Option<&'static str>,
    enum_values: &'static [&'static str],
    id_drilldown: Option<&'static str>,
) -> FieldSpec {
    FieldSpec {
        set,
        name,
        value_type,
        units,
        enum_values,
        id_drilldown,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlannedStage {
    pub name: String,
    pub input: Option<SetKind>,
    pub output: SetKind,
    pub implicit_run_to_ctx: bool,
    pub availability: Availability,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct QueryPlan {
    pub stages: Vec<PlannedStage>,
    pub output: SetKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ScriptPlan {
    pub statements: Vec<(Option<String>, QueryPlan)>,
}

pub fn plan(script: &Script) -> Result<ScriptPlan, QueryError> {
    let mut statements = Vec::new();
    for statement in &script.statements {
        statements.push((
            statement.name.clone(),
            plan_pipeline(&statement.pipeline, &script.source)?,
        ));
    }
    Ok(ScriptPlan { statements })
}

fn plan_pipeline(pipeline: &Pipeline, source: &str) -> Result<QueryPlan, QueryError> {
    let catalog = stage_catalog();
    let mut current = None;
    let mut stages = Vec::new();
    for (index, call) in pipeline.stages.iter().enumerate() {
        let Some(spec) = catalog.iter().find(|spec| spec.name == call.name) else {
            let names = catalog.iter().map(|spec| spec.name).collect::<Vec<_>>();
            let suggestion = closest(&call.name, &names).map(str::to_owned);
            let mut error = QueryError::bql(
                "E_UNKNOWN_STAGE",
                source,
                call.span.start,
                call.span.end,
                format!("unknown BQL stage `{}`", call.name),
            );
            if let QueryError::Bql(diagnostic) = &mut error {
                diagnostic.valid = names.into_iter().map(str::to_owned).collect();
                diagnostic.correction = suggestion
                    .as_deref()
                    .map(|name| source.replacen(&call.name, name, 1));
            }
            return Err(error);
        };
        validate_arguments(call, spec, source)?;
        if index == 0 && !spec.inputs.is_empty() {
            return Err(stage_input_error(call, source, None, spec.inputs));
        }
        if index != 0 && spec.inputs.is_empty() {
            return Err(QueryError::bql(
                "E_STAGE_INPUT",
                source,
                call.span.start,
                call.span.end,
                format!("source stage `{}` must begin a pipeline", call.name),
            ));
        }
        let mut implicit_run_to_ctx = false;
        if let Some(input) = current
            && !spec.inputs.contains(&input)
        {
            if input == SetKind::RunSet && spec.inputs.contains(&SetKind::CtxSet) {
                implicit_run_to_ctx = true;
            } else {
                return Err(stage_input_error(call, source, Some(input), spec.inputs));
            }
        }
        let output = if spec.preserves_input {
            current.unwrap_or(spec.output)
        } else {
            spec.output
        };
        stages.push(PlannedStage {
            name: call.name.clone(),
            input: current,
            output,
            implicit_run_to_ctx,
            availability: spec.availability,
        });
        current = Some(output);
        validate_nested(call, source)?;
    }
    let output = current.expect("parser rejects empty pipeline");
    if output != SetKind::Table {
        stages.push(PlannedStage {
            name: "<implicit table>".to_owned(),
            input: Some(output),
            output: SetKind::Table,
            implicit_run_to_ctx: false,
            availability: Availability::Implemented,
        });
    }
    Ok(QueryPlan {
        stages,
        output: SetKind::Table,
    })
}

fn validate_nested(call: &StageCall, source: &str) -> Result<(), QueryError> {
    // Values such as `p95(total_ns)` are metric expressions, represented by the
    // same syntax node as a nested stage. Only diff owns executable subqueries.
    if call.name != "diff" {
        return Ok(());
    }
    for argument in &call.arguments {
        if let Value::Stage(stage) = &argument.value {
            plan_pipeline(
                &Pipeline {
                    stages: vec![(**stage).clone()],
                    span: stage.span,
                },
                source,
            )?;
        }
    }
    Ok(())
}

fn validate_arguments(call: &StageCall, spec: &StageSpec, source: &str) -> Result<(), QueryError> {
    let positional = call
        .arguments
        .iter()
        .filter(|argument| argument.name.is_none())
        .count();
    let variadic = matches!(call.name.as_str(), "select" | "stats");
    if !variadic && positional > spec.arguments.len() {
        return Err(QueryError::bql(
            "E_STAGE_ARG",
            source,
            call.span.start,
            call.span.end,
            format!("too many positional arguments for `{}`", call.name),
        ));
    }
    for argument in &call.arguments {
        let Some(name) = argument.name.as_deref() else {
            continue;
        };
        if call.name != "stats"
            && !spec
                .arguments
                .iter()
                .any(|candidate| candidate.name == name)
        {
            let valid = spec
                .arguments
                .iter()
                .map(|candidate| candidate.name.to_owned())
                .collect::<Vec<_>>();
            let mut error = QueryError::bql(
                "E_UNKNOWN_FIELD",
                source,
                argument.span.start,
                argument.span.end,
                format!("stage `{}` has no argument `{name}`", call.name),
            );
            if let QueryError::Bql(diagnostic) = &mut error {
                diagnostic.valid = valid;
            }
            return Err(error);
        }
    }
    for (index, argument) in spec.arguments.iter().enumerate() {
        if argument.required
            && positional <= index
            && !call
                .arguments
                .iter()
                .any(|actual| actual.name.as_deref() == Some(argument.name))
        {
            return Err(QueryError::bql(
                "E_STAGE_ARG",
                source,
                call.span.start,
                call.span.end,
                format!(
                    "stage `{}` requires argument `{}`",
                    call.name, argument.name
                ),
            ));
        }
    }
    Ok(())
}

fn stage_input_error(
    call: &StageCall,
    source: &str,
    actual: Option<SetKind>,
    expected: &[SetKind],
) -> QueryError {
    QueryError::bql(
        "E_STAGE_INPUT",
        source,
        call.span.start,
        call.span.end,
        format!(
            "stage `{}` expects {}, got {}",
            call.name,
            expected
                .iter()
                .map(|kind| format!("{kind:?}"))
                .collect::<Vec<_>>()
                .join(" or "),
            actual.map_or_else(|| "pipeline start".to_owned(), |kind| format!("{kind:?}"))
        ),
    )
}

fn closest<'a>(needle: &str, candidates: &'a [&str]) -> Option<&'a str> {
    candidates
        .iter()
        .copied()
        .map(|candidate| (edit_distance(needle, candidate), candidate))
        .min_by_key(|(distance, _)| *distance)
        .filter(|(distance, _)| *distance <= 3)
        .map(|(_, candidate)| candidate)
}

fn edit_distance(left: &str, right: &str) -> usize {
    let mut row = (0..=right.len()).collect::<Vec<_>>();
    for (left_index, left_byte) in left.bytes().enumerate() {
        let mut diagonal = row[0];
        row[0] = left_index + 1;
        for (right_index, right_byte) in right.bytes().enumerate() {
            let above = row[right_index + 1];
            row[right_index + 1] = if left_byte == right_byte {
                diagonal
            } else {
                diagonal.min(above).min(row[right_index]) + 1
            };
            diagonal = above;
        }
    }
    row[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bql::syntax::parse;

    #[test]
    fn planner_applies_only_the_run_to_ctx_coercion() {
        let script = parse("runs(latest) | calls() | top(5, by=errors)").unwrap();
        let plan = plan(&script).unwrap();
        assert!(plan.statements[0].1.stages[1].implicit_run_to_ctx);
        assert_eq!(plan.statements[0].1.output, SetKind::Table);
    }

    #[test]
    fn planner_rejects_wrong_stage_input_with_machine_code() {
        let script = parse("run(\"baml_id_1_bad\") | events(around=call)").unwrap();
        let error = plan(&script).unwrap_err();
        assert_eq!(error.code(), "E_STAGE_INPUT");
    }

    #[test]
    fn schema_has_every_nine_set_kind_and_drilldowns() {
        let schema = bql_schema();
        assert_eq!(schema.set_kinds.len(), 9);
        assert!(
            schema
                .fields
                .iter()
                .any(|field| field.id_drilldown.is_some())
        );
        assert!(schema.stages.len() >= 40);
    }

    #[test]
    fn representative_surface_queries_type_check_before_capability_gates() {
        for source in [
            "run(\"baml_id_1_example\") | dumps(trigger=error) | events(around=trigger, before=200, after=20, threads=all)",
            "ctx(range=03:00..03:10, align=fqn) | series(bucket=15m, metrics=[calls, errors, p95(total_ns)])",
            "diff(runs(rev=\"old\"), runs(rev=\"new\"), align=fqn) | calls(fn=\"fn#7\") | compare(metrics=[calls, errors], match_io=true)",
            "runs(latest) | calls() | values(role=input) | stats(n=count(), by=cid) | where(n > 1)",
        ] {
            let script = parse(source).unwrap_or_else(|error| panic!("{source}: {error}"));
            plan(&script).unwrap_or_else(|error| panic!("{source}: {error}"));
        }
    }
}
