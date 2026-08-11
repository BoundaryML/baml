//! §8 BQL v1 — parser, typed planner, and executor.
//!
//! A query is a pipeline of typed stages: `source | transform | ... | sink`
//! (§8.1). Every stage's input/output set kind is checked at plan time
//! (§8.2); v1 implements three kinds (`RunSet`, `CtxSet`, `Table`) with
//! exactly two implicit coercions: `RunSet → CtxSet` (run-scoped `ctx()`,
//! single-run sets only — multi-run sets fail closed with
//! `E_MULTI_RUN_CTX`) and `X → Table` at pipeline end.
//!
//! Stage catalog v1:
//! - sources: `runs(last=, status=)`, `run(id=)`, `ctx()`
//! - `CtxSet` transforms: `calls(fn=)`, `errors()`, `rollup(by=fn)`,
//!   `where(<metric> <op> <value>)`, `sort(by=, desc|asc)`
//! - sinks: `top(k, by=)`, `stats()`, `limit(k)`
//!
//! §8.4 trust contract: every result is a [`BqlTable`] carrying a
//! [`Completeness`] footer (sealed/torn scan facts + degradation notes:
//! percentile mean-fallback, implicit coercions, implicit row caps).
//! Errors are typed [`BqlError`]s (`E_PARSE`, `E_UNKNOWN_STAGE`, `E_TYPE`,
//! `E_MULTI_RUN_CTX`, `E_BAD_ARG`) with a machine-actionable remedy.

use serde::Serialize;

use crate::bqf1::{self, Col, FrameKind};
use crate::cct::HIST_BUCKETS;
use crate::engine::ObserveEngine;
use crate::runs;

/// §8.4 implicit default row limit on materialized tables.
pub const DEFAULT_ROW_LIMIT: usize = 1000;

// ---------------------------------------------------------------------------
// Public result / error types
// ---------------------------------------------------------------------------

/// Typed, fail-closed BQL error (§8.4): a stable code, a human message,
/// and a ready-to-act remedy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BqlError {
    /// `E_PARSE` | `E_UNKNOWN_STAGE` | `E_TYPE` | `E_MULTI_RUN_CTX` | `E_BAD_ARG`.
    pub code: &'static str,
    pub message: String,
    pub remedy: String,
}

impl BqlError {
    fn new(code: &'static str, message: impl Into<String>, remedy: impl Into<String>) -> BqlError {
        BqlError {
            code,
            message: message.into(),
            remedy: remedy.into(),
        }
    }

    fn parse(message: impl Into<String>) -> BqlError {
        BqlError::new(
            "E_PARSE",
            message,
            "BQL syntax is stage(arg, key=value) | stage(...) | ...; strings are double-quoted, durations look like 30s/5m/24h/7d",
        )
    }

    fn bad_arg(message: impl Into<String>, remedy: impl Into<String>) -> BqlError {
        BqlError::new("E_BAD_ARG", message, remedy)
    }
}

impl std::fmt::Display for BqlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {} (remedy: {})",
            self.code, self.message, self.remedy
        )
    }
}

/// One column of result data.
#[derive(Debug, Clone, PartialEq)]
pub enum ColData {
    U32(Vec<u32>),
    U64(Vec<u64>),
    F64(Vec<f64>),
    Str(Vec<String>),
    /// Serialized JSON per cell (hydrated values): renders as an object in
    /// JSON output, as a truncated preview in tables, as Str on the wire.
    Json(Vec<String>),
}

impl ColData {
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            ColData::U32(v) => v.len(),
            ColData::U64(v) => v.len(),
            ColData::F64(v) => v.len(),
            ColData::Str(v) | ColData::Json(v) => v.len(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn type_name(&self) -> &'static str {
        match self {
            ColData::U32(_) => "u32",
            ColData::U64(_) => "u64",
            ColData::F64(_) => "f64",
            ColData::Str(_) => "str",
            ColData::Json(_) => "json",
        }
    }

    fn truncate(&mut self, k: usize) {
        match self {
            ColData::U32(v) => v.truncate(k),
            ColData::U64(v) => v.truncate(k),
            ColData::F64(v) => v.truncate(k),
            ColData::Str(v) | ColData::Json(v) => v.truncate(k),
        }
    }
}

/// §8.4 completeness footer — mandatory on every result, computed from the
/// blocks the query actually touched.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Completeness {
    /// All consulted segments carried a sealed end marker.
    pub sealed: bool,
    /// At least one consulted segment was torn / undecodable at its tail.
    pub torn: bool,
    pub first_ts_ns: u64,
    pub last_ts_ns: u64,
    /// Honest degradation notes: percentile mean-fallback, implicit
    /// RunSet→CtxSet coercion, implicit row caps, empty-set explanations.
    pub degraded: Vec<String>,
}

/// A materialized BQL result: named typed columns + the mandatory footer.
#[derive(Debug, Clone, PartialEq)]
pub struct BqlTable {
    pub columns: Vec<(String, ColData)>,
    pub footer: Completeness,
}

impl BqlTable {
    /// Number of data rows.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.columns.first().map_or(0, |(_, c)| c.len())
    }

    /// Column by name.
    #[must_use]
    pub fn column(&self, name: &str) -> Option<&ColData> {
        self.columns.iter().find(|(n, _)| n == name).map(|(_, c)| c)
    }

    /// Encode as a [`FrameKind::BqlTable`] BQF1 frame (layout documented on
    /// the enum variant): data columns with a sentinel row 0, plus a final
    /// `Str` meta column whose row 0 is the column-schema + footer JSON.
    #[must_use]
    pub fn to_frame(&self, request_id: u64) -> Vec<u8> {
        enum Shifted {
            U32(Vec<u32>),
            U64(Vec<u64>),
            F64(Vec<f64>),
            Str(Vec<String>),
        }
        let n = self.rows();
        let mut shifted: Vec<Shifted> = Vec::with_capacity(self.columns.len());
        for (_, data) in &self.columns {
            shifted.push(match data {
                ColData::U32(v) => {
                    let mut s = Vec::with_capacity(v.len() + 1);
                    s.push(0);
                    s.extend_from_slice(v);
                    Shifted::U32(s)
                }
                ColData::U64(v) => {
                    let mut s = Vec::with_capacity(v.len() + 1);
                    s.push(0);
                    s.extend_from_slice(v);
                    Shifted::U64(s)
                }
                ColData::F64(v) => {
                    let mut s = Vec::with_capacity(v.len() + 1);
                    s.push(0.0);
                    s.extend_from_slice(v);
                    Shifted::F64(s)
                }
                ColData::Str(v) | ColData::Json(v) => {
                    let mut s = Vec::with_capacity(v.len() + 1);
                    s.push(String::new());
                    s.extend(v.iter().cloned());
                    Shifted::Str(s)
                }
            });
        }
        let cols_meta: Vec<serde_json::Value> = self
            .columns
            .iter()
            .map(|(name, c)| serde_json::json!({ "name": name, "type": c.type_name() }))
            .collect();
        let meta = serde_json::json!({
            "columns": cols_meta,
            "rows": n,
            "footer": &self.footer,
        });
        let mut meta_col = vec![serde_json::to_string(&meta).unwrap_or_default()];
        meta_col.resize(n + 1, String::new());

        let mut cols: Vec<Col<'_>> = shifted
            .iter()
            .map(|s| match s {
                Shifted::U32(v) => Col::U32(v),
                Shifted::U64(v) => Col::U64(v),
                Shifted::F64(v) => Col::F64(v),
                Shifted::Str(v) => Col::Str(v),
            })
            .collect();
        cols.push(Col::Str(&meta_col));
        let flags = if self.footer.torn || !self.footer.sealed {
            bqf1::FLAG_PARTIAL_TAIL
        } else {
            0
        };
        bqf1::encode_frame(FrameKind::BqlTable, flags, request_id, 0, &cols)
    }
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Int(i64),
    Float(f64),
    Str(String),
    /// Duration literal, already in nanoseconds.
    Dur(u64),
    /// Byte-size literal (`64kb`), already in bytes.
    Size(u64),
    Pipe,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    /// `=` (named argument).
    Assign,
    Cmp(CmpOp),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CmpOp {
    Gt,
    Ge,
    Lt,
    Le,
    Eq,
    Ne,
}

impl CmpOp {
    fn eval(self, lhs: u64, rhs: u64) -> bool {
        match self {
            CmpOp::Gt => lhs > rhs,
            CmpOp::Ge => lhs >= rhs,
            CmpOp::Lt => lhs < rhs,
            CmpOp::Le => lhs <= rhs,
            CmpOp::Eq => lhs == rhs,
            CmpOp::Ne => lhs != rhs,
        }
    }
}

const DURATION_UNITS: &[(&str, u64)] = &[
    ("ns", 1),
    ("us", 1_000),
    ("ms", 1_000_000),
    ("s", 1_000_000_000),
    ("m", 60 * 1_000_000_000),
    ("h", 3_600 * 1_000_000_000),
    ("d", 86_400 * 1_000_000_000),
];

const SIZE_UNITS: &[(&str, u64)] = &[
    ("b", 1),
    ("kb", 1024),
    ("mb", 1024 * 1024),
    ("gb", 1024 * 1024 * 1024),
];

