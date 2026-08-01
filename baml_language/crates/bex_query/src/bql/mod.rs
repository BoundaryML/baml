//! Typed BAML Query Language surface shared by native, wasm, and cloud hosts.

mod catalog;
mod execute;
mod syntax;

pub const DEFAULT_LIMIT: usize = 1_000;
pub const HARD_MAX_ROWS: usize = 100_000;

pub use catalog::{
    Availability, BqlSchema, FieldSpec, PlannedStage, QueryPlan, ScriptPlan, SetKind, StageArgSpec,
    StageCategory, StageSpec, bql_schema, plan, schema_json, stage_catalog,
};
#[cfg(feature = "native")]
pub use execute::NativeBqlEngine;
pub use execute::{
    BqlCursor, BqlRow, ExecuteOptions, NamedQueryResult, QueryCaptureLoss, QueryEnvelope,
    QueryMeta, QueryWatermark, ScriptResult, SnapshotEntry, SnapshotToken, parse_and_plan,
};
pub use syntax::{
    Argument, CompareOp, Expression, Pipeline, Script, Span, StageCall, Statement, Value,
    bind_params, parse,
};
