//! The versioned public logical catalog (TASK/baml-query-scope.md §4).
//!
//! Relations here are logical contracts: name, grain, columns with exact
//! Arrow types and nullability, key/identity scope, and resident-versus-
//! virtual status. Physical sources stay provider-private trusted
//! mappings. `catalog::v1()` is frozen by the goldens below; additive
//! changes (new nullable column, new view) stay v1, everything else is a
//! `v2` relation.

use std::{collections::BTreeMap, sync::Arc};

use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};

/// The catalog version this crate freezes.
pub const CATALOG_V1: &str = "v1";

/// Field-metadata key marking a virtual BAML value column. The physical
/// column carries an opaque provider handle (Binary); the value itself is
/// hydrated on demand and never resident.
pub const VALUE_META_KEY: &str = "baml.virtual";
pub const VALUE_META_VALUE: &str = "value";
/// Field-metadata key naming the captured role (`input`/`output`/`error`).
pub const VALUE_ROLE_KEY: &str = "baml.role";

/// Who sees a relation, view, or column (§4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Visibility {
    Public,
    /// `CatalogProfile::internal()` only (CLI under `BAML_INTERNAL`, the
    /// playground): store-debugging relations and raw counters.
    Internal,
    Hidden,
}

/// What one row of a relation represents (grain honesty is part of the
/// public contract: retained counts are never population counts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grain {
    /// One logical thread (root rows are executions).
    Thread,
    /// One distinct call path within one execution (population-true).
    Context,
    /// One individually retained call span (never "all calls").
    RetainedCall,
    /// One captured error.
    Error,
    /// One function within one program.
    ProgramFunction,
    /// One health metric within one execution (long format).
    HealthMetric,
    /// One writing process (internal).
    Stream,
    /// One published store file (internal).
    Segment,
    /// One CAS object (internal).
    CasObject,
}

/// One column's public contract.
#[derive(Debug, Clone)]
pub struct ColumnDef {
    pub name: &'static str,
    pub data_type: DataType,
    pub nullable: bool,
    /// Part of the relation's logical uniqueness key.
    pub key: bool,
    /// A virtual BAML value column (role name). The physical Arrow type
    /// is Binary (opaque handle); selecting or filtering it triggers
    /// budgeted hydration through the `ValueResolver`.
    pub value_role: Option<&'static str>,
    pub doc: &'static str,
    pub visibility: Visibility,
}

/// One relation's public contract.
#[derive(Debug, Clone)]
pub struct RelationDef {
    /// Versioned canonical name (`threads_v1`).
    pub name: &'static str,
    /// Convenience alias bound to this catalog version (`threads`). Saved
    /// / portable queries should use the versioned name; the alias is
    /// pinned to the session's bound catalog version, never "latest".
    pub alias: &'static str,
    /// A second alias kept for canon readers (`cct_population`), when one
    /// exists.
    pub secondary_alias: Option<&'static str>,
    pub grain: Grain,
    /// Provisional relations may change shape with a backend decision
    /// (documented per relation) without a catalog major bump.
    pub provisional: bool,
    pub columns: Vec<ColumnDef>,
    pub doc: &'static str,
    pub visibility: Visibility,
}

impl RelationDef {
    /// The Arrow schema (virtual value columns appear as Binary handles
    /// with `baml.virtual`/`baml.role` field metadata).
    #[must_use]
    pub fn schema(&self) -> SchemaRef {
        let fields: Vec<Field> = self
            .columns
            .iter()
            .map(|c| {
                let field = Field::new(c.name, c.data_type.clone(), c.nullable);
                match c.value_role {
                    Some(role) => field.with_metadata(
                        BTreeMap::from([
                            (VALUE_META_KEY.to_string(), VALUE_META_VALUE.to_string()),
                            (VALUE_ROLE_KEY.to_string(), role.to_string()),
                        ])
                        .into_iter()
                        .collect(),
                    ),
                    None => field,
                }
            })
            .collect();
        Arc::new(Schema::new(fields))
    }

    #[must_use]
    pub fn key_columns(&self) -> Vec<&'static str> {
        self.columns
            .iter()
            .filter(|c| c.key)
            .map(|c| c.name)
            .collect()
    }

    #[must_use]
    pub fn column(&self, name: &str) -> Option<&ColumnDef> {
        self.columns.iter().find(|c| c.name == name)
    }

    /// Every name this relation answers to.
    pub fn names(&self) -> impl Iterator<Item = &'static str> {
        [Some(self.name), Some(self.alias), self.secondary_alias]
            .into_iter()
            .flatten()
    }
}

/// One convenience view: SQL over other relations/views, registered as a
/// `DataFusion` `ViewTable` at session build. Views are how convenience
/// tables ship without provider code.
#[derive(Debug, Clone)]
pub struct ViewDef {
    pub name: &'static str,
    pub alias: &'static str,
    pub sql: &'static str,
    pub doc: &'static str,
    pub visibility: Visibility,
}

/// The complete versioned catalog.
#[derive(Debug, Clone)]
pub struct Catalog {
    pub version: &'static str,
    pub relations: Vec<RelationDef>,
    pub views: Vec<ViewDef>,
}

impl Catalog {
    /// Resolve a canonical name or one of its version-pinned aliases.
    #[must_use]
    pub fn relation(&self, name: &str) -> Option<&RelationDef> {
        self.relations.iter().find(|r| r.names().any(|n| n == name))
    }

    #[must_use]
    pub fn view(&self, name: &str) -> Option<&ViewDef> {
        self.views
            .iter()
            .find(|v| v.name == name || v.alias == name)
    }
}