fn lex(src: &str) -> Result<Vec<(Tok, usize)>, BqlError> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let at = i;
        let b = bytes[i];
        match b {
            b' ' | b'\t' | b'\r' | b'\n' => i += 1,
            b'|' => {
                out.push((Tok::Pipe, at));
                i += 1;
            }
            b'(' => {
                out.push((Tok::LParen, at));
                i += 1;
            }
            b')' => {
                out.push((Tok::RParen, at));
                i += 1;
            }
            b'[' => {
                out.push((Tok::LBracket, at));
                i += 1;
            }
            b']' => {
                out.push((Tok::RBracket, at));
                i += 1;
            }
            b',' => {
                out.push((Tok::Comma, at));
                i += 1;
            }
            b'=' => {
                if bytes.get(i + 1) == Some(&b'=') {
                    out.push((Tok::Cmp(CmpOp::Eq), at));
                    i += 2;
                } else {
                    out.push((Tok::Assign, at));
                    i += 1;
                }
            }
            b'!' => {
                if bytes.get(i + 1) == Some(&b'=') {
                    out.push((Tok::Cmp(CmpOp::Ne), at));
                    i += 2;
                } else {
                    return Err(BqlError::parse(format!("unexpected '!' at byte {at}")));
                }
            }
            b'>' => {
                if bytes.get(i + 1) == Some(&b'=') {
                    out.push((Tok::Cmp(CmpOp::Ge), at));
                    i += 2;
                } else {
                    out.push((Tok::Cmp(CmpOp::Gt), at));
                    i += 1;
                }
            }
            b'<' => {
                if bytes.get(i + 1) == Some(&b'=') {
                    out.push((Tok::Cmp(CmpOp::Le), at));
                    i += 2;
                } else {
                    out.push((Tok::Cmp(CmpOp::Lt), at));
                    i += 1;
                }
            }
            b'"' => {
                i += 1;
                let mut s = String::new();
                loop {
                    match bytes.get(i) {
                        None => {
                            return Err(BqlError::parse(format!(
                                "unterminated string starting at byte {at}"
                            )));
                        }
                        Some(b'"') => {
                            i += 1;
                            break;
                        }
                        Some(b'\\') => match bytes.get(i + 1) {
                            Some(b'"') => {
                                s.push('"');
                                i += 2;
                            }
                            Some(b'\\') => {
                                s.push('\\');
                                i += 2;
                            }
                            _ => {
                                return Err(BqlError::parse(format!(
                                    "unsupported escape in string at byte {i}"
                                )));
                            }
                        },
                        Some(_) => {
                            // Advance one whole UTF-8 char.
                            let rest = &src[i..];
                            let ch = rest.chars().next().unwrap_or('\u{fffd}');
                            s.push(ch);
                            i += ch.len_utf8();
                        }
                    }
                }
                out.push((Tok::Str(s), at));
            }
            b'0'..=b'9' => {
                let start = i;
                let mut saw_dot = false;
                while i < bytes.len()
                    && (bytes[i].is_ascii_digit() || (bytes[i] == b'.' && !saw_dot))
                {
                    if bytes[i] == b'.' {
                        // A dot must be followed by a digit to be a float.
                        if !bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
                            break;
                        }
                        saw_dot = true;
                    }
                    i += 1;
                }
                let num = &src[start..i];
                let unit_start = i;
                while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let unit = &src[unit_start..i];
                if unit.is_empty() {
                    if saw_dot {
                        let f: f64 = num.parse().map_err(|_| {
                            BqlError::parse(format!("bad number '{num}' at byte {start}"))
                        })?;
                        out.push((Tok::Float(f), at));
                    } else {
                        let v: i64 = num.parse().map_err(|_| {
                            BqlError::parse(format!("bad integer '{num}' at byte {start}"))
                        })?;
                        out.push((Tok::Int(v), at));
                    }
                } else {
                    let Some(&(_, unit_ns)) = DURATION_UNITS.iter().find(|(u, _)| *u == unit)
                    else {
                        // Byte-size literal (`64kb`, `4mb`) — §8.4 budgets.
                        if let Some(&(_, mult)) = SIZE_UNITS
                            .iter()
                            .find(|(u, _)| *u == unit.to_ascii_lowercase())
                        {
                            if saw_dot {
                                return Err(BqlError::parse(format!(
                                    "byte sizes are integers ('{num}{unit}' at byte {start})"
                                )));
                            }
                            let v: u64 = num.parse().map_err(|_| {
                                BqlError::parse(format!("bad integer '{num}' at byte {start}"))
                            })?;
                            let bytes = v.checked_mul(mult).ok_or_else(|| {
                                BqlError::parse(format!(
                                    "size '{num}{unit}' overflows at byte {start}"
                                ))
                            })?;
                            out.push((Tok::Size(bytes), at));
                            i = i.max(unit_start + unit.len());
                            continue;
                        }
                        return Err(BqlError::parse(format!(
                            "unknown unit '{unit}' at byte {unit_start} (durations: ns/us/ms/s/m/h/d; sizes: b/kb/mb/gb)"
                        )));
                    };
                    let ns = if saw_dot {
                        let f: f64 = num.parse().map_err(|_| {
                            BqlError::parse(format!("bad number '{num}' at byte {start}"))
                        })?;
                        #[expect(
                            clippy::cast_precision_loss,
                            clippy::cast_possible_truncation,
                            clippy::cast_sign_loss,
                            reason = "range-checked ns rounding of a human duration literal"
                        )]
                        {
                            let scaled = f * unit_ns as f64;
                            if !(0.0..=u64::MAX as f64).contains(&scaled) {
                                return Err(BqlError::parse(format!(
                                    "duration '{num}{unit}' out of range at byte {start}"
                                )));
                            }
                            scaled as u64
                        }
                    } else {
                        let v: u64 = num.parse().map_err(|_| {
                            BqlError::parse(format!("bad integer '{num}' at byte {start}"))
                        })?;
                        v.checked_mul(unit_ns).ok_or_else(|| {
                            BqlError::parse(format!(
                                "duration '{num}{unit}' overflows at byte {start}"
                            ))
                        })?
                    };
                    out.push((Tok::Dur(ns), at));
                }
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                out.push((Tok::Ident(src[start..i].to_string()), at));
            }
            other => {
                return Err(BqlError::parse(format!(
                    "unexpected character '{}' at byte {at}",
                    char::from(other)
                )));
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Parser (AST)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum ValueAst {
    Int(i64),
    Float(f64),
    Str(String),
    Ident(String),
    Dur(u64),
    Size(u64),
    List(Vec<ValueAst>),
}

impl ValueAst {
    fn describe(&self) -> String {
        match self {
            ValueAst::Int(v) => format!("integer {v}"),
            ValueAst::Float(v) => format!("float {v}"),
            ValueAst::Str(s) => format!("string \"{s}\""),
            ValueAst::Ident(s) => format!("identifier {s}"),
            ValueAst::Dur(ns) => format!("duration {ns}ns"),
            ValueAst::Size(b) => format!("size {b}b"),
            ValueAst::List(items) => format!("list of {} value(s)", items.len()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ArgAst {
    Pos(ValueAst),
    Named(String, ValueAst),
    Cmp {
        metric: String,
        op: CmpOp,
        value: ValueAst,
    },
}

#[derive(Debug, Clone, PartialEq)]
struct StageAst {
    name: String,
    args: Vec<ArgAst>,
}

struct Parser {
    toks: Vec<(Tok, usize)>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i).map(|(t, _)| t)
    }

    fn peek2(&self) -> Option<&Tok> {
        self.toks.get(self.i + 1).map(|(t, _)| t)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.i).map(|(t, _)| t.clone());
        self.i += 1;
        t
    }

    fn pos(&self) -> String {
        self.toks.get(self.i).map_or_else(
            || "end of query".to_string(),
            |(_, at)| format!("byte {at}"),
        )
    }

    fn value(&mut self) -> Result<ValueAst, BqlError> {
        let pos = self.pos();
        match self.next() {
            Some(Tok::Int(v)) => Ok(ValueAst::Int(v)),
            Some(Tok::Float(v)) => Ok(ValueAst::Float(v)),
            Some(Tok::Str(s)) => Ok(ValueAst::Str(s)),
            Some(Tok::Ident(s)) => Ok(ValueAst::Ident(s)),
            Some(Tok::Dur(ns)) => Ok(ValueAst::Dur(ns)),
            Some(Tok::Size(b)) => Ok(ValueAst::Size(b)),
            Some(Tok::LBracket) => {
                let mut items = Vec::new();
                if self.peek() == Some(&Tok::RBracket) {
                    self.next();
                    return Ok(ValueAst::List(items));
                }
                loop {
                    items.push(self.value()?);
                    let pos = self.pos();
                    match self.next() {
                        Some(Tok::Comma) => {}
                        Some(Tok::RBracket) => break,
                        _ => {
                            return Err(BqlError::parse(format!(
                                "expected ',' or ']' in list at {pos}"
                            )));
                        }
                    }
                }
                Ok(ValueAst::List(items))
            }
            _ => Err(BqlError::parse(format!("expected a value at {pos}"))),
        }
    }

    fn arg(&mut self) -> Result<ArgAst, BqlError> {
        if let Some(Tok::Ident(name)) = self.peek() {
            let name = name.clone();
            match self.peek2() {
                Some(Tok::Assign) => {
                    self.next();
                    self.next();
                    return Ok(ArgAst::Named(name, self.value()?));
                }
                Some(Tok::Cmp(op)) => {
                    let op = *op;
                    self.next();
                    self.next();
                    return Ok(ArgAst::Cmp {
                        metric: name,
                        op,
                        value: self.value()?,
                    });
                }
                _ => {}
            }
        }
        Ok(ArgAst::Pos(self.value()?))
    }

    fn stage(&mut self) -> Result<StageAst, BqlError> {
        let pos = self.pos();
        let Some(Tok::Ident(name)) = self.next() else {
            return Err(BqlError::parse(format!("expected a stage name at {pos}")));
        };
        let pos = self.pos();
        if self.next() != Some(Tok::LParen) {
            return Err(BqlError::parse(format!(
                "expected '(' after '{name}' at {pos}"
            )));
        }
        let mut args = Vec::new();
        if self.peek() == Some(&Tok::RParen) {
            self.next();
            return Ok(StageAst { name, args });
        }
        loop {
            args.push(self.arg()?);
            let pos = self.pos();
            match self.next() {
                Some(Tok::Comma) => {}
                Some(Tok::RParen) => break,
                _ => {
                    return Err(BqlError::parse(format!(
                        "expected ',' or ')' in '{name}(...)' at {pos}"
                    )));
                }
            }
        }
        Ok(StageAst { name, args })
    }
}

fn parse(src: &str) -> Result<Vec<StageAst>, BqlError> {
    let toks = lex(src)?;
    if toks.is_empty() {
        return Err(BqlError::parse("empty query"));
    }
    let mut p = Parser { toks, i: 0 };
    let mut stages = vec![p.stage()?];
    while p.peek() == Some(&Tok::Pipe) {
        p.next();
        stages.push(p.stage()?);
    }
    if p.peek().is_some() {
        return Err(BqlError::parse(format!(
            "unexpected trailing input at {} (stages join with '|')",
            p.pos()
        )));
    }
    Ok(stages)
}

// ---------------------------------------------------------------------------
// Planner (typed set kinds, §8.2)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetKind {
    RunSet,
    CtxSet,
    ValueSet,
    Table,
}

fn kind_name(kind: Option<SetKind>) -> &'static str {
    match kind {
        None => "nothing (source position)",
        Some(SetKind::RunSet) => "RunSet",
        Some(SetKind::CtxSet) => "CtxSet",
        Some(SetKind::ValueSet) => "ValueSet",
        Some(SetKind::Table) => "Table",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Metric {
    Calls,
    TotalNs,
    SelfNs,
    Errors,
    P50,
    P95,
    P99,
}

const METRIC_NAMES: &str = "calls|total_ns|self_ns|errors|p50|p95|p99";

fn metric_from(name: &str) -> Option<Metric> {
    match name {
        "calls" => Some(Metric::Calls),
        "total_ns" => Some(Metric::TotalNs),
        "self_ns" => Some(Metric::SelfNs),
        "errors" => Some(Metric::Errors),
        "p50" => Some(Metric::P50),
        "p95" => Some(Metric::P95),
        "p99" => Some(Metric::P99),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
enum PlanStage {
    Runs {
        last_ns: Option<u64>,
        status: Option<String>,
    },
    RunId {
        id: String,
    },
    Ctx,
    Calls {
        pattern: String,
    },
    Errors,
    Rollup,
    Where {
        metric: Metric,
        op: CmpOp,
        value: u64,
    },
    Sort {
        by: Metric,
        desc: bool,
    },
    Top {
        k: usize,
        by: Metric,
    },
    Stats,
    /// `stats(by=cid)` over a ValueSet: dedupe view (§8.5).
    StatsByCid,
    Limit {
        k: usize,
    },
    /// §8.2 ValueSet source: a run's captured values.
    Values {
        roles: Vec<String>,
        fn_pattern: Option<String>,
    },
    /// §8.4 bounded hydration sink.
    Get {
        max_bytes: usize,
        max_depth: u32,
    },
    /// §8.2 exact-instance gate.
    Instances,
    /// §8.5 verify-my-fix: two runs' outputs compared on matched inputs.
    VDiff {
        a: String,
        b: String,
    },
}

const STAGE_LIST: &str = "runs, run, ctx, calls, errors, rollup, where, sort, top, stats, limit, values, get, instances, vdiff";

/// Reject named keys outside `named`, more than `max_pos` positional args,
/// and comparison args unless `allow_cmp`.
fn check_args(
    stage: &StageAst,
    named: &[&str],
    max_pos: usize,
    allow_cmp: bool,
) -> Result<(), BqlError> {
    let mut pos_seen = 0usize;
    for arg in &stage.args {
        match arg {
            ArgAst::Named(key, _) => {
                if !named.contains(&key.as_str()) {
                    return Err(BqlError::bad_arg(
                        format!("unknown argument '{key}' for stage '{}'", stage.name),
                        if named.is_empty() {
                            format!("'{}' takes no named arguments", stage.name)
                        } else {
                            format!("'{}' accepts: {}", stage.name, named.join(", "))
                        },
                    ));
                }
            }
            ArgAst::Pos(_) => {
                pos_seen += 1;
                if pos_seen > max_pos {
                    return Err(BqlError::bad_arg(
                        format!(
                            "too many positional arguments for stage '{}' (max {max_pos})",
                            stage.name
                        ),
                        "use key=value for named arguments",
                    ));
                }
            }
            ArgAst::Cmp { .. } => {
                if !allow_cmp {
                    return Err(BqlError::bad_arg(
                        format!("stage '{}' does not take a comparison argument", stage.name),
                        "comparisons like calls > 1 belong in where(...)",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn named_value<'a>(stage: &'a StageAst, key: &str) -> Option<&'a ValueAst> {
    stage.args.iter().find_map(|a| match a {
        ArgAst::Named(k, v) if k == key => Some(v),
        _ => None,
    })
}

fn positional(stage: &StageAst) -> Vec<&ValueAst> {
    stage
        .args
        .iter()
        .filter_map(|a| match a {
            ArgAst::Pos(v) => Some(v),
            _ => None,
        })
        .collect()
}

fn metric_arg(stage: &StageAst, key: &str, default: Metric) -> Result<Metric, BqlError> {
    match named_value(stage, key) {
        None => Ok(default),
        Some(ValueAst::Ident(name)) => metric_from(name).ok_or_else(|| {
            BqlError::bad_arg(
                format!("unknown metric '{name}' in '{}({key}=...)'", stage.name),
                format!("metrics: {METRIC_NAMES}"),
            )
        }),
        Some(other) => Err(BqlError::bad_arg(
            format!(
                "'{}({key}=...)' expects a metric name, got {}",
                stage.name,
                other.describe()
            ),
            format!("metrics: {METRIC_NAMES}"),
        )),
    }
}

fn require_ctx_input(stage: &StageAst, kind: Option<SetKind>) -> Result<(), BqlError> {
    match kind {
        Some(SetKind::CtxSet | SetKind::RunSet) => Ok(()),
        other => Err(BqlError::new(
            "E_TYPE",
            format!(
                "stage '{}' expects CtxSet input (RunSet coerces), got {}",
                stage.name,
                kind_name(other)
            ),
            "start the pipeline with a source stage: ctx(), runs(...), or run(id=...)",
        )),
    }
}

fn require_source_position(stage: &StageAst, kind: Option<SetKind>) -> Result<(), BqlError> {
    if kind.is_none() {
        Ok(())
    } else {
        Err(BqlError::new(
            "E_TYPE",
            format!(
                "'{}' is a source stage and must start the pipeline, got {} input",
                stage.name,
                kind_name(kind)
            ),
            "move the source to the front: source | transform | sink",
        ))
    }
}

fn positive_int(stage: &StageAst, value: &ValueAst, what: &str) -> Result<usize, BqlError> {
    match value {
        ValueAst::Int(v) if *v >= 1 => usize::try_from(*v).map_err(|_| {
            BqlError::bad_arg(
                format!("'{}' {what} is out of range", stage.name),
                "use a smaller count",
            )
        }),
        other => Err(BqlError::bad_arg(
            format!(
                "'{}' expects a positive integer {what}, got {}",
                stage.name,
                other.describe()
            ),
            format!("example: {}(10)", stage.name),
        )),
    }
}

fn plan_stage(stage: &StageAst, kind: Option<SetKind>) -> Result<(PlanStage, SetKind), BqlError> {
    match stage.name.as_str() {
        "runs" => {
            require_source_position(stage, kind)?;
            check_args(stage, &["last", "status"], 0, false)?;
            let last_ns = match named_value(stage, "last") {
                None => None,
                Some(ValueAst::Dur(ns)) => Some(*ns),
                Some(other) => {
                    return Err(BqlError::bad_arg(
                        format!(
                            "'runs(last=...)' expects a duration, got {}",
                            other.describe()
                        ),
                        "durations look like 30s, 5m, 24h, 7d",
                    ));
                }
            };
            let status = match named_value(stage, "status") {
                None => None,
                Some(ValueAst::Ident(s) | ValueAst::Str(s)) => Some(s.clone()),
                Some(other) => {
                    return Err(BqlError::bad_arg(
                        format!(
                            "'runs(status=...)' expects a status name, got {}",
                            other.describe()
                        ),
                        "example: runs(status=errored)",
                    ));
                }
            };
            Ok((PlanStage::Runs { last_ns, status }, SetKind::RunSet))
        }
        "run" => {
            require_source_position(stage, kind)?;
            check_args(stage, &["id"], 0, false)?;
            let id = match named_value(stage, "id") {
                Some(ValueAst::Str(s)) if !s.is_empty() => s.clone(),
                Some(other) => {
                    return Err(BqlError::bad_arg(
                        format!(
                            "'run(id=...)' expects a non-empty string, got {}",
                            other.describe()
                        ),
                        "example: run(id=\"1700000000-abc...-e1\")",
                    ));
                }
                None => {
                    return Err(BqlError::bad_arg(
                        "'run' requires an id argument",
                        "example: run(id=\"<run key>\"); list keys with runs()",
                    ));
                }
            };
            Ok((PlanStage::RunId { id }, SetKind::RunSet))
        }
        "ctx" => {
            match kind {
                None | Some(SetKind::RunSet) => {}
                other => {
                    return Err(BqlError::new(
                        "E_TYPE",
                        format!(
                            "'ctx' expects a RunSet input or the source position, got {}",
                            kind_name(other)
                        ),
                        "use ctx() first in the pipeline, or after runs()/run(id=...)",
                    ));
                }
            }
            if !stage.args.is_empty() {
                return Err(BqlError::bad_arg(
                    "'ctx' takes no arguments in v1",
                    "scope with a preceding runs()/run(id=...) or the request's run key",
                ));
            }
            Ok((PlanStage::Ctx, SetKind::CtxSet))
        }
        "calls" => {
            require_ctx_input(stage, kind)?;
            check_args(stage, &["fn"], 0, false)?;
            let pattern = match named_value(stage, "fn") {
                None => "*".to_string(),
                Some(ValueAst::Str(s)) => s.clone(),
                Some(other) => {
                    return Err(BqlError::bad_arg(
                        format!(
                            "'calls(fn=...)' expects a quoted glob, got {}",
                            other.describe()
                        ),
                        "example: calls(fn=\"extract_*\")",
                    ));
                }
            };
            Ok((PlanStage::Calls { pattern }, SetKind::CtxSet))
        }
        "errors" => {
            require_ctx_input(stage, kind)?;
            check_args(stage, &[], 0, false)?;
            Ok((PlanStage::Errors, SetKind::CtxSet))
        }
        "rollup" => {
            require_ctx_input(stage, kind)?;
            check_args(stage, &["by"], 0, false)?;
            match named_value(stage, "by") {
                None => {}
                Some(ValueAst::Ident(by)) if by == "fn" => {}
                Some(other) => {
                    return Err(BqlError::bad_arg(
                        format!(
                            "'rollup(by=...)' supports only by=fn in v1, got {}",
                            other.describe()
                        ),
                        "use rollup(by=fn)",
                    ));
                }
            }
            Ok((PlanStage::Rollup, SetKind::CtxSet))
        }
        "where" => {
            require_ctx_input(stage, kind)?;
            check_args(stage, &[], 0, true)?;
            let [ArgAst::Cmp { metric, op, value }] = stage.args.as_slice() else {
                return Err(BqlError::bad_arg(
                    "'where' expects exactly one comparison",
                    "example: where(calls > 1) or where(p95 >= 10ms)",
                ));
            };
            let metric = metric_from(metric).ok_or_else(|| {
                BqlError::bad_arg(
                    format!("unknown metric '{metric}' in where(...)"),
                    format!("metrics: {METRIC_NAMES}"),
                )
            })?;
            let value = match value {
                ValueAst::Int(v) if *v >= 0 => u64::try_from(*v).unwrap_or_default(),
                ValueAst::Dur(ns) => *ns,
                other => {
                    return Err(BqlError::bad_arg(
                        format!(
                            "'where' expects an integer or duration, got {}",
                            other.describe()
                        ),
                        "example: where(total_ns > 5s) or where(errors >= 1)",
                    ));
                }
            };
            Ok((
                PlanStage::Where {
                    metric,
                    op: *op,
                    value,
                },
                SetKind::CtxSet,
            ))
        }
        "sort" => {
            require_ctx_input(stage, kind)?;
            check_args(stage, &["by"], 1, false)?;
            let by = metric_arg(stage, "by", Metric::TotalNs)?;
            let desc = match positional(stage).first() {
                None => true,
                Some(ValueAst::Ident(d)) if d == "desc" => true,
                Some(ValueAst::Ident(d)) if d == "asc" => false,
                Some(other) => {
                    return Err(BqlError::bad_arg(
                        format!(
                            "'sort' direction must be desc or asc, got {}",
                            other.describe()
                        ),
                        "example: sort(by=calls, asc)",
                    ));
                }
            };
            Ok((PlanStage::Sort { by, desc }, SetKind::CtxSet))
        }
        "top" => {
            require_ctx_input(stage, kind)?;
            check_args(stage, &["by"], 1, false)?;
            let k = match positional(stage).first() {
                Some(v) => positive_int(stage, v, "row count")?,
                None => {
                    return Err(BqlError::bad_arg(
                        "'top' requires a row count",
                        "example: top(10, by=total_ns)",
                    ));
                }
            };
            let by = metric_arg(stage, "by", Metric::TotalNs)?;
            Ok((PlanStage::Top { k, by }, SetKind::Table))
        }
        "stats" => {
            if kind == Some(SetKind::ValueSet) {
                check_args(stage, &["by"], 0, false)?;
                match named_value(stage, "by") {
                    Some(ValueAst::Ident(by)) if by == "cid" => {}
                    Some(other) => {
                        return Err(BqlError::bad_arg(
                            format!(
                                "'stats(by=...)' over values supports only by=cid in v1, got {}",
                                other.describe()
                            ),
                            "use stats(by=cid) for the dedupe view",
                        ));
                    }
                    None => {
                        return Err(BqlError::bad_arg(
                            "'stats' over values needs a grouping in v1",
                            "use stats(by=cid) for the dedupe view",
                        ));
                    }
                }
                return Ok((PlanStage::StatsByCid, SetKind::Table));
            }
            require_ctx_input(stage, kind)?;
            check_args(stage, &[], 0, false)?;
            Ok((PlanStage::Stats, SetKind::Table))
        }
        "values" => {
            match kind {
                None | Some(SetKind::RunSet) => {}
                other => {
                    return Err(BqlError::new(
                        "E_TYPE",
                        format!(
                            "'values' expects a run scope (RunSet or source position), got {}",
                            kind_name(other)
                        ),
                        "aggregates cannot produce instances (§8.2): use values() after runs()/run(id=...) or as the source with the request's run",
                    ));
                }
            }
            check_args(stage, &["role", "fn"], 0, false)?;
            let mut roles = Vec::new();
            match named_value(stage, "role") {
                None => {}
                Some(ValueAst::Ident(r) | ValueAst::Str(r)) => roles.push(r.clone()),
                Some(ValueAst::List(items)) => {
                    for item in items {
                        match item {
                            ValueAst::Ident(r) | ValueAst::Str(r) => roles.push(r.clone()),
                            other => {
                                return Err(BqlError::bad_arg(
                                    format!(
                                        "'values(role=[...])' expects role names, got {}",
                                        other.describe()
                                    ),
                                    "roles: input, output, error, log",
                                ));
                            }
                        }
                    }
                }
                Some(other) => {
                    return Err(BqlError::bad_arg(
                        format!(
                            "'values(role=...)' expects a role or [role, ...], got {}",
                            other.describe()
                        ),
                        "example: values(role=[input, output])",
                    ));
                }
            }
            for role in &roles {
                if !matches!(role.as_str(), "input" | "output" | "error" | "log") {
                    return Err(BqlError::bad_arg(
                        format!("unknown role '{role}'"),
                        "roles: input, output, error, log",
                    ));
                }
            }
            let fn_pattern = match named_value(stage, "fn") {
                None => None,
                Some(ValueAst::Str(p)) => Some(p.clone()),
                Some(other) => {
                    return Err(BqlError::bad_arg(
                        format!(
                            "'values(fn=...)' expects a quoted glob, got {}",
                            other.describe()
                        ),
                        "example: values(fn=\"*line_total*\")",
                    ));
                }
            };
            Ok((PlanStage::Values { roles, fn_pattern }, SetKind::ValueSet))
        }
        "get" => {
            match kind {
                Some(SetKind::ValueSet) => {}
                other => {
                    return Err(BqlError::new(
                        "E_TYPE",
                        format!("'get' expects ValueSet input, got {}", kind_name(other)),
                        "pipe values(...) into get(...): run(id=...) | values(role=[input, output]) | get(max_bytes=64kb)",
                    ));
                }
            }
            check_args(stage, &["max_bytes", "depth", "as"], 0, false)?;
            if named_value(stage, "as").is_some() {
                return Err(BqlError::bad_arg(
                    "'get(as=Type)' typed hydration is not implemented in v1",
                    "omit as=; values hydrate as schema-erased JSON",
                ));
            }
            let max_bytes = match named_value(stage, "max_bytes") {
                None => 64 * 1024,
                Some(ValueAst::Size(b)) => usize::try_from(*b).unwrap_or(usize::MAX),
                Some(ValueAst::Int(v)) if *v > 0 => usize::try_from(*v).unwrap_or(usize::MAX),
                Some(other) => {
                    return Err(BqlError::bad_arg(
                        format!(
                            "'get(max_bytes=...)' expects a byte size, got {}",
                            other.describe()
                        ),
                        "sizes look like 64kb, 4mb, or a plain byte count",
                    ));
                }
            };
            let max_depth = match named_value(stage, "depth") {
                None => 32,
                Some(ValueAst::Int(v)) if *v >= 0 => u32::try_from(*v).unwrap_or(u32::MAX),
                Some(other) => {
                    return Err(BqlError::bad_arg(
                        format!(
                            "'get(depth=...)' expects a non-negative integer, got {}",
                            other.describe()
                        ),
                        "example: get(max_bytes=64kb, depth=3)",
                    ));
                }
            };
            Ok((
                PlanStage::Get {
                    max_bytes,
                    max_depth,
                },
                SetKind::Table,
            ))
        }
        "instances" => {
            match kind {
                None | Some(SetKind::RunSet) => {}
                other => {
                    return Err(BqlError::new(
                        "E_TYPE",
                        format!("'instances' expects a run scope, got {}", kind_name(other)),
                        "use instances() after runs()/run(id=...) or as the source",
                    ));
                }
            }
            check_args(stage, &["source"], 0, false)?;
            match named_value(stage, "source") {
                None => {}
                Some(ValueAst::Ident(source)) if source == "values" => {}
                Some(ValueAst::Ident(source)) => {
                    return Err(BqlError::new(
                        "E_NO_EXACT_SOURCE",
                        format!("exact source '{source}' is not readable in v1"),
                        "v1 derives instances from value join keys: instances(source=values); flight/trace sources are roadmap",
                    ));
                }
                Some(other) => {
                    return Err(BqlError::bad_arg(
                        format!(
                            "'instances(source=...)' expects an identifier, got {}",
                            other.describe()
                        ),
                        "example: instances(source=values)",
                    ));
                }
            }
            Ok((PlanStage::Instances, SetKind::Table))
        }
        "vdiff" => {
            require_source_position(stage, kind)?;
            check_args(stage, &["a", "b"], 0, false)?;
            let run_arg = |key: &str| -> Result<String, BqlError> {
                match named_value(stage, key) {
                    Some(ValueAst::Str(s)) if !s.is_empty() => Ok(s.clone()),
                    _ => Err(BqlError::bad_arg(
                        format!("'vdiff' requires {key}=\"<run key>\""),
                        "example: vdiff(a=\"<before run>\", b=\"<after run>\"); list keys with runs()",
                    )),
                }
            };
            Ok((
                PlanStage::VDiff {
                    a: run_arg("a")?,
                    b: run_arg("b")?,
                },
                SetKind::Table,
            ))
        }
        "limit" => {
            let Some(kind) = kind else {
                return Err(BqlError::new(
                    "E_TYPE",
                    "stage 'limit' expects RunSet, CtxSet, or Table input, got nothing (source position)",
                    "start the pipeline with a source stage: ctx(), runs(...), or run(id=...)",
                ));
            };
            check_args(stage, &[], 1, false)?;
            let k = match positional(stage).first() {
                Some(v) => positive_int(stage, v, "row count")?,
                None => {
                    return Err(BqlError::bad_arg(
                        "'limit' requires a row count",
                        "example: limit(100)",
                    ));
                }
            };
            Ok((PlanStage::Limit { k }, kind))
        }
        other => Err(BqlError::new(
            "E_UNKNOWN_STAGE",
            format!("unknown stage '{other}'"),
            format!("v1 stages: {STAGE_LIST}"),
        )),
    }
}

fn plan(stages: &[StageAst]) -> Result<Vec<PlanStage>, BqlError> {
    let mut kind: Option<SetKind> = None;
    let mut out = Vec::with_capacity(stages.len());
    for stage in stages {
        let (planned, next) = plan_stage(stage, kind)?;
        out.push(planned);
        kind = Some(next);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Executor
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct RunEntry {
    key: String,
    row: Option<runs::RunRow>,
}

#[derive(Debug, Clone)]
struct CtxRow {
    function: u32,
    name: String,
    depth: u32,
    calls: u64,
    errors: u64,
    total_ns: u64,
    self_ns: u64,
    await_ns: u64,
    hist: [u32; HIST_BUCKETS],
}

#[derive(Debug, Clone, Default)]
struct CtxState {
    rows: Vec<CtxRow>,
    sealed: bool,
    torn: bool,
    first_ts_ns: u64,
    last_ts_ns: u64,
    /// Declared loss/degradation markers from the fold (§8.4): rendered
    /// into the mandatory footer so a shed/degraded/saturated run can
    /// never present itself as complete.
    loss_notes: Vec<String>,
}

enum State {
    Start,
    Runs(Vec<RunEntry>),
    Ctx(CtxState),
    Values(ValueState),
    Table(BqlTable),
}

/// A resolved ValueSet: one run's captured values (§8.2).
struct ValueState {
    run_dir: std::path::PathBuf,
    rows: Vec<crate::values::ValueRow>,
    truncated: bool,
}

/// Execution context: degradation notes + the percentile-fallback flag,
/// merged into the footer at materialization (§8.4).
#[derive(Default)]
struct ExecNotes {
    notes: Vec<String>,
    mean_fallback_rows: usize,
}

impl ExecNotes {
    fn into_degraded(mut self) -> Vec<String> {
        if self.mean_fallback_rows > 0 {
            self.notes.push(format!(
                "percentiles degraded to mean for {} row(s): no histogram data",
                self.mean_fallback_rows
            ));
        }
        self.notes
    }
}

/// Bucket upper bound in ns (×4 stride from 1 µs; bucket 0 ⇒ < 1 µs).
fn bucket_upper_ns(bucket: usize) -> u64 {
    1_000u64.saturating_mul(4u64.saturating_pow(u32::try_from(bucket).unwrap_or(u32::MAX)))
}

/// Fold a percentile from a histogram (bucket-upper-bound estimate). Empty
/// histograms degrade to the mean with the fallback counter bumped.
fn percentile_ns(row: &CtxRow, pct: u64, exec: &mut ExecNotes) -> u64 {
    let n: u64 = row.hist.iter().map(|&c| u64::from(c)).sum();
    if n == 0 {
        exec.mean_fallback_rows += 1;
        return if row.calls == 0 {
            0
        } else {
            row.total_ns / row.calls
        };
    }
    let target = (n * pct).div_ceil(100).max(1);
    let mut cum = 0u64;
    for (bucket, &count) in row.hist.iter().enumerate() {
        cum += u64::from(count);
        if cum >= target {
            return bucket_upper_ns(bucket);
        }
    }
    bucket_upper_ns(HIST_BUCKETS - 1)
}

fn metric_value(row: &CtxRow, metric: Metric, exec: &mut ExecNotes) -> u64 {
    match metric {
        Metric::Calls => row.calls,
        Metric::TotalNs => row.total_ns,
        Metric::SelfNs => row.self_ns,
        Metric::Errors => row.errors,
        Metric::P50 => percentile_ns(row, 50, exec),
        Metric::P95 => percentile_ns(row, 95, exec),
        Metric::P99 => percentile_ns(row, 99, exec),
    }
}

/// `*`-only glob match.
fn glob_match(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    let Some((first, rest)) = parts.split_first() else {
        return pattern == text;
    };
    if !text.starts_with(first) {
        return false;
    }
    if rest.is_empty() {
        return text == *first;
    }
    let Some((last, middle)) = rest.split_last() else {
        return true;
    };
    let mut at = first.len();
    for part in middle {
        if part.is_empty() {
            continue;
        }
        match text[at..].find(part) {
            Some(i) => at += i + part.len(),
            None => return false,
        }
    }
    if last.is_empty() {
        return true;
    }
    text.len() >= at + last.len() && text[at..].ends_with(last)
}

fn open_ctx(engine: &mut ObserveEngine, key: &str) -> Result<CtxState, BqlError> {
    if engine.run_epoch(key).is_none() {
        engine.open_run(key).map_err(|err| {
            BqlError::bad_arg(
                format!("cannot open run '{key}': {err}"),
                "list valid run keys with runs()",
            )
        })?;
    }
    let names = engine.names(key).cloned().unwrap_or_default();
    let Some(fold) = engine.fold(key) else {
        return Err(BqlError::bad_arg(
            format!("run '{key}' is not open"),
            "list valid run keys with runs()",
        ));
    };
    let mut rows = Vec::new();
    for i in 1..fold.len() {
        let function = fold.function[i];
        if function == 0 {
            // Partition-root / unattributable bookkeeping nodes.
            continue;
        }
        rows.push(CtxRow {
            function,
            name: names
                .get(&function)
                .cloned()
                .unwrap_or_else(|| format!("fn#{function}")),
            depth: fold.depth[i],
            calls: fold.enters[i],
            errors: fold.ends_err[i],
            total_ns: fold.total_ns[i],
            self_ns: fold.self_ns[i],
            await_ns: fold.await_ns[i],
            hist: fold.hist[i],
        });
    }
    let loss_notes = fold
        .loss_markers
        .iter()
        .map(|(kind, detail)| {
            let label = match *kind {
                k if k == bex_events::prof::cct::blocks::marker_kind::LOSS => "loss",
                k if k == bex_events::prof::cct::blocks::marker_kind::DEGRADED => "degraded",
                k if k == bex_events::prof::cct::blocks::marker_kind::SHED => "shed",
                k if k == bex_events::prof::cct::blocks::marker_kind::BUDGET_EXHAUSTED => {
                    "budget_exhausted"
                }
                k if k == bex_events::prof::cct::blocks::marker_kind::SATURATED => "saturated",
                _ => "marker",
            };
            format!("{label}: {detail}")
        })
        .collect();
    Ok(CtxState {
        rows,
        sealed: fold.sealed,
        torn: fold.torn,
        first_ts_ns: fold.first_ts_ns,
        last_ts_ns: fold.last_ts_ns,
        loss_notes,
    })
}

/// Surface a fold's declared loss markers into the execution notes (and
/// therefore the mandatory §8.4 footer): a shed/degraded/saturated run
/// must never present itself as complete.
fn noted_ctx(ctx: CtxState, exec: &mut ExecNotes) -> CtxState {
    for note in &ctx.loss_notes {
        if !exec.notes.contains(note) {
            exec.notes.push(note.clone());
        }
    }
    ctx
}

/// Resolve a `RunSet` (or the source position) into one opened ctx —
/// the §8.2 `RunSet → CtxSet` coercion, failing closed on multi-run sets.
fn ctx_from(
    engine: &mut ObserveEngine,
    state: State,
    stage: &'static str,
    default_run: Option<&str>,
    implicit: bool,
    exec: &mut ExecNotes,
) -> Result<CtxState, BqlError> {
    match state {
        State::Ctx(ctx) => Ok(ctx),
        State::Start => {
            let Some(key) = default_run else {
                return Err(BqlError::bad_arg(
                    format!("'{stage}' requires a run in scope"),
                    "pass a run key with the request, or pipe run(id=\"...\") | ctx() first",
                ));
            };
            open_ctx(engine, key).map(|ctx| noted_ctx(ctx, exec))
        }
        State::Runs(entries) => match entries.as_slice() {
            [] => {
                exec.notes
                    .push("RunSet is empty: no runs matched, ctx has no data".to_string());
                Ok(CtxState {
                    sealed: true,
                    ..CtxState::default()
                })
            }
            [only] => {
                if implicit {
                    exec.notes.push(format!(
                        "implicit ctx(): RunSet coerced to run-scoped ctx of '{}'",
                        only.key
                    ));
                }
                open_ctx(engine, &only.key).map(|ctx| noted_ctx(ctx, exec))
            }
            many => Err(BqlError::new(
                "E_MULTI_RUN_CTX",
                format!(
                    "RunSet has {} runs; '{stage}' needs a single run's ctx in v1",
                    many.len()
                ),
                "pipe through run(id) or pass a run key",
            )),
        },
        State::Values(_) => Err(BqlError::new(
            "E_TYPE",
            format!("stage '{stage}' expects CtxSet input, got ValueSet"),
            "value pipelines end in get(...) / stats(by=cid) / limit(k)",
        )),
        State::Table(_) => Err(BqlError::new(
            "E_TYPE",
            format!("stage '{stage}' expects CtxSet input, got Table"),
            "apply ctx transforms before the sink stage",
        )),
    }
}

/// Resolve a run scope (RunSet or the source position) to one run's key +
/// boundary dir + bound session — the value plane's `ctx_from`.
fn run_scope(
    engine: &mut ObserveEngine,
    state: State,
    stage: &'static str,
    default_run: Option<&str>,
    exec: &mut ExecNotes,
) -> Result<Option<(String, std::path::PathBuf, Option<String>)>, BqlError> {
    let key = match state {
        State::Start => match default_run {
            Some(key) => key.to_string(),
            None => {
                return Err(BqlError::bad_arg(
                    format!("'{stage}' requires a run in scope"),
                    "pass a run key with the request, or pipe run(id=\"...\") first",
                ));
            }
        },
        State::Runs(entries) => match entries.as_slice() {
            [] => {
                exec.notes
                    .push("RunSet is empty: no runs matched, no values".to_string());
                return Ok(None);
            }
            [only] => only.key.clone(),
            many => {
                return Err(BqlError::new(
                    "E_MULTI_RUN_CTX",
                    format!(
                        "RunSet has {} runs; '{stage}' needs a single run in v1",
                        many.len()
                    ),
                    "pipe through run(id) or pass a run key",
                ));
            }
        },
        _ => {
            return Err(BqlError::new(
                "E_TYPE",
                format!("'{stage}' expects a run scope"),
                "use runs()/run(id=...) or the request's run key",
            ));
        }
    };
    // Resolve dir + bound session via the run index (the boundary's meta).
    let (now_ns, _) = now_epoch();
    let sessions = runs::list_sessions(engine.root(), now_ns);
    let rows = runs::list_runs(engine.root(), &sessions);
    let row = rows.iter().find(|r| run_key_of(r) == key);
    let dir = row.map_or_else(
        || engine.root().join("history").join(&key),
        |r| r.dir.clone(),
    );
    if !dir.is_dir() {
        return Err(BqlError::bad_arg(
            format!("run '{key}' has no boundary dir"),
            "list valid run keys with runs()",
        ));
    }
    let session = row.map(|r| r.session_dir.clone()).filter(|s| !s.is_empty());
    Ok(Some((key, dir, session)))
}

/// List one run's values with the exact-source fn join.
fn load_values(
    engine: &mut ObserveEngine,
    key: &str,
    dir: &std::path::Path,
    session: Option<&str>,
) -> Result<crate::values::RunValues, BqlError> {
    // Names come from the run's dictionary (already loaded on open).
    if engine.run_epoch(key).is_none() {
        let _ = engine.open_run(key);
    }
    let names = engine.names(key).cloned();
    crate::values::list_run_values(engine.root(), dir, session, names.as_ref()).map_err(|err| {
        BqlError::bad_arg(
            format!("cannot read values of run '{key}': {err}"),
            "the run's .bamlvalue segments are unreadable",
        )
    })
}

fn now_epoch() -> (u64, u64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let ns = u64::try_from(now.as_nanos()).unwrap_or(u64::MAX);
    let ms = u64::try_from(now.as_millis()).unwrap_or(u64::MAX);
    (ns, ms)
}

fn run_key_of(row: &runs::RunRow) -> String {
    row.dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn ctx_to_table(ctx: CtxState, exec: &mut ExecNotes) -> BqlTable {
    let mut rows = ctx.rows;
    if rows.len() > DEFAULT_ROW_LIMIT {
        exec.notes.push(format!(
            "result truncated to {DEFAULT_ROW_LIMIT} rows (implicit limit); add limit(k) or top(k, by=...) to choose"
        ));
        rows.truncate(DEFAULT_ROW_LIMIT);
    }
    let n = rows.len();
    let mut name = Vec::with_capacity(n);
    let mut function = Vec::with_capacity(n);
    let mut depth = Vec::with_capacity(n);
    let mut calls = Vec::with_capacity(n);
    let mut errors = Vec::with_capacity(n);
    let mut total = Vec::with_capacity(n);
    let mut self_ns = Vec::with_capacity(n);
    let mut await_ns = Vec::with_capacity(n);
    let mut p50 = Vec::with_capacity(n);
    let mut p95 = Vec::with_capacity(n);
    let mut p99 = Vec::with_capacity(n);
    for row in &rows {
        name.push(row.name.clone());
        function.push(row.function);
        depth.push(row.depth);
        calls.push(row.calls);
        errors.push(row.errors);
        total.push(row.total_ns);
        self_ns.push(row.self_ns);
        await_ns.push(row.await_ns);
        p50.push(percentile_ns(row, 50, exec));
        p95.push(percentile_ns(row, 95, exec));
        p99.push(percentile_ns(row, 99, exec));
    }
    BqlTable {
        columns: vec![
            ("fn".to_string(), ColData::Str(name)),
            ("function_id".to_string(), ColData::U32(function)),
            ("depth".to_string(), ColData::U32(depth)),
            ("calls".to_string(), ColData::U64(calls)),
            ("errors".to_string(), ColData::U64(errors)),
            ("total_ns".to_string(), ColData::U64(total)),
            ("self_ns".to_string(), ColData::U64(self_ns)),
            ("await_ns".to_string(), ColData::U64(await_ns)),
            ("p50_ns".to_string(), ColData::U64(p50)),
            ("p95_ns".to_string(), ColData::U64(p95)),
            ("p99_ns".to_string(), ColData::U64(p99)),
        ],
        footer: Completeness {
            sealed: ctx.sealed,
            torn: ctx.torn,
            first_ts_ns: ctx.first_ts_ns,
            last_ts_ns: ctx.last_ts_ns,
            degraded: Vec::new(),
        },
    }
}

fn runs_to_table(mut entries: Vec<RunEntry>, exec: &mut ExecNotes) -> BqlTable {
    if entries.len() > DEFAULT_ROW_LIMIT {
        exec.notes.push(format!(
            "result truncated to {DEFAULT_ROW_LIMIT} rows (implicit limit); add limit(k) to choose"
        ));
        entries.truncate(DEFAULT_ROW_LIMIT);
    }
    let n = entries.len();
    let mut run = Vec::with_capacity(n);
    let mut boundary = Vec::with_capacity(n);
    let mut target = Vec::with_capacity(n);
    let mut status = Vec::with_capacity(n);
    let mut revision = Vec::with_capacity(n);
    let mut created = Vec::with_capacity(n);
    let mut completed = Vec::with_capacity(n);
    for entry in entries {
        run.push(entry.key);
        let row = entry.row;
        boundary.push(
            row.as_ref()
                .map(|r| r.boundary_id.clone())
                .unwrap_or_default(),
        );
        target.push(row.as_ref().map(|r| r.target.clone()).unwrap_or_default());
        status.push(row.as_ref().map(|r| r.status.clone()).unwrap_or_default());
        revision.push(
            row.as_ref()
                .map(|r| r.revision_id.clone())
                .unwrap_or_default(),
        );
        created.push(row.as_ref().map_or(0, |r| r.created_ms));
        completed.push(row.as_ref().map_or(0, |r| r.completed_ms));
    }
    BqlTable {
        columns: vec![
            ("run".to_string(), ColData::Str(run)),
            ("boundary_id".to_string(), ColData::Str(boundary)),
            ("target".to_string(), ColData::Str(target)),
            ("status".to_string(), ColData::Str(status)),
            ("revision".to_string(), ColData::Str(revision)),
            ("created_ms".to_string(), ColData::U64(created)),
            ("completed_ms".to_string(), ColData::U64(completed)),
        ],
        footer: Completeness {
            sealed: true,
            torn: false,
            first_ts_ns: 0,
            last_ts_ns: 0,
            degraded: Vec::new(),
        },
    }
}

fn value_footer(values: &ValueState, exec: &mut ExecNotes) -> Completeness {
    let _ = exec;
    Completeness {
        sealed: !values.truncated,
        torn: values.truncated,
        first_ts_ns: 0,
        last_ts_ns: 0,
        degraded: Vec::new(),
    }
}

/// ValueSet listing WITHOUT hydration (pipeline ended before `get`).
fn values_to_table(values: ValueState, exec: &mut ExecNotes) -> BqlTable {
    if !values.rows.is_empty() {
        exec.notes.push(
            "values listed without bodies; pipe | get(max_bytes=64kb) to hydrate".to_string(),
        );
    }
    let footer = value_footer(&values, exec);
    let mut rows = values.rows;
    if rows.len() > DEFAULT_ROW_LIMIT {
        exec.notes.push(format!(
            "result truncated to {DEFAULT_ROW_LIMIT} rows (implicit limit); add limit(k) to choose"
        ));
        rows.truncate(DEFAULT_ROW_LIMIT);
    }
    let n = rows.len();
    let mut value_id = Vec::with_capacity(n);
    let mut role = Vec::with_capacity(n);
    let mut kind = Vec::with_capacity(n);
    let mut thread = Vec::with_capacity(n);
    let mut call = Vec::with_capacity(n);
    let mut fn_name = Vec::with_capacity(n);
    let mut bytes = Vec::with_capacity(n);
    let mut cid = Vec::with_capacity(n);
    let mut promoted = Vec::with_capacity(n);
    for row in &rows {
        value_id.push(row.value_id.clone());
        role.push(row.role.to_string());
        kind.push(row.kind.to_string());
        thread.push(row.thread_id);
        call.push(row.call_id);
        fn_name.push(row.fn_name.clone().unwrap_or_default());
        bytes.push(row.original_bytes);
        cid.push(
            row.cid
                .map(|c| bex_events::store::canon::cid_wire(&c))
                .unwrap_or_default(),
        );
        promoted.push(row.promoted_by.clone().unwrap_or_default());
    }
    BqlTable {
        columns: vec![
            ("value".to_string(), ColData::Str(value_id)),
            ("role".to_string(), ColData::Str(role)),
            ("kind".to_string(), ColData::Str(kind)),
            ("thread".to_string(), ColData::U64(thread)),
            ("call".to_string(), ColData::U64(call)),
            ("fn".to_string(), ColData::Str(fn_name)),
            ("bytes".to_string(), ColData::U64(bytes)),
            ("cid".to_string(), ColData::Str(cid)),
            ("promoted_by".to_string(), ColData::Str(promoted)),
        ],
        footer,
    }
}

/// `get(...)`: the hydration sink (§8.4 bounded, elision-honest).
fn hydrate_to_table(
    engine: &mut ObserveEngine,
    values: ValueState,
    max_bytes: usize,
    max_depth: u32,
    exec: &mut ExecNotes,
) -> BqlTable {
    let footer = value_footer(&values, exec);
    let mut rows = values.rows;
    if rows.len() > DEFAULT_ROW_LIMIT {
        exec.notes.push(format!(
            "result truncated to {DEFAULT_ROW_LIMIT} rows (implicit limit); add limit(k) to choose"
        ));
        rows.truncate(DEFAULT_ROW_LIMIT);
    }
    let store = bex_events::store::Store::open(&engine.root().join("store"), [0; 16]).ok();
    if store.is_none() && rows.iter().any(|r| r.cid.is_some()) {
        exec.notes.push(
            "value store unreadable: canonical bodies fall back to inline copies".to_string(),
        );
    }
    let mut elided_total = 0usize;
    let n = rows.len();
    let mut value_id = Vec::with_capacity(n);
    let mut role = Vec::with_capacity(n);
    let mut thread = Vec::with_capacity(n);
    let mut call = Vec::with_capacity(n);
    let mut fn_name = Vec::with_capacity(n);
    let mut cid = Vec::with_capacity(n);
    let mut body = Vec::with_capacity(n);
    for row in &rows {
        value_id.push(row.value_id.clone());
        role.push(row.role.to_string());
        thread.push(row.thread_id);
        call.push(row.call_id);
        fn_name.push(row.fn_name.clone().unwrap_or_default());
        cid.push(
            row.cid
                .map(|c| bex_events::store::canon::cid_wire(&c))
                .unwrap_or_default(),
        );
        let json = match row.hydrate(store.as_ref(), &values.run_dir, max_bytes, max_depth) {
            Ok(hydrated) => {
                elided_total += hydrated.elided.len();
                hydrated.json
            }
            Err(err) => serde_json::json!({ "$unavailable": err.to_string() }),
        };
        body.push(json.to_string());
    }
    if elided_total > 0 {
        exec.notes.push(format!(
            "{elided_total} subtree(s) elided by the {max_bytes}-byte budget; raise get(max_bytes=...) or descend by cid"
        ));
    }
    BqlTable {
        columns: vec![
            ("value".to_string(), ColData::Str(value_id)),
            ("role".to_string(), ColData::Str(role)),
            ("thread".to_string(), ColData::U64(thread)),
            ("call".to_string(), ColData::U64(call)),
            ("fn".to_string(), ColData::Str(fn_name)),
            ("cid".to_string(), ColData::Str(cid)),
            ("body".to_string(), ColData::Json(body)),
        ],
        footer,
    }
}

/// `instances(source=values)`: exact call instances from value join keys.
fn instances_to_table(listed: crate::values::RunValues, exec: &mut ExecNotes) -> BqlTable {
    use std::collections::BTreeMap;
    let mut by_call: BTreeMap<(u64, u64), (Option<String>, Vec<&'static str>, u64)> =
        BTreeMap::new();
    for row in &listed.rows {
        let entry = by_call.entry((row.thread_id, row.call_id)).or_insert((
            row.fn_name.clone(),
            Vec::new(),
            0,
        ));
        if entry.0.is_none() {
            entry.0.clone_from(&row.fn_name);
        }
        if !entry.1.contains(&row.role) {
            entry.1.push(row.role);
        }
        entry.2 += 1;
    }
    if listed.fn_join == crate::values::FnJoin::NoExactSource {
        exec.notes.push(
            "fn names unavailable: no exact source (re-run with BAML_PROFILE_RAW=1)".to_string(),
        );
    }
    let n = by_call.len();
    let mut thread = Vec::with_capacity(n);
    let mut call = Vec::with_capacity(n);
    let mut fn_name = Vec::with_capacity(n);
    let mut roles = Vec::with_capacity(n);
    let mut captures = Vec::with_capacity(n);
    for ((t, c), (f, r, count)) in by_call {
        thread.push(t);
        call.push(c);
        fn_name.push(f.unwrap_or_default());
        roles.push(r.join("+"));
        captures.push(count);
    }
    BqlTable {
        columns: vec![
            ("thread".to_string(), ColData::U64(thread)),
            ("call".to_string(), ColData::U64(call)),
            ("fn".to_string(), ColData::Str(fn_name)),
            ("roles".to_string(), ColData::Str(roles)),
            ("captures".to_string(), ColData::U64(captures)),
        ],
        footer: Completeness {
            sealed: !listed.truncated,
            torn: listed.truncated,
            ..Completeness::default()
        },
    }
}

/// `stats(by=cid)`: the §8.5 dedupe view — how often each distinct content
/// address appears.
fn stats_by_cid_table(values: ValueState, exec: &mut ExecNotes) -> BqlTable {
    use std::collections::BTreeMap;
    let footer = value_footer(&values, exec);
    let mut groups: BTreeMap<String, (u64, u64, Vec<&'static str>)> = BTreeMap::new();
    let mut no_cid = 0u64;
    for row in &values.rows {
        let Some(c) = row.cid else {
            no_cid += 1;
            continue;
        };
        let entry = groups
            .entry(bex_events::store::canon::cid_wire(&c))
            .or_insert((0, 0, Vec::new()));
        entry.0 += 1;
        entry.1 += row.original_bytes;
        if !entry.2.contains(&row.role) {
            entry.2.push(row.role);
        }
    }
    if no_cid > 0 {
        exec.notes.push(format!(
            "{no_cid} value(s) have no CID (no canonical body) and are excluded from the dedupe view"
        ));
    }
    let mut rows: Vec<(String, u64, u64, String)> = groups
        .into_iter()
        .map(|(cid, (n, bytes, roles))| (cid, n, bytes, roles.join("+")))
        .collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let n = rows.len();
    let mut cid = Vec::with_capacity(n);
    let mut count = Vec::with_capacity(n);
    let mut bytes = Vec::with_capacity(n);
    let mut roles = Vec::with_capacity(n);
    for (c, cnt, b, r) in rows {
        cid.push(c);
        count.push(cnt);
        bytes.push(b);
        roles.push(r);
    }
    BqlTable {
        columns: vec![
            ("cid".to_string(), ColData::Str(cid)),
            ("n".to_string(), ColData::U64(count)),
            ("bytes".to_string(), ColData::U64(bytes)),
            ("roles".to_string(), ColData::Str(roles)),
        ],
        footer,
    }
}

/// `vdiff(a=, b=)`: match calls across two runs by INPUT CID, compare
/// output CIDs (§8.5 verify-my-fix; CID equality is the Merkle
/// short-circuit — bodies are never fetched).
fn vdiff_table(
    a_key: &str,
    a: crate::values::RunValues,
    b_key: &str,
    b: crate::values::RunValues,
    exec: &mut ExecNotes,
) -> BqlTable {
    use std::collections::BTreeMap;
    // Per side: call -> (input cid, output cid).
    fn calls_of(
        listed: &crate::values::RunValues,
    ) -> BTreeMap<(u64, u64), (Option<[u8; 32]>, Option<[u8; 32]>, Option<String>)> {
        let mut out: BTreeMap<(u64, u64), (Option<[u8; 32]>, Option<[u8; 32]>, Option<String>)> =
            BTreeMap::new();
        for row in &listed.rows {
            let entry = out.entry((row.thread_id, row.call_id)).or_default();
            match row.role {
                "input" => entry.0 = row.cid,
                "output" | "error" => entry.1 = row.cid,
                _ => {}
            }
            if entry.2.is_none() {
                entry.2.clone_from(&row.fn_name);
            }
        }
        out
    }
    let a_calls = calls_of(&a);
    let b_calls = calls_of(&b);
    // Index side B by input cid (first match wins per input).
    let mut b_by_input: BTreeMap<[u8; 32], Vec<((u64, u64), Option<[u8; 32]>)>> = BTreeMap::new();
    for (call, (input, output, _)) in &b_calls {
        if let Some(input) = input {
            b_by_input.entry(*input).or_default().push((*call, *output));
        }
    }
    let mut matched = 0u64;
    let mut unmatched_a = 0u64;
    let mut fn_name = Vec::new();
    let mut input_cid = Vec::new();
    let mut out_a = Vec::new();
    let mut out_b = Vec::new();
    let mut equal = Vec::new();
    for (_call, (input, output_a, f)) in &a_calls {
        let Some(input) = input else { continue };
        let Some(candidates) = b_by_input.get_mut(input) else {
            unmatched_a += 1;
            continue;
        };
        let Some((_b_call, output_b)) = candidates.pop() else {
            unmatched_a += 1;
            continue;
        };
        matched += 1;
        fn_name.push(f.clone().unwrap_or_default());
        input_cid.push(bex_events::store::canon::cid_wire(input));
        out_a.push(
            output_a
                .map(|c| bex_events::store::canon::cid_wire(&c))
                .unwrap_or_default(),
        );
        out_b.push(
            output_b
                .map(|c| bex_events::store::canon::cid_wire(&c))
                .unwrap_or_default(),
        );
        equal.push(u32::from(*output_a == output_b));
    }
    let unmatched_b: u64 = b_by_input.values().map(|v| v.len() as u64).sum();
    exec.notes.push(format!(
        "vdiff {a_key} vs {b_key}: {matched} input-matched call(s); {unmatched_a} unmatched in a, {unmatched_b} in b"
    ));
    BqlTable {
        columns: vec![
            ("fn".to_string(), ColData::Str(fn_name)),
            ("input_cid".to_string(), ColData::Str(input_cid)),
            ("output_a".to_string(), ColData::Str(out_a)),
            ("output_b".to_string(), ColData::Str(out_b)),
            ("outputs_equal".to_string(), ColData::U32(equal)),
        ],
        footer: Completeness {
            sealed: !(a.truncated || b.truncated),
            torn: a.truncated || b.truncated,
            ..Completeness::default()
        },
    }
}

fn apply(
    engine: &mut ObserveEngine,
    default_run: Option<&str>,
    state: State,
    stage: &PlanStage,
    exec: &mut ExecNotes,
) -> Result<State, BqlError> {
    match stage {
        PlanStage::Runs { last_ns, status } => {
            let (now_ns, now_ms) = now_epoch();
            let sessions = runs::list_sessions(engine.root(), now_ns);
            let rows = runs::list_runs(engine.root(), &sessions);
            let cutoff_ms = last_ns.map(|ns| now_ms.saturating_sub(ns / 1_000_000));
            let entries = rows
                .into_iter()
                .filter(|r| cutoff_ms.is_none_or(|cut| r.created_ms >= cut))
                .filter(|r| status.as_ref().is_none_or(|s| &r.status == s))
                .map(|r| RunEntry {
                    key: run_key_of(&r),
                    row: Some(r),
                })
                .collect();
            Ok(State::Runs(entries))
        }
        PlanStage::RunId { id } => Ok(State::Runs(vec![RunEntry {
            key: id.clone(),
            row: None,
        }])),
        PlanStage::Ctx => {
            let ctx = ctx_from(engine, state, "ctx", default_run, false, exec)?;
            Ok(State::Ctx(ctx))
        }
        PlanStage::Calls { pattern } => {
            let mut ctx = ctx_from(engine, state, "calls", default_run, true, exec)?;
            ctx.rows.retain(|r| glob_match(pattern, &r.name));
            Ok(State::Ctx(ctx))
        }
        PlanStage::Errors => {
            let mut ctx = ctx_from(engine, state, "errors", default_run, true, exec)?;
            ctx.rows.retain(|r| r.errors > 0);
            Ok(State::Ctx(ctx))
        }
        PlanStage::Rollup => {
            let mut ctx = ctx_from(engine, state, "rollup", default_run, true, exec)?;
            let mut order: Vec<u32> = Vec::new();
            let mut by_fn: rustc_hash::FxHashMap<u32, CtxRow> = rustc_hash::FxHashMap::default();
            for row in ctx.rows.drain(..) {
                match by_fn.entry(row.function) {
                    std::collections::hash_map::Entry::Vacant(slot) => {
                        order.push(row.function);
                        slot.insert(row);
                    }
                    std::collections::hash_map::Entry::Occupied(mut slot) => {
                        let agg = slot.get_mut();
                        agg.calls += row.calls;
                        agg.errors += row.errors;
                        agg.total_ns += row.total_ns;
                        agg.self_ns += row.self_ns;
                        agg.await_ns += row.await_ns;
                        agg.depth = agg.depth.min(row.depth);
                        for (a, b) in agg.hist.iter_mut().zip(row.hist.iter()) {
                            *a = a.saturating_add(*b);
                        }
                    }
                }
            }
            ctx.rows = order.into_iter().filter_map(|f| by_fn.remove(&f)).collect();
            Ok(State::Ctx(ctx))
        }
        PlanStage::Where { metric, op, value } => {
            let mut ctx = ctx_from(engine, state, "where", default_run, true, exec)?;
            ctx.rows
                .retain(|r| op.eval(metric_value(r, *metric, exec), *value));
            Ok(State::Ctx(ctx))
        }
        PlanStage::Sort { by, desc } => {
            let mut ctx = ctx_from(engine, state, "sort", default_run, true, exec)?;
            let mut keyed: Vec<(u64, CtxRow)> = ctx
                .rows
                .drain(..)
                .map(|r| (metric_value(&r, *by, exec), r))
                .collect();
            keyed.sort_by_key(|(v, _)| *v);
            if *desc {
                keyed.reverse();
            }
            ctx.rows = keyed.into_iter().map(|(_, r)| r).collect();
            Ok(State::Ctx(ctx))
        }
        PlanStage::Top { k, by } => {
            let mut ctx = ctx_from(engine, state, "top", default_run, true, exec)?;
            let mut keyed: Vec<(u64, CtxRow)> = ctx
                .rows
                .drain(..)
                .map(|r| (metric_value(&r, *by, exec), r))
                .collect();
            keyed.sort_by_key(|(v, _)| std::cmp::Reverse(*v));
            keyed.truncate(*k);
            ctx.rows = keyed.into_iter().map(|(_, r)| r).collect();
            Ok(State::Table(ctx_to_table(ctx, exec)))
        }
        PlanStage::Stats => {
            let ctx = ctx_from(engine, state, "stats", default_run, true, exec)?;
            let mut all = CtxRow {
                function: 0,
                name: String::new(),
                depth: 0,
                calls: 0,
                errors: 0,
                total_ns: 0,
                self_ns: 0,
                await_ns: 0,
                hist: [0; HIST_BUCKETS],
            };
            for row in &ctx.rows {
                all.calls += row.calls;
                all.errors += row.errors;
                all.total_ns += row.total_ns;
                all.self_ns += row.self_ns;
                all.await_ns += row.await_ns;
                for (a, b) in all.hist.iter_mut().zip(row.hist.iter()) {
                    *a = a.saturating_add(*b);
                }
            }
            let table = BqlTable {
                columns: vec![
                    ("calls".to_string(), ColData::U64(vec![all.calls])),
                    ("errors".to_string(), ColData::U64(vec![all.errors])),
                    ("total_ns".to_string(), ColData::U64(vec![all.total_ns])),
                    (
                        "p50_ns".to_string(),
                        ColData::U64(vec![percentile_ns(&all, 50, exec)]),
                    ),
                    (
                        "p95_ns".to_string(),
                        ColData::U64(vec![percentile_ns(&all, 95, exec)]),
                    ),
                    (
                        "p99_ns".to_string(),
                        ColData::U64(vec![percentile_ns(&all, 99, exec)]),
                    ),
                ],
                footer: Completeness {
                    sealed: ctx.sealed,
                    torn: ctx.torn,
                    first_ts_ns: ctx.first_ts_ns,
                    last_ts_ns: ctx.last_ts_ns,
                    degraded: Vec::new(),
                },
            };
            Ok(State::Table(table))
        }
        PlanStage::Values { roles, fn_pattern } => {
            let Some((run_key, run_dir, session)) =
                run_scope(engine, state, "values", default_run, exec)?
            else {
                return Ok(State::Values(ValueState {
                    run_dir: std::path::PathBuf::new(),
                    rows: Vec::new(),
                    truncated: false,
                }));
            };
            let listed = load_values(engine, &run_key, &run_dir, session.as_deref())?;
            if fn_pattern.is_some() && listed.fn_join == crate::values::FnJoin::NoExactSource {
                return Err(BqlError::new(
                    "E_NO_EXACT_SOURCE",
                    "filtering values by function name needs an exact source, and none covers this run",
                    "re-run with BAML_PROFILE_RAW=1 (raw firehose), arm the flight recorder, or drop the fn= filter",
                ));
            }
            if listed.fn_join == crate::values::FnJoin::NoExactSource && !listed.rows.is_empty() {
                exec.notes.push(
                    "fn names unavailable: no exact source for this run (re-run with BAML_PROFILE_RAW=1 to join call->function)"
                        .to_string(),
                );
            }
            let mut rows = listed.rows;
            if !roles.is_empty() {
                rows.retain(|r| roles.iter().any(|want| want == r.role));
            }
            if let Some(pattern) = fn_pattern {
                rows.retain(|r| r.fn_name.as_deref().is_some_and(|f| glob_match(pattern, f)));
            }
            if rows.is_empty() {
                exec.notes.push(
                    "no captured values matched: capture defaults are llm_boundary - opt calls in with $id = boundary.id().capture(...)"
                        .to_string(),
                );
            }
            let _ = run_key;
            Ok(State::Values(ValueState {
                run_dir,
                rows,
                truncated: listed.truncated,
            }))
        }
        PlanStage::Get {
            max_bytes,
            max_depth,
        } => {
            let State::Values(values) = state else {
                return Err(BqlError::new(
                    "E_TYPE",
                    "'get' expects ValueSet input",
                    "pipe values(...) into get(...)",
                ));
            };
            Ok(State::Table(hydrate_to_table(
                engine, values, *max_bytes, *max_depth, exec,
            )))
        }
        PlanStage::Instances => {
            let Some((run_key, run_dir, session)) =
                run_scope(engine, state, "instances", default_run, exec)?
            else {
                return Ok(State::Table(BqlTable {
                    columns: vec![("thread".to_string(), ColData::U64(Vec::new()))],
                    footer: Completeness {
                        sealed: true,
                        ..Completeness::default()
                    },
                }));
            };
            let listed = load_values(engine, &run_key, &run_dir, session.as_deref())?;
            if listed.rows.is_empty() {
                return Err(BqlError::new(
                    "E_NO_EXACT_SOURCE",
                    format!("no exact source covers run '{run_key}': no captured values"),
                    "opt calls in with $id = boundary.id().capture(...), arm the flight recorder, or request a bounded full trace",
                ));
            }
            Ok(State::Table(instances_to_table(listed, exec)))
        }
        PlanStage::StatsByCid => {
            let State::Values(values) = state else {
                return Err(BqlError::new(
                    "E_TYPE",
                    "'stats(by=cid)' expects ValueSet input",
                    "pipe values(...) into stats(by=cid)",
                ));
            };
            Ok(State::Table(stats_by_cid_table(values, exec)))
        }
        PlanStage::VDiff { a, b } => {
            let side = |engine: &mut ObserveEngine, key: &str, exec: &mut ExecNotes| {
                let scoped = run_scope(
                    engine,
                    State::Runs(vec![RunEntry {
                        key: key.to_string(),
                        row: None,
                    }]),
                    "vdiff",
                    None,
                    exec,
                )?;
                let Some((run_key, run_dir, session)) = scoped else {
                    return Err(BqlError::bad_arg(
                        format!("run '{key}' not found"),
                        "list valid run keys with runs()",
                    ));
                };
                load_values(engine, &run_key, &run_dir, session.as_deref())
            };
            let side_a = side(engine, a, exec)?;
            let side_b = side(engine, b, exec)?;
            Ok(State::Table(vdiff_table(a, side_a, b, side_b, exec)))
        }
        PlanStage::Limit { k } => match state {
            State::Runs(mut entries) => {
                entries.truncate(*k);
                Ok(State::Runs(entries))
            }
            State::Ctx(mut ctx) => {
                ctx.rows.truncate(*k);
                Ok(State::Ctx(ctx))
            }
            State::Values(mut values) => {
                values.rows.truncate(*k);
                Ok(State::Values(values))
            }
            State::Table(mut table) => {
                for (_, col) in &mut table.columns {
                    col.truncate(*k);
                }
                Ok(State::Table(table))
            }
            State::Start => Err(BqlError::new(
                "E_TYPE",
                "stage 'limit' expects RunSet, CtxSet, or Table input, got nothing",
                "start the pipeline with a source stage",
            )),
        },
    }
}

/// Parse, plan, and execute one BQL query against an engine. `default_run`
/// is the request-scoped run key `ctx()` uses when no `RunSet` precedes it.
pub fn run(
    engine: &mut ObserveEngine,
    default_run: Option<&str>,
    query: &str,
) -> Result<BqlTable, BqlError> {
    let stages = parse(query)?;
    let planned = plan(&stages)?;
    let mut exec = ExecNotes::default();
    let mut state = State::Start;
    for stage in &planned {
        state = apply(engine, default_run, state, stage, &mut exec)?;
    }
    // §8.2: X → Table coercion at pipeline end; §8.4: footer always ships.
    let mut table = match state {
        State::Start => {
            return Err(BqlError::parse("empty query"));
        }
        State::Runs(entries) => runs_to_table(entries, &mut exec),
        State::Ctx(ctx) => ctx_to_table(ctx, &mut exec),
        State::Values(values) => values_to_table(values, &mut exec),
        State::Table(table) => table,
    };
    table.footer.degraded = exec.into_degraded();
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_star_only_semantics() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("extract_*", "extract_resume"));
        assert!(!glob_match("extract_*", "parse_resume"));
        assert!(glob_match("*leaf*", "app.leaf"));
        assert!(glob_match("*leaf", "app.leaf"));
        assert!(!glob_match("*leaf", "app.leaf2"));
        assert!(glob_match("a*b*c", "a-x-b-y-c"));
        assert!(!glob_match("a*b*c", "a-x-c"));
        assert!(glob_match("exact", "exact"));
        assert!(!glob_match("exact", "exact2"));
    }

    #[test]
    fn lexer_durations_and_ops() {
        let toks = lex("runs(last=24h) | where(p95 >= 10ms)").unwrap();
        assert!(
            toks.iter()
                .any(|(t, _)| *t == Tok::Dur(24 * 3_600 * 1_000_000_000))
        );
        assert!(toks.iter().any(|(t, _)| *t == Tok::Dur(10_000_000)));
        assert!(toks.iter().any(|(t, _)| *t == Tok::Cmp(CmpOp::Ge)));
        assert!(lex("where(x > 5parsecs)").is_err());
    }

    #[test]
    fn bucket_upper_bounds_follow_x4_stride() {
        assert_eq!(bucket_upper_ns(0), 1_000);
        assert_eq!(bucket_upper_ns(1), 4_000);
        assert_eq!(bucket_upper_ns(2), 16_000);
        assert_eq!(bucket_upper_ns(15), 1_073_741_824_000);
    }

    #[test]
    fn planner_rejects_sink_at_source_position() {
        let stages = parse("top(5) | ctx()").unwrap();
        let err = plan(&stages).unwrap_err();
        assert_eq!(err.code, "E_TYPE");
    }

    #[test]
    fn planner_rejects_unknown_stage() {
        let stages = parse("frobnicate()").unwrap();
        let err = plan(&stages).unwrap_err();
        assert_eq!(err.code, "E_UNKNOWN_STAGE");
    }

    #[test]
    fn empty_table_frame_carries_footer() {
        let table = BqlTable {
            columns: vec![("run".to_string(), ColData::Str(Vec::new()))],
            footer: Completeness {
                sealed: true,
                ..Completeness::default()
            },
        };
        let frame = table.to_frame(7);
        let view = bqf1::decode_frame(&frame).unwrap();
        assert_eq!(view.kind, FrameKind::BqlTable as u16);
        assert_eq!(view.nrows, 1, "meta row only");
        let meta = view.col_str(view.cols.len() - 1).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&meta[0]).unwrap();
        assert_eq!(parsed["rows"], 0);
        assert_eq!(parsed["footer"]["sealed"], true);
    }
}
