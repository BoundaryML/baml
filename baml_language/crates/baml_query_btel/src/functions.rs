//! SQLite functions registered on every query connection.
//!
//! Identity and timing functions are pure. Value functions resolve CAS
//! blobs lazily through the current query's context: a query that never
//! evaluates `args`, `output` or `error` reads no blob. Within one query,
//! hydration outcomes (including missing and corrupt blobs) are memoized
//! under entry and byte limits; the next query starts empty and can find a
//! blob that arrived in between.
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Instant,
};

use btel_reader::{
    cas::{CasOutcome, CasStore},
    evidence::ArgumentNames,
    timing::{Clock, Conversion, EpochStatus, TimingState, UtcAnchor, format_unix_ns},
    value::{self, CmpOp, Kind, Nav, Operand, RenderLimits, Root, Scalar, Segment, equality},
};
use btel_snapshot::SnapshotId;
use rusqlite::{
    Connection, Error as SqlError,
    functions::{Aggregate, Context, FunctionFlags},
    types::{Null, Value as SqlValue, ValueRef},
};
use serde::Serialize;

/// Function names are reserved: user SQL containing this prefix is rejected
/// before translation, so only the translator and catalog views call them.
pub const PREFIX: &str = "__btel_";

#[derive(Clone, Copy, Debug)]
pub struct ValueLimits {
    /// Decoded snapshots kept for the rest of the query.
    pub max_cached_blobs: usize,
    /// Approximate encoded bytes of cached snapshots.
    pub max_cached_bytes: u64,
    /// Navigation results kept for render/kind pairs and repeated paths.
    pub max_cached_results: usize,
    pub render: RenderLimits,
    pub comparison: equality::Limits,
}
impl Default for ValueLimits {
    fn default() -> Self {
        Self {
            max_cached_blobs: 4096,
            max_cached_bytes: 256 << 20,
            max_cached_results: 16_384,
            render: RenderLimits::default(),
            comparison: equality::Limits::default(),
        }
    }
}

/// Work done by value functions during one query.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ValueMetrics {
    pub value_evaluations: u64,
    pub cas_loads: u64,
    pub cas_bytes_read: u64,
    pub cas_cache_hits: u64,
    pub result_cache_hits: u64,
    /// Distinct unavailable values by diagnostic code.
    pub unavailable: BTreeMap<String, u64>,
}

struct CacheState {
    blobs: HashMap<[u8; 16], CasOutcome>,
    blob_bytes: u64,
    results: HashMap<Vec<u8>, (Scalar, Kind)>,
    unavailable_seen: HashSet<Vec<u8>>,
    metrics: ValueMetrics,
}

/// One query's value-resolution state.
pub struct QueryContext {
    cas: CasStore,
    limits: ValueLimits,
    deadline: Option<Instant>,
    state: Mutex<CacheState>,
}

impl QueryContext {
    pub fn new(cas: CasStore, limits: ValueLimits, deadline: Option<Instant>) -> Self {
        Self {
            cas,
            limits,
            deadline,
            state: Mutex::new(CacheState {
                blobs: HashMap::new(),
                blob_bytes: 0,
                results: HashMap::new(),
                unavailable_seen: HashSet::new(),
                metrics: ValueMetrics::default(),
            }),
        }
    }

    pub fn metrics(&self) -> ValueMetrics {
        self.state.lock().expect("value state").metrics.clone()
    }