/// Host-facing overrides applied over the base catalog (§4.1).
#[derive(Debug, Clone)]
pub enum Override {
    HideRelation(&'static str),
    HideColumn(&'static str, &'static str),
    ExposeInternal(&'static str),
    AddView(ViewDef),
}

/// A rendered slice of the catalog: what one session (and `--schema`)
/// exposes. What an agent discovers is exactly what it may query.
#[derive(Debug, Clone)]
pub struct CatalogProfile {
    pub base: Catalog,
    /// Maximum visibility level shown (`Public` or `Internal`).
    pub show: Visibility,
    pub overrides: Vec<Override>,
}

impl CatalogProfile {
    /// The default public profile over the frozen v1 catalog.
    #[must_use]
    pub fn public() -> CatalogProfile {
        CatalogProfile {
            base: v1(),
            show: Visibility::Public,
            overrides: Vec::new(),
        }
    }

    /// The internal profile (CLI under `BAML_INTERNAL`, the playground):
    /// also shows `Internal` relations such as `store_files_v1`.
    #[must_use]
    pub fn internal() -> CatalogProfile {
        CatalogProfile {
            base: v1(),
            show: Visibility::Internal,
            overrides: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_overrides(mut self, overrides: Vec<Override>) -> CatalogProfile {
        self.overrides = overrides;
        self
    }

    fn relation_visible(&self, relation: &RelationDef) -> bool {
        let exposed = self.overrides.iter().any(
            |o| matches!(o, Override::ExposeInternal(name) if relation.names().any(|n| n == *name)),
        );
        let hidden = self.overrides.iter().any(
            |o| matches!(o, Override::HideRelation(name) if relation.names().any(|n| n == *name)),
        );
        if hidden {
            return false;
        }
        relation.visibility <= self.show || (exposed && relation.visibility == Visibility::Internal)
    }

    fn column_visible(&self, relation: &RelationDef, column: &ColumnDef) -> bool {
        if self.overrides.iter().any(|o| {
            matches!(o, Override::HideColumn(rel, col)
                if relation.names().any(|n| n == *rel) && column.name == *col)
        }) {
            return false;
        }
        column.visibility <= self.show
    }

    /// The relations this profile exposes, with hidden columns removed.
    #[must_use]
    pub fn relations(&self) -> Vec<RelationDef> {
        self.base
            .relations
            .iter()
            .filter(|r| self.relation_visible(r))
            .map(|r| RelationDef {
                columns: r
                    .columns
                    .iter()
                    .filter(|c| self.column_visible(r, c))
                    .cloned()
                    .collect(),
                ..r.clone()
            })
            .collect()
    }

    /// The views this profile exposes (base + `AddView` overrides).
    #[must_use]
    pub fn views(&self) -> Vec<ViewDef> {
        let mut views: Vec<ViewDef> = self
            .base
            .views
            .iter()
            .filter(|v| v.visibility <= self.show)
            .cloned()
            .collect();
        for o in &self.overrides {
            if let Override::AddView(view) = o {
                views.push(view.clone());
            }
        }
        views
    }

    /// Resolve a visible relation by any of its names.
    #[must_use]
    pub fn relation(&self, name: &str) -> Option<RelationDef> {
        self.relations()
            .into_iter()
            .find(|r| r.names().any(|n| n == name))
    }
}

// ── column constructors ────────────────────────────────────────────────

fn ts() -> DataType {
    DataType::Timestamp(TimeUnit::Nanosecond, Some("UTC".into()))
}

fn col(name: &'static str, data_type: DataType, doc: &'static str) -> ColumnDef {
    ColumnDef {
        name,
        data_type,
        nullable: false,
        key: false,
        value_role: None,
        doc,
        visibility: Visibility::Public,
    }
}

fn nullable(name: &'static str, data_type: DataType, doc: &'static str) -> ColumnDef {
    ColumnDef {
        nullable: true,
        ..col(name, data_type, doc)
    }
}

fn key(name: &'static str, data_type: DataType, doc: &'static str) -> ColumnDef {
    ColumnDef {
        key: true,
        ..col(name, data_type, doc)
    }
}

/// Virtual BAML value column: Binary opaque handle, always nullable at
/// the Arrow level (NULL handle = role not applicable; every other
/// unavailability is typed inside the handle so the resolver can account
/// for it).
fn value(name: &'static str, role: &'static str, doc: &'static str) -> ColumnDef {
    ColumnDef {
        name,
        data_type: DataType::Binary,
        nullable: true,
        key: false,
        value_role: Some(role),
        doc,
        visibility: Visibility::Public,
    }
}

fn utf8_list(name: &'static str, doc: &'static str) -> ColumnDef {
    col(
        name,
        DataType::List(Arc::new(Field::new("item", DataType::Utf8, false))),
        doc,
    )
}

// ── catalog v1 ─────────────────────────────────────────────────────────

/// Build the frozen catalog v1.
#[must_use]
pub fn v1() -> Catalog {
    Catalog {
        version: CATALOG_V1,
        relations: vec![
            threads_v1(),
            call_path_stats_v1(),
            calls_v1(),
            errors_v1(),
            function_definitions_v1(),
            health_v1(),
            processes_v1(),
            store_files_v1(),
            value_index_v1(),
        ],
        // `llm_calls` (calls WHERE kind = 'llm') is deliberately absent:
        // no producer emits an LLM function kind yet (`FunctionKindCode`
        // is bytecode|sysop|native|native_unresolved), so the view could
        // only ever return an empty — and misleading — result. Re-adding
        // it when the kind exists is additive and stays v1.
        views: vec![ViewDef {
            name: "hot_call_paths_v1",
            alias: "hot_call_paths",
            sql: "SELECT execution_id, fqn, call_path_id, self_ns, inclusive_ns, \
                      calls_started FROM call_path_stats WHERE overflow_reason IS NULL \
                      AND timing_complete ORDER BY self_ns DESC",
            doc: "Call paths ranked by self time (overflow rows and incomplete \
                      timing excluded).",
            visibility: Visibility::Public,
        }],
    }
}

fn threads_v1() -> RelationDef {
    RelationDef {
        name: "threads_v1",
        alias: "threads",
        secondary_alias: None,
        grain: Grain::Thread,
        provisional: false,
        visibility: Visibility::Public,
        doc: "One row per logical thread (ThreadStart/ThreadEnd facts). A \
              thread with parent_thread_id IS NULL is an execution; the \
              execution-level columns are non-NULL only on those rows. \
              Thread listing reads no data files.",
        columns: vec![
            key(
                "execution_id",
                DataType::Utf8,
                "The root thread's id (baml_thread_1_… wire form); scopes every execution-scoped relation.",
            ),
            key(
                "thread_id",
                DataType::Utf8,
                "This thread's id (baml_thread_1_… wire form).",
            ),
            nullable(
                "parent_thread_id",
                DataType::Utf8,
                "The spawning thread; NULL exactly on execution roots.",
            ),
            nullable(
                "spawn_call_id",
                DataType::Utf8,
                "The parent call that spawned this thread, when retained.",
            ),
            nullable(
                "spawn_function_id",
                DataType::UInt32,
                "The spawned function's id (via the function table).",
            ),
            nullable(
                "spawn_fqn",
                DataType::Utf8,
                "The spawned function's fully qualified name.",
            ),
            nullable(
                "spawn_site_file",
                DataType::Utf8,
                "Source file of the spawn site.",
            ),
            nullable(
                "spawn_site_line",
                DataType::UInt32,
                "1-based line of the spawn site.",
            ),
            nullable(
                "name",
                DataType::Utf8,
                "User-assigned thread name; NULL when unnamed.",
            ),
            col("kind", DataType::Utf8, "root|spawn."),
            col(
                "started_ns",
                DataType::UInt64,
                "Process-relative start (ThreadStart).",
            ),
            nullable(
                "ended_ns",
                DataType::UInt64,
                "Process-relative end (ThreadEnd); NULL without an end fact.",
            ),
            nullable(
                "started_at",
                ts(),
                "Wall-clock start via the process's clock anchor; NULL if it is missing.",
            ),
            nullable(
                "ended_at",
                ts(),
                "Wall-clock end via the process's clock anchor.",
            ),
            nullable(
                "end_status",
                DataType::Utf8,
                "completed|cancelled|errored (ThreadEnd); NULL without an end fact.",
            ),
            nullable(
                "process_id",
                DataType::Utf8,
                "Root rows only: the writing process (durable id, euid hex — not the OS pid).",
            ),
            nullable(
                "engine_id",
                DataType::UInt64,
                "Root rows only: the engine within the process.",
            ),
            nullable(
                "program_id",
                DataType::Utf8,
                "Root rows only: conservative program content identity (EngineStarted).",
            ),
            nullable(
                "revision_id",
                DataType::Utf8,
                "Root rows only: compiled revision identity, when recorded.",
            ),
            nullable(
                "source_label",
                DataType::Utf8,
                "Root rows only: human source label from EngineStarted.",
            ),
            nullable(
                "runtime_id",
                DataType::Utf8,
                "Root rows only: the host runtime token (baml_id_1_…) the root returned.",
            ),
            nullable(
                "entry_function_id",
                DataType::UInt32,
                "Root rows only: the root span's function id; NULL if the root span is not retained.",
            ),
            nullable(
                "entry_fqn",
                DataType::Utf8,
                "Root rows only: the root span's fully qualified name.",
            ),
            nullable(
                "status",
                DataType::Utf8,
                "Root rows only: running|abandoned|succeeded|failed|cancelled|panicked (streams spec §6.2).",
            ),
            nullable(
                "index_state",
                DataType::Utf8,
                "Root rows only: complete|no_root_ended|root_started_lost|index_corrupt (meta plane only).",
            ),
            nullable(
                "duration_ns",
                DataType::UInt64,
                "Root rows only: root span inclusive time, else ended_ns - started_ns.",
            ),
            nullable(
                "total_calls",
                DataType::UInt64,
                "Root rows only: population call total incl. overflow (CCT counters).",
            ),
            nullable(
                "total_errors",
                DataType::UInt64,
                "Root rows only: population errored-call total.",
            ),
            nullable(
                "total_cancelled",
                DataType::UInt64,
                "Root rows only: population cancelled-call total.",
            ),
            nullable(
                "calls_retained",
                DataType::UInt64,
                "Root rows only: spans with a retained SpanStart.",
            ),
            nullable(
                "threads_total",
                DataType::UInt64,
                "Root rows only: ThreadStart count.",
            ),
            nullable(
                "value_state",
                DataType::Utf8,
                "Root rows only: complete|partial|none (any ValueState::Lost ⇒ partial).",
            ),
            nullable(
                "data_first_seq",
                DataType::UInt64,
                "Root rows only: first data file (RootEnded).",
            ),
            nullable(
                "data_last_seq",
                DataType::UInt64,
                "Root rows only: last data file (RootEnded).",
            ),
            nullable(
                "data_file_count",
                DataType::UInt64,
                "Root rows only: data files holding this execution's groups.",
            ),
        ],
    }
}

fn call_path_stats_v1() -> RelationDef {
    RelationDef {
        name: "call_path_stats_v1",
        alias: "call_path_stats",
        secondary_alias: Some("cct_population"),
        grain: Grain::Context,
        provisional: false,
        visibility: Visibility::Public,
        doc: "One row per distinct call path within one execution \
              (the CCT population) — population-true aggregates: every \
              runtime call contributes; never one row per invocation.",
        columns: vec![
            key("execution_id", DataType::Utf8, "The root thread's id."),
            key(
                "call_path_id",
                DataType::Utf8,
                "Call-path key hex (ContextKey); overflow rows use 'overflow:<reason>:<edge>'.",
            ),
            nullable(
                "parent_call_path_id",
                DataType::Utf8,
                "Tree parent; NULL for the root call path and overflow rows.",
            ),
            col(
                "depth",
                DataType::UInt32,
                "Depth from the execution root (0 = root), derived by walking parents.",
            ),
            col(
                "function_id",
                DataType::UInt32,
                "Function identity within the execution's program.",
            ),
            nullable(
                "fqn",
                DataType::Utf8,
                "Fully qualified name via the function table; NULL when the table is unavailable.",
            ),
            nullable(
                "definition_key",
                DataType::Utf8,
                "Stable logical function identity across revisions.",
            ),
            nullable(
                "kind",
                DataType::Utf8,
                "bytecode|sysop|native|native_unresolved via the function table.",
            ),
            nullable(
                "origin",
                DataType::Utf8,
                "user|companion|internal|builtin|auto_derive via the function table.",
            ),
            nullable(
                "call_site_file",
                DataType::Utf8,
                "Call-site source file (CallSiteSourceSpan + file table).",
            ),
            nullable(
                "call_site_line",
                DataType::UInt32,
                "1-based call-site line.",
            ),
            nullable(
                "call_site_start",
                DataType::UInt32,
                "Call-site span start offset.",
            ),
            nullable(
                "call_site_end",
                DataType::UInt32,
                "Call-site span end offset.",
            ),
            col("edge_kind", DataType::Utf8, "root|call|spawn."),
            col(
                "calls_started",
                DataType::UInt64,
                "Total entries (rate/mean denominator).",
            ),
            col(
                "calls_selected",
                DataType::UInt64,
                "Capture-policy selections; ≥ retained calls rows when records were lost.",
            ),
            col("completed_ok", DataType::UInt64, "Terminal successes."),
            col("completed_error", DataType::UInt64, "Terminal errors."),
            col(
                "completed_cancelled",
                DataType::UInt64,
                "Terminal cancellations (distinct from failure).",
            ),
            col(
                "completed_exit",
                DataType::UInt64,
                "Other explicit terminal exits.",
            ),
            col(
                "inclusive_ns",
                DataType::UInt64,
                "Function plus nested calls.",
            ),
            col(
                "direct_child_ns",
                DataType::UInt64,
                "Time attributed to direct children.",
            ),
            col("await_ns", DataType::UInt64, "Suspended/waiting time."),
            col(
                "self_ns",
                DataType::UInt64,
                "Direct execution only (derived at fold time).",
            ),
            col("await_count", DataType::UInt64, "Await suspensions."),
            col(
                "timing_complete",
                DataType::Boolean,
                "Timing is exact: no counter loss and no u64 saturation.",
            ),
            nullable(
                "overflow_reason",
                DataType::Utf8,
                "Non-NULL only on synthetic overflow rows.",
            ),
        ],
    }
}

fn calls_v1() -> RelationDef {
    RelationDef {
        name: "calls_v1",
        alias: "calls",
        secondary_alias: Some("retained_calls"),
        grain: Grain::RetainedCall,
        provisional: false,
        visibility: Visibility::Public,
        doc: "One row per individually retained call span. Bounded by \
              capture policy — never implies it contains all calls; \
              population totals live in call_path_stats_v1. args/output/error \
              are virtual value fields hydrated on demand; args is a \
              named-argument object (args['customer']).",
        columns: vec![
            key("execution_id", DataType::Utf8, "The root thread's id."),
            key(
                "call_id",
                DataType::Utf8,
                "Exact invocation identity (baml_call_1_… wire form).",
            ),
            nullable(
                "parent_call_id",
                DataType::Utf8,
                "Exact parentage when known; the parent itself may not be retained.",
            ),
            col(
                "thread_id",
                DataType::Utf8,
                "The logical thread this span ran on.",
            ),
            nullable(
                "call_path_id",
                DataType::Utf8,
                "The call's path; NULL for overflow buckets.",
            ),
            nullable(
                "call_path_overflow_reason",
                DataType::Utf8,
                "Why the call's path is an overflow bucket, when it is.",
            ),
            col(
                "function_id",
                DataType::UInt32,
                "Function identity within the execution's program.",
            ),
            nullable("fqn", DataType::Utf8, "Via the function table."),
            nullable(
                "definition_key",
                DataType::Utf8,
                "Stable logical function identity across revisions.",
            ),
            nullable(
                "kind",
                DataType::Utf8,
                "bytecode|sysop|native|native_unresolved via the function table.",
            ),
            col("edge_kind", DataType::Utf8, "root|call|spawn."),
            nullable("call_site_file", DataType::Utf8, "Call-site source file."),
            nullable(
                "call_site_line",
                DataType::UInt32,
                "1-based call-site line.",
            ),
            nullable(
                "call_site_start",
                DataType::UInt32,
                "Call-site span start offset.",
            ),
            nullable(
                "call_site_end",
                DataType::UInt32,
                "Call-site span end offset.",
            ),
            col(
                "started_ns",
                DataType::UInt64,
                "Process-relative start (SpanStart).",
            ),
            nullable(
                "ended_ns",
                DataType::UInt64,
                "Process-relative end; NULL without an end fact.",
            ),
            nullable(
                "duration_ns",
                DataType::UInt64,
                "Inclusive span duration; NULL without an end fact.",
            ),
            nullable("started_at", ts(), "Wall clock via the stream header."),
            nullable("ended_at", ts(), "Wall clock via the stream header."),
            nullable(
                "status",
                DataType::Utf8,
                "ok|errored|cancelled|exited (SpanEnd); NULL = no end fact.",
            ),
            utf8_list(
                "selection_reasons",
                "Why this span was retained: root|llm|manual.",
            ),
            utf8_list(
                "roles",
                "Captured roles present on this span: input|output|error.",
            ),
            utf8_list(
                "runtime_ids",
                "Initial + SpanRuntimeId overrides (baml_id_1_…), in order.",
            ),
            col(
                "args_state",
                DataType::Utf8,
                "available|not_captured|lost:<reason>|not_applicable.",
            ),
            col(
                "output_state",
                DataType::Utf8,
                "available|not_captured|lost:<reason>|not_applicable.",
            ),
            col(
                "error_state",
                DataType::Utf8,
                "available|not_captured|lost:<reason>|not_applicable.",
            ),
            nullable(
                "args_cid",
                DataType::Utf8,
                "Resident value identity (bamlv_1_…), joinable without hydration.",
            ),
            nullable("output_cid", DataType::Utf8, "Resident value identity."),
            nullable("error_cid", DataType::Utf8, "Resident value identity."),
            value(
                "args",
                "input",
                "Virtual: the named-argument object; args['name'] selects a parameter.",
            ),
            value("output", "output", "Virtual: the captured output value."),
            value("error", "error", "Virtual: the captured error value."),
            nullable(
                "error_id",
                DataType::Utf8,
                "The span's terminal error capture, when linked.",
            ),
            nullable(
                "error_lost_reason",
                DataType::Utf8,
                "Why the terminal error link is unavailable (TerminalErrorRef::Lost).",
            ),
        ],
    }
}

fn errors_v1() -> RelationDef {
    RelationDef {
        name: "errors_v1",
        alias: "errors",
        secondary_alias: None,
        grain: Grain::Error,
        provisional: false,
        visibility: Visibility::Public,
        doc: "One row per captured error (ErrorCapture).",
        columns: vec![
            key("execution_id", DataType::Utf8, "The root thread's id."),
            key("error_id", DataType::Utf8, "The capture's identity."),
            col(
                "throw_call_id",
                DataType::Utf8,
                "The call that raised the error.",
            ),
            col(
                "throw_thread_id",
                DataType::Utf8,
                "The thread that raised the error.",
            ),
            nullable(
                "throw_call_path_id",
                DataType::Utf8,
                "The raising call's path; NULL for overflow buckets.",
            ),
            col(
                "throw_function_id",
                DataType::UInt32,
                "The raising function's id.",
            ),
            nullable("throw_fqn", DataType::Utf8, "Via the function table."),
            nullable("throw_site_file", DataType::Utf8, "Throw-site source file."),
            nullable(
                "throw_site_line",
                DataType::UInt32,
                "1-based throw-site line.",
            ),
            nullable(
                "throw_site_start",
                DataType::UInt32,
                "Throw-site span start offset.",
            ),
            nullable(
                "throw_site_end",
                DataType::UInt32,
                "Throw-site span end offset.",
            ),
            col("kind", DataType::Utf8, "fresh|rethrow."),
            col(
                "source",
                DataType::Utf8,
                "bytecode|native_call|engine_call|future_resume.",
            ),
            col(
                "value_state",
                DataType::Utf8,
                "available|not_captured|lost:<reason>.",
            ),
            nullable(
                "value_cid",
                DataType::Utf8,
                "Resident value identity (bamlv_1_…).",
            ),
            value("value", "error", "Virtual: the captured error value."),
            col(
                "stack_complete",
                DataType::Boolean,
                "The stack covers root→throw without gaps.",
            ),
            utf8_list("stack", "Function fqns root→throw (error_stack)."),
            utf8_list(
                "terminal_call_ids",
                "Spans whose TerminalErrorRef targets this capture.",
            ),
        ],
    }
}

fn function_definitions_v1() -> RelationDef {
    RelationDef {
        name: "function_definitions_v1",
        alias: "function_definitions",
        secondary_alias: None,
        grain: Grain::ProgramFunction,
        provisional: false,
        visibility: Visibility::Public,
        doc: "Identity dictionary: one row per (program_id, function_id) \
              from the durable function table (FunctionTableV1). Joined \
              from call_path_stats/calls/errors/threads through the execution's \
              program_id.",
        columns: vec![
            key(
                "program_id",
                DataType::Utf8,
                "Scopes program-local function ids.",
            ),
            key(
                "function_id",
                DataType::UInt32,
                "Compact runtime identity emitted in call evidence.",
            ),
            col("fqn", DataType::Utf8, "Unambiguous user-facing name."),
            col(
                "display_name",
                DataType::Utf8,
                "Concise label for tree/list views.",
            ),
            nullable(
                "definition_key",
                DataType::Utf8,
                "Logical identity across revisions; a rename changes it.",
            ),
            col(
                "kind",
                DataType::Utf8,
                "bytecode|sysop|native|native_unresolved — execution kinds whose timing must not be conflated.",
            ),
            nullable("kind_detail", DataType::Utf8, "Kind-specific detail."),
            col(
                "origin",
                DataType::Utf8,
                "user|companion|internal|builtin|auto_derive.",
            ),
            nullable(
                "source_file",
                DataType::Utf8,
                "Project-relative definition file.",
            ),
            nullable(
                "source_start",
                DataType::UInt32,
                "Definition start offset for editor navigation.",
            ),
            nullable("source_end", DataType::UInt32, "Definition end offset."),
            nullable("package", DataType::Utf8, "Owning package, when any."),
            col("namespace", DataType::Utf8, "Owning namespace."),
            nullable(
                "revision_id",
                DataType::Utf8,
                "Compiled revision identity, when recorded.",
            ),
            nullable(
                "source_label",
                DataType::Utf8,
                "Human source label from EngineStarted.",
            ),
        ],
    }
}

fn health_v1() -> RelationDef {
    RelationDef {
        name: "health_v1",
        alias: "health",
        secondary_alias: None,
        grain: Grain::HealthMetric,
        provisional: false,
        visibility: Visibility::Public,
        doc: "Long-format health: one row per (execution_id, metric) — \
              the ExecutionHealthSnapshot counters from RootEnded, the \
              CounterHealth flags, one row per overflow bucket, the data \
              file count, and per-DataIssue rows from the fold.",
        columns: vec![
            key("execution_id", DataType::Utf8, "The root thread's id."),
            key("metric", DataType::Utf8, "Counter/flag/issue name."),
            col(
                "plane",
                DataType::Utf8,
                "execution|cct|overflow|process|data.",
            ),
            col("value", DataType::UInt64, "Counter value (flags as 0/1)."),
            nullable(
                "edge_kind",
                DataType::Utf8,
                "For overflow rows: the overflowed edge.",
            ),
            nullable(
                "reason",
                DataType::Utf8,
                "For overflow/issue rows: the typed reason.",
            ),
        ],
    }
}

fn processes_v1() -> RelationDef {
    RelationDef {
        name: "processes_v1",
        alias: "processes",
        secondary_alias: None,
        grain: Grain::Stream,
        provisional: false,
        visibility: Visibility::Internal,
        doc: "Internal: one row per writing process — the row is what the \
              process wrote, so it outlives the process.",
        columns: vec![
            key("process_id", DataType::Utf8, "Process euid hex."),
            nullable("os_pid", DataType::UInt32, "OS pid from StreamStarted."),
            nullable(
                "zero_unix_ns",
                DataType::UInt64,
                "Process clock zero (StreamStarted); NULL if the header is missing.",
            ),
            nullable(
                "baml_version",
                DataType::Utf8,
                "Producing BAML version (StreamStarted).",
            ),
            nullable("os_arch", DataType::Utf8, "Producing platform."),
            col(
                "alive",
                DataType::Boolean,
                "stream.lock held at bind time (streams spec §6.4).",
            ),
            col("meta_hw", DataType::UInt64, "Committed meta high-water."),
            col("data_hw", DataType::UInt64, "Committed data high-water."),
        ],
    }
}

fn store_files_v1() -> RelationDef {
    RelationDef {
        name: "store_files_v1",
        alias: "store_files",
        secondary_alias: None,
        grain: Grain::Segment,
        provisional: false,
        visibility: Visibility::Internal,
        doc: "Internal: one row per published store file.",
        columns: vec![
            key("process_id", DataType::Utf8, "Writing process."),
            key("plane", DataType::Utf8, "meta|data."),
            key(
                "sequence",
                DataType::UInt64,
                "File sequence number within its plane.",
            ),
            col("path", DataType::Utf8, "Store-relative file path."),
            nullable(
                "record_or_group_count",
                DataType::UInt64,
                "Records (meta) or groups (data); NULL when undecodable.",
            ),
            col("payload_len", DataType::UInt64, "Payload bytes."),
            col("checksum_ok", DataType::Boolean, "Checksum verified."),
            col("decode_ok", DataType::Boolean, "Payload decoded."),
        ],
    }
}

fn value_index_v1() -> RelationDef {
    RelationDef {
        name: "value_index_v1",
        alias: "value_index",
        secondary_alias: None,
        grain: Grain::CasObject,
        provisional: false,
        visibility: Visibility::Internal,
        doc: "Internal: one row per CAS object in the bound store.",
        columns: vec![
            key("cid", DataType::Utf8, "Content id (bamlv_1_… wire form)."),
            col("codec", DataType::UInt32, "Body codec."),
            col("body_len", DataType::UInt64, "Body bytes."),
            col("path", DataType::Utf8, "Store-relative file path."),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_v1_relations_and_keys_are_frozen() {
        let catalog = v1();
        let names: Vec<&str> = catalog.relations.iter().map(|r| r.name).collect();
        assert_eq!(
            names,
            vec![
                "threads_v1",
                "call_path_stats_v1",
                "calls_v1",
                "errors_v1",
                "function_definitions_v1",
                "health_v1",
                "processes_v1",
                "store_files_v1",
                "value_index_v1",
            ],
        );
        let threads = catalog.relation("threads_v1").unwrap();
        assert_eq!(threads.key_columns(), vec!["execution_id", "thread_id"]);
        let contexts = catalog.relation("cct_population").unwrap();
        assert_eq!(
            contexts.name, "call_path_stats_v1",
            "secondary alias resolves, version pinned"
        );
        assert_eq!(contexts.key_columns(), vec!["execution_id", "call_path_id"]);
        let calls = catalog.relation("retained_calls").unwrap();
        assert_eq!(calls.name, "calls_v1");
        assert_eq!(calls.key_columns(), vec!["execution_id", "call_id"]);
        let errors = catalog.relation("errors").unwrap();
        assert_eq!(errors.key_columns(), vec!["execution_id", "error_id"]);
        let functions = catalog.relation("function_definitions").unwrap();
        assert_eq!(functions.key_columns(), vec!["program_id", "function_id"]);
        let health = catalog.relation("health").unwrap();
        assert_eq!(health.key_columns(), vec!["execution_id", "metric"]);
    }

    #[test]
    fn virtual_value_columns_are_binary_handles_with_metadata() {
        let catalog = v1();
        let calls = catalog.relation("calls_v1").unwrap();
        let schema = calls.schema();
        for (name, role) in [("args", "input"), ("output", "output"), ("error", "error")] {
            let field = schema.field_with_name(name).unwrap();
            assert_eq!(field.data_type(), &DataType::Binary);
            assert!(field.is_nullable());
            assert_eq!(
                field.metadata().get(VALUE_META_KEY).map(String::as_str),
                Some(VALUE_META_VALUE),
            );
            assert_eq!(
                field.metadata().get(VALUE_ROLE_KEY).map(String::as_str),
                Some(role),
            );
        }
        // Exactly the three roles are virtual; everything else is resident.
        let virtuals = calls
            .columns
            .iter()
            .filter(|c| c.value_role.is_some())
            .count();
        assert_eq!(virtuals, 3);
        // errors_v1 carries exactly one virtual column.
        let errors = catalog.relation("errors_v1").unwrap();
        let virtuals: Vec<&str> = errors
            .columns
            .iter()
            .filter(|c| c.value_role.is_some())
            .map(|c| c.name)
            .collect();
        assert_eq!(virtuals, vec!["value"]);
    }

    #[test]
    fn profiles_gate_internal_relations() {
        let public = CatalogProfile::public();
        assert!(public.relation("store_files_v1").is_none());
        assert!(public.relation("threads").is_some());
        let internal = CatalogProfile::internal();
        assert!(internal.relation("store_files_v1").is_some());
        assert!(internal.relation("processes").is_some());
        assert!(internal.relation("value_index").is_some());

        // Overrides: hide a relation, hide a column, expose an internal.
        let profile = CatalogProfile::public().with_overrides(vec![
            Override::HideRelation("errors"),
            Override::HideColumn("calls", "args"),
            Override::ExposeInternal("processes_v1"),
        ]);
        assert!(profile.relation("errors_v1").is_none());
        assert!(profile.relation("processes").is_some());
        let calls = profile.relation("calls").unwrap();
        assert!(calls.column("args").is_none());
        assert!(calls.column("output").is_some());
    }

    #[test]
    fn no_executions_view_ships_in_v1() {
        // Decided 2026-08-24: root threads are selected with
        // `WHERE parent_thread_id IS NULL`; adding the view later is
        // additive.
        let catalog = v1();
        assert!(catalog.view("executions").is_none());
        assert!(catalog.relation("executions").is_none());
        let views: Vec<&str> = catalog.views.iter().map(|v| v.alias).collect();
        assert_eq!(views, vec!["hot_call_paths"]);
        // llm_calls stays out until a producer emits an LLM function kind.
        assert!(catalog.view("llm_calls").is_none());
    }

    /// The frozen column golden: any addition, removal, reorder, or
    /// retype of a catalog-v1 column must be a deliberate edit HERE.
    #[test]
    fn catalog_v1_column_golden() {
        let catalog = v1();
        let render = |name: &str| -> Vec<String> {
            catalog
                .relation(name)
                .unwrap()
                .columns
                .iter()
                .map(|c| {
                    format!(
                        "{}:{:?}{}{}",
                        c.name,
                        c.data_type,
                        if c.nullable { "?" } else { "" },
                        if c.value_role.is_some() {
                            ":virtual"
                        } else {
                            ""
                        },
                    )
                })
                .collect()
        };
        assert_eq!(
            render("threads_v1"),
            [
                "execution_id:Utf8",
                "thread_id:Utf8",
                "parent_thread_id:Utf8?",
                "spawn_call_id:Utf8?",
                "spawn_function_id:UInt32?",
                "spawn_fqn:Utf8?",
                "spawn_site_file:Utf8?",
                "spawn_site_line:UInt32?",
                "name:Utf8?",
                "kind:Utf8",
                "started_ns:UInt64",
                "ended_ns:UInt64?",
                "started_at:Timestamp(Nanosecond, Some(\"UTC\"))?",
                "ended_at:Timestamp(Nanosecond, Some(\"UTC\"))?",
                "end_status:Utf8?",
                "process_id:Utf8?",
                "engine_id:UInt64?",
                "program_id:Utf8?",
                "revision_id:Utf8?",
                "source_label:Utf8?",
                "runtime_id:Utf8?",
                "entry_function_id:UInt32?",
                "entry_fqn:Utf8?",
                "status:Utf8?",
                "index_state:Utf8?",
                "duration_ns:UInt64?",
                "total_calls:UInt64?",
                "total_errors:UInt64?",
                "total_cancelled:UInt64?",
                "calls_retained:UInt64?",
                "threads_total:UInt64?",
                "value_state:Utf8?",
                "data_first_seq:UInt64?",
                "data_last_seq:UInt64?",
                "data_file_count:UInt64?",
            ]
        );
        assert_eq!(
            render("call_path_stats_v1"),
            [
                "execution_id:Utf8",
                "call_path_id:Utf8",
                "parent_call_path_id:Utf8?",
                "depth:UInt32",
                "function_id:UInt32",
                "fqn:Utf8?",
                "definition_key:Utf8?",
                "kind:Utf8?",
                "origin:Utf8?",
                "call_site_file:Utf8?",
                "call_site_line:UInt32?",
                "call_site_start:UInt32?",
                "call_site_end:UInt32?",
                "edge_kind:Utf8",
                "calls_started:UInt64",
                "calls_selected:UInt64",
                "completed_ok:UInt64",
                "completed_error:UInt64",
                "completed_cancelled:UInt64",
                "completed_exit:UInt64",
                "inclusive_ns:UInt64",
                "direct_child_ns:UInt64",
                "await_ns:UInt64",
                "self_ns:UInt64",
                "await_count:UInt64",
                "timing_complete:Boolean",
                "overflow_reason:Utf8?",
            ]
        );
        assert_eq!(
            render("calls_v1"),
            [
                "execution_id:Utf8",
                "call_id:Utf8",
                "parent_call_id:Utf8?",
                "thread_id:Utf8",
                "call_path_id:Utf8?",
                "call_path_overflow_reason:Utf8?",
                "function_id:UInt32",
                "fqn:Utf8?",
                "definition_key:Utf8?",
                "kind:Utf8?",
                "edge_kind:Utf8",
                "call_site_file:Utf8?",
                "call_site_line:UInt32?",
                "call_site_start:UInt32?",
                "call_site_end:UInt32?",
                "started_ns:UInt64",
                "ended_ns:UInt64?",
                "duration_ns:UInt64?",
                "started_at:Timestamp(Nanosecond, Some(\"UTC\"))?",
                "ended_at:Timestamp(Nanosecond, Some(\"UTC\"))?",
                "status:Utf8?",
                "selection_reasons:List(Field { data_type: Utf8 })",
                "roles:List(Field { data_type: Utf8 })",
                "runtime_ids:List(Field { data_type: Utf8 })",
                "args_state:Utf8",
                "output_state:Utf8",
                "error_state:Utf8",
                "args_cid:Utf8?",
                "output_cid:Utf8?",
                "error_cid:Utf8?",
                "args:Binary?:virtual",
                "output:Binary?:virtual",
                "error:Binary?:virtual",
                "error_id:Utf8?",
                "error_lost_reason:Utf8?",
            ]
        );
        assert_eq!(
            render("errors_v1"),
            [
                "execution_id:Utf8",
                "error_id:Utf8",
                "throw_call_id:Utf8",
                "throw_thread_id:Utf8",
                "throw_call_path_id:Utf8?",
                "throw_function_id:UInt32",
                "throw_fqn:Utf8?",
                "throw_site_file:Utf8?",
                "throw_site_line:UInt32?",
                "throw_site_start:UInt32?",
                "throw_site_end:UInt32?",
                "kind:Utf8",
                "source:Utf8",
                "value_state:Utf8",
                "value_cid:Utf8?",
                "value:Binary?:virtual",
                "stack_complete:Boolean",
                "stack:List(Field { data_type: Utf8 })",
                "terminal_call_ids:List(Field { data_type: Utf8 })",
            ]
        );
        assert_eq!(
            render("function_definitions_v1"),
            [
                "program_id:Utf8",
                "function_id:UInt32",
                "fqn:Utf8",
                "display_name:Utf8",
                "definition_key:Utf8?",
                "kind:Utf8",
                "kind_detail:Utf8?",
                "origin:Utf8",
                "source_file:Utf8?",
                "source_start:UInt32?",
                "source_end:UInt32?",
                "package:Utf8?",
                "namespace:Utf8",
                "revision_id:Utf8?",
                "source_label:Utf8?",
            ]
        );
        assert_eq!(
            render("health_v1"),
            [
                "execution_id:Utf8",
                "metric:Utf8",
                "plane:Utf8",
                "value:UInt64",
                "edge_kind:Utf8?",
                "reason:Utf8?",
            ]
        );
        assert_eq!(
            render("processes_v1"),
            [
                "process_id:Utf8",
                "os_pid:UInt32?",
                "zero_unix_ns:UInt64?",
                "baml_version:Utf8?",
                "os_arch:Utf8?",
                "alive:Boolean",
                "meta_hw:UInt64",
                "data_hw:UInt64",
            ]
        );
        assert_eq!(
            render("store_files_v1"),
            [
                "process_id:Utf8",
                "plane:Utf8",
                "sequence:UInt64",
                "path:Utf8",
                "record_or_group_count:UInt64?",
                "payload_len:UInt64",
                "checksum_ok:Boolean",
                "decode_ok:Boolean",
            ]
        );
        assert_eq!(
            render("value_index_v1"),
            ["cid:Utf8", "codec:UInt32", "body_len:UInt64", "path:Utf8",]
        );
    }

    #[test]
    fn every_column_documents_itself() {
        for relation in v1().relations {
            assert!(!relation.doc.is_empty(), "{} has no doc", relation.name);
            for column in &relation.columns {
                assert!(
                    !column.doc.is_empty(),
                    "{}.{} has no doc",
                    relation.name,
                    column.name
                );
            }
        }
        for view in v1().views {
            assert!(!view.doc.is_empty(), "{} has no doc", view.name);
        }
    }
}