    fn check_deadline(&self) -> rusqlite::Result<()> {
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(user_error("query time budget exceeded"));
        }
        Ok(())
    }

    fn load(&self, state: &mut CacheState, id: [u8; 16]) -> CasOutcome {
        if let Some(outcome) = state.blobs.get(&id) {
            state.metrics.cas_cache_hits += 1;
            return outcome.clone();
        }
        let load = self.cas.load(SnapshotId::from_bytes(id));
        state.metrics.cas_loads += 1;
        state.metrics.cas_bytes_read += load.bytes_read;
        if state.blobs.len() < self.limits.max_cached_blobs
            && state.blob_bytes + load.bytes_read <= self.limits.max_cached_bytes
        {
            state.blob_bytes += load.bytes_read;
            state.blobs.insert(id, load.outcome.clone());
        }
        load.outcome
    }

    /// Report `code` once per distinct `key` (a handle, or a handle plus an
    /// operation marker).
    fn note(&self, key: &[u8], code: &str) {
        let mut state = self.state.lock().expect("value state");
        Self::unavailable(&mut state, key, code);
    }

    fn unavailable(state: &mut CacheState, handle: &[u8], code: &str) {
        if state.unavailable_seen.insert(handle.to_vec()) {
            *state
                .metrics
                .unavailable
                .entry(code.to_owned())
                .or_default() += 1;
        }
    }

    /// Navigate a handle and present the result as a SQL scalar.
    fn resolve(&self, handle: &[u8]) -> rusqlite::Result<(Scalar, Kind)> {
        self.check_deadline()?;
        let mut state = self.state.lock().expect("value state");
        state.metrics.value_evaluations += 1;
        if let Some(result) = state.results.get(handle) {
            let result = result.clone();
            state.metrics.result_cache_hits += 1;
            return Ok(result);
        }
        let parsed =
            Handle::decode(handle).ok_or_else(|| user_error("invalid BAML value handle"))?;
        if parsed.pending {
            Self::unavailable(&mut state, handle, PENDING);
            return Ok((Scalar::Null, Kind::Unavailable));
        }
        let result = match self.load(&mut state, parsed.cas) {
            CasOutcome::Available(snapshot) => {
                let nav = value::navigate(
                    &snapshot,
                    parsed.root(),
                    parsed.names.as_ref(),
                    &parsed.path,
                );
                if let Nav::Unavailable(reason) = nav {
                    Self::unavailable(&mut state, handle, reason.code());
                }
                value::to_scalar(&snapshot, nav, parsed.names.as_ref(), &self.limits.render)
            }
            outcome => {
                Self::unavailable(&mut state, handle, outcome.code());
                (Scalar::Null, Kind::Unavailable)
            }
        };
        if state.results.len() < self.limits.max_cached_results {
            state.results.insert(handle.to_vec(), result.clone());
        }
        Ok(result)
    }

    /// Navigate and hand the result to `f` (comparisons need the leaf).
    fn with_nav<T>(
        &self,
        handle: &[u8],
        f: impl FnOnce(Nav<'_>) -> T,
    ) -> rusqlite::Result<Option<T>> {
        self.with_value(handle, |value| f(value.nav))
    }

    fn with_value<T>(
        &self,
        handle: &[u8],
        f: impl FnOnce(equality::Captured<'_>) -> T,
    ) -> rusqlite::Result<Option<T>> {
        self.check_deadline()?;
        let mut state = self.state.lock().expect("value state");
        state.metrics.value_evaluations += 1;
        let parsed =
            Handle::decode(handle).ok_or_else(|| user_error("invalid BAML value handle"))?;
        if parsed.pending {
            Self::unavailable(&mut state, handle, PENDING);
            return Ok(None);
        }
        match self.load(&mut state, parsed.cas) {
            CasOutcome::Available(snapshot) => {
                let nav = value::navigate(
                    &snapshot,
                    parsed.root(),
                    parsed.names.as_ref(),
                    &parsed.path,
                );
                if let Nav::Unavailable(reason) = nav {
                    Self::unavailable(&mut state, handle, reason.code());
                    return Ok(None);
                }
                drop(state);
                Ok(Some(f(equality::Captured {
                    snapshot: &snapshot,
                    nav,
                    names: parsed.names.as_ref(),
                })))
            }
            outcome => {
                Self::unavailable(&mut state, handle, outcome.code());
                Ok(None)
            }
        }
    }

    /// `baml_value_state`: what the path finds, without reporting it as
    /// unavailable evidence (the caller asked for exactly this answer).
    fn state(&self, handle: &[u8]) -> rusqlite::Result<String> {
        self.check_deadline()?;
        let mut state = self.state.lock().expect("value state");
        state.metrics.value_evaluations += 1;
        let parsed =
            Handle::decode(handle).ok_or_else(|| user_error("invalid BAML value handle"))?;
        if parsed.pending {
            return Ok(PENDING.to_owned());
        }
        Ok(match self.load(&mut state, parsed.cas) {
            CasOutcome::Available(snapshot) => value::state(
                &snapshot,
                parsed.root(),
                parsed.names.as_ref(),
                &parsed.path,
            )
            .to_owned(),
            outcome => outcome.code().to_owned(),
        })
    }
}

/// Unavailable code of a capture whose announcement is not indexed yet.
const PENDING: &str = "capture_pending";
/// Structured ordering and opaque-value equality are unsupported.
const COMPARISON_UNSUPPORTED: &str = "comparison_unsupported";

/// Compare two leaves; an unanswerable comparison involving a structured
/// value is reported rather than left as an unexplained NULL.
fn compare_reported(
    context: &QueryContext,
    handle: &[u8],
    left: value::Leaf<'_>,
    op: CmpOp,
    right: value::Leaf<'_>,
) -> Option<bool> {
    let result = value::compare(left, op, right);
    if result.is_none()
        && (matches!(left, value::Leaf::Structured) || matches!(right, value::Leaf::Structured))
    {
        let mut key = handle.to_vec();
        key.extend_from_slice(b"\xffcmp");
        context.note(&key, COMPARISON_UNSUPPORTED);
    }
    result
}

fn comparison_result(
    context: &QueryContext,
    left: &[u8],
    result: Option<Result<Option<bool>, equality::Error>>,
) -> Option<bool> {
    match result? {
        Ok(value) => value,
        Err(error) => {
            // One diagnostic per left value/reason, not per join pair or
            // JSON literal. Tracking every pair would grow quadratically.
            let mut key = left.to_vec();
            key.extend_from_slice(b"\xffwholecmp");
            key.extend_from_slice(error.code().as_bytes());
            context.note(&key, error.code());
            None
        }
    }
}

/// Where the functions find the current query's context.
pub type ContextSlot = Arc<Mutex<Option<Arc<QueryContext>>>>;

fn current(slot: &ContextSlot) -> rusqlite::Result<Arc<QueryContext>> {
    slot.lock()
        .expect("context slot")
        .clone()
        .ok_or_else(|| user_error("BAML value functions are only available inside a query"))
}

fn user_error(message: &str) -> SqlError {
    SqlError::UserFunctionError(message.into())
}

/// A lazily resolved BAML value: which capture, which root, which path.
/// Encoded as a BLOB so it flows through views, joins and subqueries.
#[derive(Clone, Debug, PartialEq)]
pub struct Handle {
    /// 1 inputs, 2 output, 3 error.
    pub kind: u8,
    /// The capture is expected but its evidence is not indexed yet (inputs
    /// of a completion whose announcement has not arrived); `cas` is unused.
    pub pending: bool,
    pub cas: [u8; 16],
    /// Argument slot names; `None` when not recorded.
    pub names: Option<ArgumentNames>,
    pub path: Vec<Segment>,
}

const HANDLE_MAGIC: [u8; 2] = [0xB7, 0x01];
const PENDING_BIT: u8 = 0x80;

impl Handle {
    fn root(&self) -> Root {
        if self.kind == 1 {
            Root::Arguments
        } else {
            Root::Value
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32);
        out.extend_from_slice(&HANDLE_MAGIC);
        out.push(self.kind | if self.pending { PENDING_BIT } else { 0 });
        out.extend_from_slice(&self.cas);
        match &self.names {
            Some(names) => {
                let names = names.encode();
                out.push(1);
                out.extend_from_slice(&component_len(names.len()).to_le_bytes());
                out.extend_from_slice(&names);
            }
            None => out.push(0),
        }
        for segment in &self.path {
            push_segment(&mut out, segment);
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let rest = bytes.strip_prefix(&HANDLE_MAGIC)?;
        let (&kind, rest) = rest.split_first()?;
        let (kind, pending) = (kind & !PENDING_BIT, kind & PENDING_BIT != 0);
        let (cas, rest) = rest.split_at_checked(16)?;
        let (&has_names, mut rest) = rest.split_first()?;
        let names = if has_names == 1 {
            let (len, tail) = rest.split_at_checked(4)?;
            let len = u32::from_le_bytes(len.try_into().ok()?) as usize;
            let (names, tail) = tail.split_at_checked(len)?;
            rest = tail;
            Some(ArgumentNames::decode(names)?)
        } else {
            None
        };
        let mut path = Vec::new();
        while let Some((&tag, tail)) = rest.split_first() {
            match tag {
                0 => {
                    let (len, tail) = tail.split_at_checked(4)?;
                    let len = u32::from_le_bytes(len.try_into().ok()?) as usize;
                    let (key, tail) = tail.split_at_checked(len)?;
                    path.push(Segment::Key(String::from_utf8(key.to_vec()).ok()?));
                    rest = tail;
                }
                1 => {
                    let (index, tail) = tail.split_at_checked(8)?;
                    path.push(Segment::Index(i64::from_le_bytes(index.try_into().ok()?)));
                    rest = tail;
                }
                _ => return None,
            }
        }
        Some(Self {
            kind,
            pending,
            cas: cas.try_into().ok()?,
            names,
            path,
        })
    }
}

/// Handle components come from recorded names and SQL literals, far below
/// 4 GiB.
fn component_len(len: usize) -> u32 {
    u32::try_from(len).expect("handle component fits u32")
}

fn push_segment(out: &mut Vec<u8>, segment: &Segment) {
    match segment {
        Segment::Key(key) => {
            out.push(0);
            out.extend_from_slice(&component_len(key.len()).to_le_bytes());
            out.extend_from_slice(key.as_bytes());
        }
        Segment::Index(index) => {
            out.push(1);
            out.extend_from_slice(&index.to_le_bytes());
        }
    }
}

fn scalar_to_sql(scalar: Scalar) -> SqlValue {
    match scalar {
        Scalar::Null => SqlValue::Null,
        Scalar::Integer(n) => SqlValue::Integer(n),
        Scalar::Real(f) => SqlValue::Real(f),
        Scalar::Text(s) => SqlValue::Text(s),
    }
}

fn opt_i64(ctx: &Context<'_>, index: usize) -> rusqlite::Result<Option<i64>> {
    match ctx.get_raw(index) {
        ValueRef::Null => Ok(None),
        ValueRef::Integer(n) => Ok(Some(n)),
        _ => Err(user_error("expected an integer")),
    }
}

fn opt_blob<'a>(ctx: &'a Context<'_>, index: usize) -> rusqlite::Result<Option<&'a [u8]>> {
    match ctx.get_raw(index) {
        ValueRef::Null => Ok(None),
        ValueRef::Blob(bytes) => Ok(Some(bytes)),
        _ => Err(user_error("expected a blob")),
    }
}

/// Stored quantities are non-negative i64 by construction.
fn ticks_u64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn u64_from_blob(bytes: &[u8]) -> Option<u64> {
    Some(u64::from_be_bytes(bytes.try_into().ok()?))
}

fn hex(bytes: &[u8]) -> String {
    btel_reader::discovery::hex(bytes)
}

/// Clock facts from view columns: conversion (NULL multiplier means the
/// definition exists but is unrepresentable) and the observed status code.
fn clock(multiplier: Option<i64>, shift: Option<i64>, status: Option<i64>) -> Clock {
    Clock {
        conversion: multiplier.zip(shift).and_then(|(multiplier, shift)| {
            Some(Conversion {
                multiplier: u64::try_from(multiplier).ok()?,
                shift: u32::try_from(shift).ok()?,
            })
        }),
        status: status.and_then(EpochStatus::from_code),
    }
}

/// `(start, end, multiplier, shift, status, epoch)` where `epoch` is 0 for
/// no usable definition, 1 for one definition, 2 for conflicting ones.
fn interval(ctx: &Context<'_>) -> rusqlite::Result<Result<u64, TimingState>> {
    let start = opt_i64(ctx, 0)?.and_then(|v| u64::try_from(v).ok());
    let end = opt_i64(ctx, 1)?.and_then(|v| u64::try_from(v).ok());
    let clock = match clock_at(ctx, 2)? {
        Ok(clock) => clock,
        Err(state) if start.is_some() && end.is_some() => return Ok(Err(state)),
        Err(_) => return Ok(Err(TimingState::Incomplete)),
    };
    Ok(clock.interval_ns(start, end))
}

/// Checked i64 sum: NULL if any input is NULL (an overflowed stored total)
/// or if the sum overflows. Never a float, never a silent wrap.
struct CheckedSum;
impl Aggregate<Option<i64>, Option<i64>> for CheckedSum {
    fn init(&self, _: &mut Context<'_>) -> rusqlite::Result<Option<i64>> {
        Ok(Some(0))
    }
    fn step(&self, ctx: &mut Context<'_>, acc: &mut Option<i64>) -> rusqlite::Result<()> {
        let value = opt_i64(ctx, 0)?;
        *acc = acc.zip(value).and_then(|(a, b)| a.checked_add(b));
        Ok(())
    }
    fn finalize(
        &self,
        _: &mut Context<'_>,
        acc: Option<Option<i64>>,
    ) -> rusqlite::Result<Option<i64>> {
        Ok(acc.unwrap_or(Some(0)))
    }
}

/// Register every internal function on `conn`.
pub fn register(conn: &Connection, slot: &ContextSlot) -> rusqlite::Result<()> {
    crate::outcomes::register(conn)?;
    let pure = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    // Value functions read files, so they are not deterministic across queries.
    let io = FunctionFlags::SQLITE_UTF8;

    conn.create_scalar_function("__btel_u64", 1, pure, |ctx| {
        Ok(opt_blob(ctx, 0)?
            .and_then(u64_from_blob)
            .map(|v| v.to_string()))
    })?;
    conn.create_scalar_function("__btel_pubid", 2, pure, |ctx| {
        let (Some(recording), Some(id)) = (opt_blob(ctx, 0)?, opt_blob(ctx, 1)?) else {
            return Ok(None);
        };
        Ok(u64_from_blob(id).map(|id| format!("{}:{id}", hex(recording))))
    })?;
    conn.create_scalar_function("__btel_duration", 6, pure, |ctx| {
        Ok(interval(ctx)?.ok().and_then(|ns| i64::try_from(ns).ok()))
    })?;
    conn.create_scalar_function("__btel_timing", 6, pure, |ctx| {
        Ok(match interval(ctx)? {
            Ok(ns) if i64::try_from(ns).is_ok() => TimingState::Valid.label(),
            Ok(_) => TimingState::Overflow.label(),
            Err(state) => state.label(),
        })
    })?;
    // `(ticks, multiplier, shift, status, epoch)` for accumulated totals.
    conn.create_scalar_function("__btel_total_ns", 5, pure, |ctx| {
        let ticks = opt_i64(ctx, 0)?.and_then(|v| u64::try_from(v).ok());
        let Ok(clock) = clock_at(ctx, 1)? else {
            return Ok(None);
        };
        Ok(clock
            .total_ns(ticks)
            .ok()
            .and_then(|ns| i64::try_from(ns).ok()))
    })?;
    conn.create_scalar_function("__btel_total_timing", 5, pure, |ctx| {
        let ticks = opt_i64(ctx, 0)?.and_then(|v| u64::try_from(v).ok());
        let clock = match clock_at(ctx, 1)? {
            Ok(clock) => clock,
            Err(state) => return Ok(state.label()),
        };
        Ok(match clock.total_ns(ticks) {
            Ok(ns) if i64::try_from(ns).is_ok() => TimingState::Valid.label(),
            Ok(_) => TimingState::Overflow.label(),
            Err(state) => state.label(),
        })
    })?;
    conn.create_scalar_function("__btel_utc", 5, pure, |ctx| {
        let (Some(ticks), Some(anchor_ticks), Some(anchor_ns), Some(multiplier), Some(shift)) = (
            opt_i64(ctx, 0)?,
            opt_i64(ctx, 1)?,
            opt_i64(ctx, 2)?,
            opt_i64(ctx, 3)?,
            opt_i64(ctx, 4)?,
        ) else {
            return Ok(None);
        };
        let conversion = Conversion {
            multiplier: ticks_u64(multiplier),
            shift: u32::try_from(shift).unwrap_or(u32::MAX),
        };
        let anchor = UtcAnchor {
            ticks: ticks_u64(anchor_ticks),
            unix_ns: i128::from(anchor_ns),
        };
        Ok(anchor
            .unix_ns_at(conversion, ticks_u64(ticks))
            .and_then(format_unix_ns))
    })?;
    conn.create_scalar_function("__btel_unix_ms", 5, pure, |ctx| {
        let (Some(ticks), Some(anchor_ticks), Some(anchor_ns), Some(multiplier), Some(shift)) = (
            opt_i64(ctx, 0)?,
            opt_i64(ctx, 1)?,
            opt_i64(ctx, 2)?,
            opt_i64(ctx, 3)?,
            opt_i64(ctx, 4)?,
        ) else {
            return Ok(None);
        };
        let anchor = UtcAnchor {
            ticks: ticks_u64(anchor_ticks),
            unix_ns: i128::from(anchor_ns),
        };
        let conversion = Conversion {
            multiplier: ticks_u64(multiplier),
            shift: u32::try_from(shift).unwrap_or(u32::MAX),
        };
        Ok(anchor
            .unix_ns_at(conversion, ticks_u64(ticks))
            .and_then(|ns| i64::try_from(ns.div_euclid(1_000_000)).ok()))
    })?;
    conn.create_aggregate_function("__btel_sum", 1, pure, CheckedSum)?;

    // Handle construction reads nothing.
    // `__btel_ref(kind, cas, names, pending)`: no CAS id is NULL (nothing
    // captured) unless `pending` says the capture's evidence is still due.
    conn.create_scalar_function("__btel_ref", 4, pure, |ctx| {
        let kind = opt_i64(ctx, 0)?.unwrap_or(0);
        let pending = opt_i64(ctx, 3)?.unwrap_or(0) != 0;
        let cas = match opt_blob(ctx, 1)? {
            Some(cas) => cas,
            None if pending => &[0; 16],
            None => return Ok(None),
        };
        let names = match opt_blob(ctx, 2)? {
            Some(bytes) => Some(
                ArgumentNames::decode(bytes).ok_or_else(|| user_error("invalid argument names"))?,
            ),
            None => None,
        };
        let handle = Handle {
            kind: u8::try_from(kind)
                .ok()
                .filter(|k| k & PENDING_BIT == 0)
                .ok_or_else(|| user_error("invalid value kind"))?,
            pending: pending && opt_blob(ctx, 1)?.is_none(),
            cas: cas.try_into().map_err(|_| user_error("invalid CAS id"))?,
            names,
            path: Vec::new(),
        };
        Ok(Some(handle.encode()))
    })?;
    conn.create_scalar_function("__btel_nav", -1, pure, |ctx| {
        let Some(handle) = opt_blob(ctx, 0)? else {
            return Ok(None);
        };
        Handle::decode(handle).ok_or_else(|| user_error("invalid BAML value handle"))?;
        let mut out = handle.to_vec();
        for index in 1..ctx.len() {
            let segment = match ctx.get_raw(index) {
                ValueRef::Text(key) => Segment::Key(
                    std::str::from_utf8(key)
                        .map_err(|_| user_error("path key is not UTF-8"))?
                        .to_owned(),
                ),
                ValueRef::Integer(i) => Segment::Index(i),
                _ => return Err(user_error("path segments are constant strings or integers")),
            };
            push_segment(&mut out, &segment);
        }
        Ok(Some(out))
    })?;
    let s = slot.clone();
    conn.create_scalar_function("__btel_render", 1, io, move |ctx| {
        let Some(handle) = opt_blob(ctx, 0)? else {
            return Ok(SqlValue::Null);
        };
        Ok(scalar_to_sql(current(&s)?.resolve(handle)?.0))
    })?;
    let s = slot.clone();
    conn.create_scalar_function("__btel_kind", 1, io, move |ctx| {
        let Some(handle) = opt_blob(ctx, 0)? else {
            return Ok(None);
        };
        Ok(Some(current(&s)?.resolve(handle)?.1.label()))
    })?;
    let s = slot.clone();
    conn.create_scalar_function("__btel_is_null", 1, io, move |ctx| {
        let Some(handle) = opt_blob(ctx, 0)? else {
            // No capture at all: null-like.
            return Ok(Some(true));
        };
        Ok(current(&s)?.with_nav(handle, value::is_null)?.flatten())
    })?;
    let s = slot.clone();
    conn.create_scalar_function("__btel_truthy", 1, io, move |ctx| {
        let Some(handle) = opt_blob(ctx, 0)? else {
            return Ok(None);
        };
        Ok(current(&s)?
            .with_nav(handle, |nav| {
                value::leaf_of(nav)
                    .and_then(|leaf| value::compare(leaf, CmpOp::Eq, value::Leaf::Bool(true)))
            })?
            .flatten())
    })?;
    let s = slot.clone();
    conn.create_scalar_function("__btel_cmp", 4, io, move |ctx| {
        let Some(handle) = opt_blob(ctx, 0)? else {
            return Ok(None);
        };
        let op = ctx.get::<String>(1)?;
        let op = CmpOp::parse(&op).ok_or_else(|| user_error("unsupported comparison"))?;
        let boolean = ctx.get::<String>(3)? == "bool";
        let text;
        let operand = match ctx.get_raw(2) {
            ValueRef::Null => Operand::Null,
            ValueRef::Integer(n) if boolean => Operand::Bool(n != 0),
            ValueRef::Integer(n) => Operand::Integer(n),
            ValueRef::Real(f) => Operand::Real(f),
            ValueRef::Text(t) => {
                text = String::from_utf8_lossy(t).into_owned();
                Operand::Text(&text)
            }
            ValueRef::Blob(_) => return Err(user_error("cannot compare a BAML value to a blob")),
        };
        let Some(right) = value::leaf_of_operand(operand) else {
            return Ok(None);
        };
        let context = current(&s)?;
        Ok(context
            .with_nav(handle, |nav| {
                value::leaf_of(nav)
                    .and_then(|left| compare_reported(&context, handle, left, op, right))
            })?
            .flatten())
    })?;
    let s = slot.clone();
    conn.create_scalar_function("__btel_cmp_values", 3, io, move |ctx| {
        let (Some(left), Some(right)) = (opt_blob(ctx, 0)?, opt_blob(ctx, 2)?) else {
            return Ok(None);
        };
        let op = ctx.get::<String>(1)?;
        let op = CmpOp::parse(&op).ok_or_else(|| user_error("unsupported comparison"))?;
        let context = current(&s)?;
        let result = context.with_value(left, |a| {
            context.with_value(right, |b| {
                equality::captured(a, op, b, &context.limits.comparison)
            })
        })?;
        let result = match result {
            Some(result) => result?,
            None => None,
        };
        context.check_deadline()?;
        Ok(comparison_result(&context, left, result))
    })?;
    let s = slot.clone();
    conn.create_scalar_function("__btel_cmp_json", 3, io, move |ctx| {
        let Some(handle) = opt_blob(ctx, 0)? else {
            return Ok(None);
        };
        let op = CmpOp::parse(&ctx.get::<String>(1)?)
            .ok_or_else(|| user_error("unsupported comparison"))?;
        // SQL lowering accepts only bounded constant JSON literals. SQLite
        // keeps their parse alongside the constant argument for this statement.
        let json: Arc<serde_json::Value> = ctx.get_or_create_aux(2, |raw| {
            let text = raw.as_str()?;
            serde_json::from_str(text).map_err(|error| user_error(&error.to_string()))
        })?;
        let context = current(&s)?;
        let result = context.with_value(handle, |left| {
            equality::json(left, op, &json, &context.limits.comparison)
        })?;
        context.check_deadline()?;
        Ok(comparison_result(&context, handle, result))
    })?;
    let s = slot.clone();
    conn.create_scalar_function("__btel_value_state", 1, io, move |ctx| {
        let Some(handle) = opt_blob(ctx, 0)? else {
            return Ok("no_value".to_owned());
        };
        current(&s)?.state(handle)
    })?;
    register_identity_and_time(conn)?;
    let _ = Null;
    Ok(())
}

/// `<recording hex>:<prefix><id>` for an INTEGER id or an 8-byte BLOB id.
fn scoped_id(ctx: &Context<'_>) -> rusqlite::Result<Option<String>> {
    let Some(recording) = opt_blob(ctx, 0)? else {
        return Ok(None);
    };
    let prefix = match ctx.get_raw(1) {
        ValueRef::Text(t) => std::str::from_utf8(t).map_err(|_| user_error("id prefix"))?,
        _ => return Err(user_error("id prefix must be text")),
    };
    let id = match ctx.get_raw(2) {
        ValueRef::Null => return Ok(None),
        ValueRef::Integer(n) => u64::try_from(n).map_err(|_| user_error("negative id"))?,
        ValueRef::Blob(b) => u64_from_blob(b).ok_or_else(|| user_error("id blob"))?,
        _ => return Err(user_error("id must be an integer or blob")),
    };
    Ok(Some(format!("{}:{prefix}{id}", hex(recording))))
}

/// One reduced aggregate node's tick totals; `None` once a total overflowed.
#[derive(Clone, Copy)]
struct NodeTicks {
    duration: Option<i64>,
    wait: Option<i64>,
}

/// Self time of a call path from its outermost (`normal`) and recursive
/// (`reentry`) nodes, when present. Returns the nanoseconds and the state,
/// or only the state when the value is unsupported.
fn path_self(
    defined: bool,
    normal: Option<NodeTicks>,
    reentry: Option<NodeTicks>,
    child_ticks: Option<i64>,
    thread_done: bool,
    clock: Result<Clock, TimingState>,
) -> (Option<u64>, &'static str) {
    if !defined {
        return (None, "unresolved");
    }
    let clock = match clock {
        Ok(clock) => clock,
        Err(state) => return (None, state.label()),
    };
    if let Err(state) = clock.total_ns(Some(0)) {
        return (None, state.label());
    }
    let Some(NodeTicks {
        duration,
        wait: normal_await,
    }) = normal
    else {
        // No completed outermost invocation in the indexed prefix.
        return (None, "incomplete");
    };
    let reentry_await = reentry.map_or(Some(0), |node| node.wait);
    let (Some(duration), Some(normal_await), Some(reentry_await), Some(children)) =
        (duration, normal_await, reentry_await, child_ticks)
    else {
        return (None, "overflow");
    };
    let spent = [children, normal_await, reentry_await]
        .into_iter()
        .try_fold(0_i64, i64::checked_add);
    let Some(ticks) = spent.and_then(|spent| duration.checked_sub(spent)) else {
        return (None, "overflow");
    };
    let Ok(ticks) = u64::try_from(ticks) else {
        // Children and waits exceed the parent: on an unfinished thread the
        // parent's own completion is simply not indexed yet.
        return (
            None,
            if thread_done {
                "underflow"
            } else {
                "incomplete"
            },
        );
    };
    match clock.total_ns(Some(ticks)) {
        Ok(ns) if i64::try_from(ns).is_ok() => {
            (Some(ns), if thread_done { "valid" } else { "provisional" })
        }
        Ok(_) => (None, "overflow"),
        Err(state) => (None, state.label()),
    }
}

/// A named field of an encoded clock epoch definition, for `clocks`.
fn epoch_field(definition: &[u8], field: &str) -> rusqlite::Result<SqlValue> {
    use btel_recorder::proto;
    use prost::Message as _;
    let Ok(epoch) = proto::ClockEpochDefinition::decode(definition) else {
        return Ok(SqlValue::Null);
    };
    let text = |v: u64| SqlValue::Text(v.to_string());
    let label = |v: Option<&'static str>| v.map_or(SqlValue::Null, |l| SqlValue::Text(l.into()));
    let calibration = epoch.calibration.as_ref();
    let utc = epoch.utc.as_ref();
    Ok(match field {
        "source" => label(
            proto::ClockSource::try_from(epoch.source)
                .ok()
                .and_then(|s| {
                    Some(match s {
                        proto::ClockSource::OsMonotonic => "os_monotonic",
                        proto::ClockSource::WindowsQpc => "windows_qpc",
                        proto::ClockSource::X86Tsc => "x86_tsc",
                        proto::ClockSource::ArmSystemCounter => "arm_system_counter",
                        proto::ClockSource::Mock => "mock",
                        proto::ClockSource::Unspecified => return None,
                    })
                }),
        ),
        "reference_tick" => text(epoch.reference_tick),
        "reference_monotonic_ns" => text(epoch.reference_monotonic_ns),
        "multiplier" => text(epoch.multiplier),
        "shift" => SqlValue::Integer(i64::from(epoch.shift)),
        "origin_uncertainty_ns" => text(epoch.origin_uncertainty_ns),
        "rate_error_ppb" => text(epoch.rate_error_ppb),
        "calibration_status" => label(calibration.and_then(|c| {
            Some(match proto::CalibrationStatus::try_from(c.status).ok()? {
                proto::CalibrationStatus::NotRequired => "not_required",
                proto::CalibrationStatus::Converged => "converged",
                proto::CalibrationStatus::DeadlineReached => "deadline_reached",
                proto::CalibrationStatus::Unspecified => return None,
            })
        })),
        "calibration_samples" => calibration.map_or(SqlValue::Null, |c| text(c.samples)),
        "calibration_elapsed_ns" => calibration.map_or(SqlValue::Null, |c| text(c.elapsed_ns)),
        "calibration_mean_residual_ns" => {
            calibration.map_or(SqlValue::Null, |c| SqlValue::Real(c.mean_residual_ns))
        }
        "calibration_mean_error_ns" => {
            calibration.map_or(SqlValue::Null, |c| SqlValue::Real(c.mean_error_ns))
        }
        "fallback_reason" => label(
            proto::FallbackReason::try_from(epoch.fallback)
                .ok()
                .map(|f| match f {
                    proto::FallbackReason::None => "none",
                    proto::FallbackReason::Requested => "requested",
                    proto::FallbackReason::Unsupported => "unsupported",
                    proto::FallbackReason::Calibration => "calibration",
                    proto::FallbackReason::ScaleValidation => "scale_validation",
                    proto::FallbackReason::Discontinuity => "discontinuity",
                }),
        ),
        "utc_anchor_ticks" => utc.map_or(SqlValue::Null, |u| text(u.ticks)),
        "utc_anchor_unix_ns" => utc
            .and_then(UtcAnchor::from_wire)
            .map_or(SqlValue::Null, |u| SqlValue::Text(u.unix_ns.to_string())),
        "utc_anchor_at" => utc
            .and_then(UtcAnchor::from_wire)
            .and_then(|u| format_unix_ns(u.unix_ns))
            .map_or(SqlValue::Null, SqlValue::Text),
        "utc_uncertainty_ns" => utc.map_or(SqlValue::Null, |u| text(u.uncertainty_ns)),
        _ => return Err(user_error("unknown clock field")),
    })
}

/// Clock for timing columns: `(multiplier, shift, status, epoch)` at `at`,
/// `epoch` as in [`interval`]. A defined epoch with an unrepresentable
/// multiplier is overflow.
fn clock_at(ctx: &Context<'_>, at: usize) -> rusqlite::Result<Result<Clock, TimingState>> {
    let multiplier = opt_i64(ctx, at)?;
    let epoch = opt_i64(ctx, at + 3)?.unwrap_or(0);
    if epoch == 2 {
        return Ok(Err(TimingState::Conflicted));
    }
    if epoch == 1 && multiplier.is_none() {
        return Ok(Err(TimingState::Overflow));
    }
    Ok(Ok(clock(
        multiplier,
        opt_i64(ctx, at + 1)?,
        opt_i64(ctx, at + 2)?,
    )))
}

fn register_identity_and_time(conn: &Connection) -> rusqlite::Result<()> {
    let pure = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    conn.create_scalar_function("__btel_sid", 3, pure, scoped_id)?;
    // Checked i64 addition: NULL on NULL input or overflow, never a float.
    conn.create_scalar_function("__btel_add", 2, pure, |ctx| {
        Ok(opt_i64(ctx, 0)?
            .zip(opt_i64(ctx, 1)?)
            .and_then(|(a, b)| a.checked_add(b)))
    })?;
    conn.create_scalar_function("__btel_unix_ns", 5, pure, |ctx| {
        let (Some(ticks), Some(anchor_ticks), Some(anchor_ns), Some(multiplier), Some(shift)) = (
            opt_i64(ctx, 0)?,
            opt_i64(ctx, 1)?,
            opt_i64(ctx, 2)?,
            opt_i64(ctx, 3)?,
            opt_i64(ctx, 4)?,
        ) else {
            return Ok(None);
        };
        let anchor = UtcAnchor {
            ticks: ticks_u64(anchor_ticks),
            unix_ns: i128::from(anchor_ns),
        };
        let conversion = Conversion {
            multiplier: ticks_u64(multiplier),
            shift: u32::try_from(shift).unwrap_or(u32::MAX),
        };
        Ok(anchor
            .unix_ns_at(conversion, ticks_u64(ticks))
            .and_then(|ns| i64::try_from(ns).ok()))
    })?;
    // `__btel_self(field, defined, n_present, n_duration, n_await,
    //  r_present, r_await, child_ticks, thread_done, multiplier, shift,
    //  status, has_epoch)`; field 0 is nanoseconds, 1 the state.
    conn.create_scalar_function("__btel_self", 13, pure, |ctx| {
        let field = opt_i64(ctx, 0)?.unwrap_or(0);
        let flag = |i| -> rusqlite::Result<bool> { Ok(opt_i64(ctx, i)?.unwrap_or(0) != 0) };
        let normal = if flag(2)? {
            Some(NodeTicks {
                duration: opt_i64(ctx, 3)?,
                wait: opt_i64(ctx, 4)?,
            })
        } else {
            None
        };
        // Only the recursive node's await time enters self time.
        let reentry = if flag(5)? {
            Some(NodeTicks {
                duration: None,
                wait: opt_i64(ctx, 6)?,
            })
        } else {
            None
        };
        let (ns, state) = path_self(
            flag(1)?,
            normal,
            reentry,
            opt_i64(ctx, 7)?,
            flag(8)?,
            clock_at(ctx, 9)?,
        );
        Ok(if field == 0 {
            ns.and_then(|ns| i64::try_from(ns).ok())
                .map(SqlValue::Integer)
        } else {
            Some(SqlValue::Text(state.to_owned()))
        }
        .unwrap_or(SqlValue::Null))
    })?;
    conn.create_scalar_function("__btel_epoch_field", 2, pure, |ctx| {
        let Some(definition) = opt_blob(ctx, 0)? else {
            return Ok(SqlValue::Null);
        };
        let field = ctx.get::<String>(1)?;
        epoch_field(definition, &field)
    })?;
    Ok(())
}
