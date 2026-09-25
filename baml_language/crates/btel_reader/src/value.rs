//! Captured-value navigation, comparison and bounded rendering over decoded
//! CAS graphs.
//!
//! Navigation follows constant string keys (map entries, instance fields,
//! named arguments) and zero-based list indices through the graph without
//! expanding it. Three outcomes stay distinct:
//! - a captured BAML `null` is data;
//! - an absent path, an omitted argument or an incompatible step is
//!   `Missing`, a SQL-NULL-like non-match that keeps an answer complete;
//! - truncated evidence or unknown argument names are `Unavailable`: the
//!   recording cannot answer, and callers report that separately.
use std::{cmp::Ordering, collections::HashMap};

use btel_snapshot::{DecodedObject, DecodedSnapshot, DecodedValue, Description, Limit};
use num_bigint::BigInt;
use serde_json::{Map, Value as Json};

use crate::evidence::ArgumentNames;

pub mod equality;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Segment {
    Key(String),
    Index(i64),
}

/// Which root of a snapshot a column refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Root {
    /// Captured inputs: a `FunctionArgs` root navigated by parameter name.
    Arguments,
    /// Captured output or error: a value root.
    Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unavailable {
    /// The producer's capture limits cut this part of the value.
    Truncated(Limit),
    /// The recording has no names for this function's argument slots.
    ArgumentNamesUnknown,
    /// Recorded names do not match the captured slot count.
    ArgumentLayoutMismatch,
    /// The snapshot root does not match the column (argument vs value).
    WrongRoot,
    /// A cycle of cells cannot resolve to a logical value.
    CellCycle,
}

impl Unavailable {
    pub fn code(self) -> &'static str {
        match self {
            Self::Truncated(_) => "value_truncated",
            Self::ArgumentNamesUnknown => "argument_names_unknown",
            Self::ArgumentLayoutMismatch => "argument_layout_mismatch",
            Self::WrongRoot => "capture_root_mismatch",
            Self::CellCycle => "value_cycle",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Nav<'a> {
    /// The whole argument root (a bare `args`).
    Arguments,
    Value(&'a DecodedValue),
    Missing,
    Unavailable(Unavailable),
}

/// Cells are an execution detail: navigation and rendering see through them.
/// Cycles of cells are unavailable rather than looping or becoming absent data.
fn through_cells<'a>(
    snapshot: &'a DecodedSnapshot,
    mut value: &'a DecodedValue,
) -> Option<&'a DecodedValue> {
    for _ in 0..=snapshot.objects.len() {
        match value {
            DecodedValue::Object(id) => match snapshot.object(*id) {
                DecodedObject::Cell(inner) => value = inner,
                _ => return Some(value),
            },
            _ => return Some(value),
        }
    }
    None
}

fn argument_slot<'a>(
    slots: &'a [DecodedValue],
    names: Option<&ArgumentNames>,
    segment: &Segment,
) -> Result<&'a DecodedValue, Nav<'a>> {
    let index = match segment {
        Segment::Key(name) => {
            let names = names.ok_or(Nav::Unavailable(Unavailable::ArgumentNamesUnknown))?;
            if names.slots.len() != slots.len() {
                return Err(Nav::Unavailable(Unavailable::ArgumentLayoutMismatch));
            }
            names.position(name).ok_or(Nav::Missing)?
        }
        Segment::Index(index) => usize::try_from(*index).map_err(|_| Nav::Missing)?,
    };
    match slots.get(index) {
        // Not passed by the caller: absent, not null.
        Some(DecodedValue::OmittedArg) | None => Err(Nav::Missing),
        Some(value) => Ok(value),
    }
}

/// Follow `path` from a snapshot root.
pub fn navigate<'a>(
    snapshot: &'a DecodedSnapshot,
    root: Root,
    names: Option<&ArgumentNames>,
    path: &[Segment],
) -> Nav<'a> {
    let (mut value, rest) = match (root, &snapshot.root) {
        (Root::Arguments, btel_snapshot::DecodedRoot::FunctionArgs { slots, .. }) => {
            let Some((first, rest)) = path.split_first() else {
                return Nav::Arguments;
            };
            match argument_slot(slots, names, first) {
                Ok(value) => (value, rest),
                Err(nav) => return nav,
            }
        }
        (Root::Value, btel_snapshot::DecodedRoot::Value(value)) => (value, path),
        _ => return Nav::Unavailable(Unavailable::WrongRoot),
    };
    for segment in rest {
        let Some(current) = through_cells(snapshot, value) else {
            return Nav::Unavailable(Unavailable::CellCycle);
        };
        value = match step(snapshot, current, segment) {
            Ok(next) => next,
            Err(nav) => return nav,
        };
    }
    match through_cells(snapshot, value) {
        Some(DecodedValue::Truncated(limit)) => Nav::Unavailable(Unavailable::Truncated(*limit)),
        Some(DecodedValue::OmittedArg) => Nav::Missing,
        None => Nav::Unavailable(Unavailable::CellCycle),
        Some(DecodedValue::Object(id)) => match snapshot.object(*id) {
            DecodedObject::Truncated(limit) => Nav::Unavailable(Unavailable::Truncated(*limit)),
            _ => Nav::Value(value_ref(snapshot, value)),
        },
        Some(value) => Nav::Value(value),
    }
}

fn value_ref<'a>(snapshot: &'a DecodedSnapshot, value: &'a DecodedValue) -> &'a DecodedValue {
    through_cells(snapshot, value).unwrap_or(value)
}

fn step<'a>(
    snapshot: &'a DecodedSnapshot,
    value: &'a DecodedValue,
    segment: &Segment,
) -> Result<&'a DecodedValue, Nav<'a>> {
    let id = match value {
        DecodedValue::Truncated(limit) => {
            return Err(Nav::Unavailable(Unavailable::Truncated(*limit)));
        }
        DecodedValue::Object(id) => *id,
        // Scalars, null and enum values have no children.
        _ => return Err(Nav::Missing),
    };
    let keyed = |entries: &'a [(Box<str>, DecodedValue)], original_len: u64, key: &str| {
        match entries.iter().find(|(k, _)| &**k == key) {
            Some((_, value)) => Ok(value),
            // The capture kept fewer entries than existed: the key may be
            // among the dropped ones.
            None if (entries.len() as u64) < original_len => {
                Err(Nav::Unavailable(Unavailable::Truncated(Limit::Values)))
            }
            None => Err(Nav::Missing),
        }
    };
    match (snapshot.object(id), segment) {
        (
            DecodedObject::List {
                items,
                original_len,
                ..
            },
            Segment::Index(index),
        ) => {
            let Ok(index) = usize::try_from(*index) else {
                return Err(Nav::Missing);
            };
            match items.get(index) {
                Some(value) => Ok(value),
                None if (index as u64) < *original_len => {
                    Err(Nav::Unavailable(Unavailable::Truncated(Limit::Values)))
                }
                None => Err(Nav::Missing),
            }
        }
        (
            DecodedObject::Map {
                entries,
                original_len,
                ..
            }
            | DecodedObject::Instance {
                fields: entries,
                original_len,
                ..
            },
            Segment::Key(key),
        ) => keyed(entries, *original_len, key),
        (DecodedObject::Truncated(limit), _) => {
            Err(Nav::Unavailable(Unavailable::Truncated(*limit)))
        }
        _ => Err(Nav::Missing),
    }
}

/// What a path finds, as `baml_value_state` reports it: `present`, `null`
/// (a captured null), `missing` (absent key or index), `omitted` (an
/// argument the caller did not pass) or an unavailable-evidence code such as
/// `value_truncated`. Distinguishes the cases navigation folds together.
pub fn state(
    snapshot: &DecodedSnapshot,
    root: Root,
    names: Option<&ArgumentNames>,
    path: &[Segment],
) -> &'static str {
    if let (Root::Arguments, btel_snapshot::DecodedRoot::FunctionArgs { slots, .. }, [first]) =
        (root, &snapshot.root, path)
        && let Ok(DecodedValue::OmittedArg) = argument_slot_raw(slots, names, first)
    {
        return "omitted";
    }
    match navigate(snapshot, root, names, path) {
        Nav::Arguments => "present",
        Nav::Value(DecodedValue::Null) => "null",
        Nav::Value(_) => "present",
        Nav::Missing => "missing",
        Nav::Unavailable(reason) => reason.code(),
    }
}

/// The raw slot a first argument segment names, without folding omitted
/// arguments into `Missing`.
fn argument_slot_raw<'a>(
    slots: &'a [DecodedValue],
    names: Option<&ArgumentNames>,
    segment: &Segment,
) -> Result<&'a DecodedValue, ()> {
    let index = match segment {
        Segment::Key(name) => {
            let names = names.ok_or(())?;
            if names.slots.len() != slots.len() {
                return Err(());
            }
            names.position(name).ok_or(())?
        }
        Segment::Index(index) => usize::try_from(*index).map_err(|_| ())?,
    };
    slots.get(index).ok_or(())
}

/// How a navigated value presents in SQL.
#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
}

/// The BAML kind behind a SQL scalar, so typed output formats can restore
/// booleans, bigints and structured values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Captured null.
    Null,
    Bool,
    Int,
    Float,
    String,
    Bigint,
    Enum,
    /// Structured value rendered as a JSON envelope.
    Json,
    /// Absent path: SQL NULL.
    Missing,
    /// Evidence unavailable: SQL NULL, reported separately.
    Unavailable,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool => "bool",
            Self::Int => "int",
            Self::Float => "float",
            Self::String => "string",
            Self::Bigint => "bigint",
            Self::Enum => "enum",
            Self::Json => "json",
            Self::Missing => "missing",
            Self::Unavailable => "unavailable",
        }
    }
    pub fn from_label(label: &str) -> Option<Self> {
        Some(match label {
            "null" => Self::Null,
            "bool" => Self::Bool,
            "int" => Self::Int,
            "float" => Self::Float,
            "string" => Self::String,
            "bigint" => Self::Bigint,
            "enum" => Self::Enum,
            "json" => Self::Json,
            "missing" => Self::Missing,
            "unavailable" => Self::Unavailable,
            _ => return None,
        })
    }
}

/// Convert a navigation result to its SQL presentation. Leaves become native
/// SQL values; structured values become bounded JSON envelope text.
pub fn to_scalar(
    snapshot: &DecodedSnapshot,
    nav: Nav<'_>,
    names: Option<&ArgumentNames>,
    limits: &RenderLimits,
) -> (Scalar, Kind) {
    let value = match nav {
        Nav::Missing => return (Scalar::Null, Kind::Missing),
        Nav::Unavailable(_) => return (Scalar::Null, Kind::Unavailable),
        Nav::Arguments => {
            let json = render_arguments(snapshot, names, limits);
            return (Scalar::Text(json.to_string()), Kind::Json);
        }
        Nav::Value(value) => value,
    };
    match value {
        DecodedValue::Null => (Scalar::Null, Kind::Null),
        DecodedValue::Bool(b) => (Scalar::Integer(i64::from(*b)), Kind::Bool),
        DecodedValue::Int(n) => (Scalar::Integer(*n), Kind::Int),
        DecodedValue::Float(f) if f.is_finite() => (Scalar::Real(*f), Kind::Float),
        DecodedValue::Float(f) => (Scalar::Text(non_finite(*f).into()), Kind::Float),
        DecodedValue::String(s) => (Scalar::Text(s.to_string()), Kind::String),
        DecodedValue::Bigint(n) => (Scalar::Text(n.to_string()), Kind::Bigint),
        DecodedValue::Enum { name, .. } => (Scalar::Text(name.to_string()), Kind::Enum),
        other => (
            Scalar::Text(render_value(snapshot, other, limits).to_string()),
            Kind::Json,
        ),
    }
}

fn non_finite(f: f64) -> &'static str {
    if f.is_nan() {
        "NaN"
    } else if f > 0.0 {
        "Infinity"
    } else {
        "-Infinity"
    }
}

/// A comparison operand from SQL.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Operand<'a> {
    Null,
    Integer(i64),
    Real(f64),
    Text(&'a str),
    Bool(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

impl CmpOp {
    pub fn parse(op: &str) -> Option<Self> {
        Some(match op {
            "=" | "==" => Self::Eq,
            "!=" | "<>" => Self::NotEq,
            "<" => Self::Lt,
            "<=" => Self::LtEq,
            ">" => Self::Gt,
            ">=" => Self::GtEq,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::NotEq => "!=",
            Self::Lt => "<",
            Self::LtEq => "<=",
            Self::Gt => ">",
            Self::GtEq => ">=",
        }
    }
    /// Swap sides: `a < b` is `b > a`.
    #[must_use]
    pub fn flip(self) -> Self {
        match self {
            Self::Lt => Self::Gt,
            Self::LtEq => Self::GtEq,
            Self::Gt => Self::Lt,
            Self::GtEq => Self::LtEq,
            other => other,
        }
    }
    fn holds(self, ordering: Ordering) -> bool {
        match self {
            Self::Eq => ordering == Ordering::Equal,
            Self::NotEq => ordering != Ordering::Equal,
            Self::Lt => ordering == Ordering::Less,
            Self::LtEq => ordering != Ordering::Greater,
            Self::Gt => ordering == Ordering::Greater,
            Self::GtEq => ordering != Ordering::Less,
        }
    }
    fn is_equality(self) -> bool {
        matches!(self, Self::Eq | Self::NotEq)
    }
}

/// A comparable leaf, borrowed from a snapshot or an SQL operand.
#[derive(Clone, Copy, Debug)]
pub enum Leaf<'a> {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Bigint(&'a BigInt),
    Text(&'a str),
    /// Enum value: equal to text naming its variant.
    Enum(&'a str),
    /// Any structured value: never equal to a leaf, never ordered.
    Structured,
}

pub fn leaf_of(nav: Nav<'_>) -> Option<Leaf<'_>> {
    Some(match nav {
        Nav::Missing | Nav::Unavailable(_) => return None,
        Nav::Arguments => Leaf::Structured,
        Nav::Value(value) => match value {
            DecodedValue::Null => Leaf::Null,
            DecodedValue::Bool(b) => Leaf::Bool(*b),
            DecodedValue::Int(n) => Leaf::Int(*n),
            DecodedValue::Float(f) => Leaf::Float(*f),
            DecodedValue::Bigint(n) => Leaf::Bigint(n),
            DecodedValue::String(s) => Leaf::Text(s),
            DecodedValue::Enum { name, .. } => Leaf::Enum(name),
            _ => Leaf::Structured,
        },
    })
}

pub fn leaf_of_operand(operand: Operand<'_>) -> Option<Leaf<'_>> {
    Some(match operand {
        // SQL NULL compares as unknown.
        Operand::Null => return None,
        Operand::Integer(n) => Leaf::Int(n),
        Operand::Real(f) => Leaf::Float(f),
        Operand::Text(s) => Leaf::Text(s),
        Operand::Bool(b) => Leaf::Bool(b),
    })
}

/// BAML comparison semantics without SQLite's implicit coercions.
/// `None` is SQL NULL: an unknown result, e.g. ordering across kinds.
/// Numbers compare exactly across int/float/bigint; every NaN equals NaN and
/// orders against nothing; text compares bytewise; a captured null equals
/// only null; structured values are unequal to every leaf.
pub fn compare(left: Leaf<'_>, op: CmpOp, right: Leaf<'_>) -> Option<bool> {
    use Leaf as L;
    // Equality has an answer across kinds (unequal); ordering does not.
    let equality = |equal: bool| op.is_equality().then(|| equal == (op == CmpOp::Eq));
    match (left, right) {
        // Whole values use equality::captured; a leaf has no graph evidence.
        (L::Structured, L::Structured) => None,
        (L::Structured, _) | (_, L::Structured) => equality(false),
        (L::Null, L::Null) => equality(true),
        (L::Null, _) | (_, L::Null) => equality(false),
        (L::Bool(a), L::Bool(b)) => Some(op.holds(a.cmp(&b))),
        (L::Text(a), L::Text(b)) => Some(op.holds(a.as_bytes().cmp(b.as_bytes()))),
        (L::Enum(a), L::Enum(b) | L::Text(b)) | (L::Text(a), L::Enum(b)) => equality(a == b),
        (L::Float(a), L::Float(b)) if a.is_nan() || b.is_nan() => {
            equality(a.is_nan() && b.is_nan())
        }
        (L::Float(a), L::Float(b)) if op.is_equality() => equality(a.to_bits() == b.to_bits()),
        (a, b) => match numeric(a, b) {
            Some(ordering) => Some(op.holds(ordering)),
            None => equality(false),
        },
    }
}

fn numeric(a: Leaf<'_>, b: Leaf<'_>) -> Option<Ordering> {
    use Leaf as L;
    Some(match (a, b) {
        (L::Int(x), L::Int(y)) => x.cmp(&y),
        (L::Int(x), L::Bigint(y)) => BigInt::from(x).cmp(y),
        (L::Bigint(x), L::Int(y)) => x.cmp(&BigInt::from(y)),
        (L::Bigint(x), L::Bigint(y)) => x.cmp(y),
        (L::Int(x), L::Float(y)) => exact_int_float(&BigInt::from(x), y)?,
        (L::Float(x), L::Int(y)) => exact_int_float(&BigInt::from(y), x)?.reverse(),
        (L::Bigint(x), L::Float(y)) => exact_int_float(x, y)?,
        (L::Float(x), L::Bigint(y)) => exact_int_float(y, x)?.reverse(),
        (L::Float(x), L::Float(y)) => x.partial_cmp(&y)?,
        _ => return None,
    })
}

/// Exact ordering of an integer against a float (no lossy `as f64`).
fn exact_int_float(int: &BigInt, float: f64) -> Option<Ordering> {
    if float.is_nan() {
        return None;
    }
    if float.is_infinite() {
        return Some(if float > 0.0 {
            Ordering::Less
        } else {
            Ordering::Greater
        });
    }
    let truncated = float.trunc();
    let whole = num_bigint_from_f64(truncated);
    match int.cmp(&whole) {
        Ordering::Equal => {
            let fraction = float - truncated;
            Some(if fraction > 0.0 {
                Ordering::Less
            } else if fraction < 0.0 {
                Ordering::Greater
            } else {
                Ordering::Equal
            })
        }
        other => Some(other),
    }
}

/// Exact integer value of an integral finite f64.
fn num_bigint_from_f64(value: f64) -> BigInt {
    // An integral f64 formats exactly with precision 0.
    format!("{value:.0}")
        .parse::<BigInt>()
        .unwrap_or_else(|_| BigInt::from(0))
}

/// `IS NULL` for a navigated value: captured null and absent paths are
/// null-like; unavailable evidence is unknown (`None`).
pub fn is_null(nav: Nav<'_>) -> Option<bool> {
    match nav {
        Nav::Missing | Nav::Value(DecodedValue::Null) => Some(true),
        Nav::Unavailable(_) => None,
        _ => Some(false),
    }
}

/// Bounds for whole-value rendering.
#[derive(Clone, Copy, Debug)]
pub struct RenderLimits {
    pub max_depth: usize,
    pub max_nodes: usize,
    /// Byte data rendered as base64 per object; longer data reports length.
    pub max_bytes_inline: usize,
}
impl Default for RenderLimits {
    fn default() -> Self {
        Self {
            max_depth: 64,
            max_nodes: 100_000,
            max_bytes_inline: 4096,
        }
    }
}

/// Render the argument root: `{name: value}` when names match the slots,
/// otherwise `{"$args": [..]}`.
pub fn render_arguments(
    snapshot: &DecodedSnapshot,
    names: Option<&ArgumentNames>,
    limits: &RenderLimits,
) -> Json {
    let btel_snapshot::DecodedRoot::FunctionArgs { slots, .. } = &snapshot.root else {
        return envelope("$unavailable", Json::from("capture_root_mismatch"));
    };
    let mut renderer = Renderer::new(snapshot, slots.iter(), limits);
    match names.filter(|names| names.slots.len() == slots.len()) {
        Some(names) if names.slots.iter().all(|slot| slot.name.is_some()) => {
            let mut map = Map::new();
            for (slot, value) in names.slots.iter().zip(slots) {
                let name = slot.name.clone().unwrap_or_default();
                map.insert(name, renderer.value(value, 1));
            }
            Json::Object(map)
        }
        _ => envelope(
            "$args",
            Json::Array(slots.iter().map(|value| renderer.value(value, 1)).collect()),
        ),
    }
}

/// Render one value as a bounded JSON envelope. Plain JSON where it is
/// unambiguous; `$`-prefixed markers for BAML kinds JSON lacks, truncation,
/// opaque objects, and graph structure (`$id`/`$ref` for shared or cyclic
/// objects). This is a display format, not a lossless serialization.
pub fn render_value(
    snapshot: &DecodedSnapshot,
    value: &DecodedValue,
    limits: &RenderLimits,
) -> Json {
    Renderer::new(snapshot, std::iter::once(value), limits).value(value, 0)
}

fn envelope(key: &str, value: Json) -> Json {
    let mut map = Map::new();
    map.insert(key.into(), value);
    Json::Object(map)
}

fn limit_label(limit: Limit) -> &'static str {
    match limit {
        Limit::Values => "values",
        Limit::Objects => "objects",
        Limit::Bytes => "bytes",
        Limit::Depth => "depth",
    }
}

fn description_label(kind: Description) -> &'static str {
    match kind {
        Description::Function => "function",
        Description::Closure => "closure",
        Description::BoundMethod => "bound_method",
        Description::GenericFunction => "generic_function",
        Description::HostFunction => "host_function",
        Description::Future => "future",
        Description::UnscheduledFuture => "unscheduled_future",
        Description::Package => "package",
        Description::Interface => "interface",
        Description::Implementation => "implementation",
        Description::TypeAlias => "type_alias",
        Description::Sentinel => "sentinel",
    }
}

struct Renderer<'a> {
    snapshot: &'a DecodedSnapshot,
    limits: &'a RenderLimits,
    /// Objects reachable more than once from the rendered roots.
    shared: HashMap<u32, u32>,
    rendered: HashMap<u32, u32>,
    nodes: usize,
}

impl<'a> Renderer<'a> {
    fn new(
        snapshot: &'a DecodedSnapshot,
        roots: impl Iterator<Item = &'a DecodedValue>,
        limits: &'a RenderLimits,
    ) -> Self {
        // Count incoming references reachable from the roots (iteratively).
        let mut counts: HashMap<u32, u32> = HashMap::new();
        let mut stack: Vec<u32> = Vec::new();
        let visit = |value: &DecodedValue, counts: &mut HashMap<u32, u32>, stack: &mut Vec<u32>| {
            if let DecodedValue::Object(id) = value {
                let count = counts.entry(*id).or_insert(0);
                *count += 1;
                if *count == 1 {
                    stack.push(*id);
                }
            }
        };
        for root in roots {
            visit(root, &mut counts, &mut stack);
        }
        while let Some(id) = stack.pop() {
            match snapshot.object(id) {
                DecodedObject::List { items, .. } => {
                    for item in items {
                        visit(item, &mut counts, &mut stack);
                    }
                }
                DecodedObject::Map { entries, .. }
                | DecodedObject::Instance {
                    fields: entries, ..
                } => {
                    for (_, value) in entries {
                        visit(value, &mut counts, &mut stack);
                    }
                }
                DecodedObject::Cell(value) => visit(value, &mut counts, &mut stack),
                _ => {}
            }
        }
        let mut shared = HashMap::new();
        for (id, count) in counts {
            if count > 1 {
                let next = u32::try_from(shared.len()).unwrap_or(u32::MAX);
                shared.insert(id, next);
            }
        }
        Self {
            snapshot,
            limits,
            shared,
            rendered: HashMap::new(),
            nodes: 0,
        }
    }

    fn declaration_name(&self, id: u32) -> Json {
        match self.snapshot.object(id) {
            DecodedObject::Declaration { name, .. } => {
                Json::from(name.0.display_name().to_string())
            }
            _ => Json::Null,
        }
    }

    fn value(&mut self, value: &DecodedValue, depth: usize) -> Json {
        self.nodes += 1;
        if self.nodes > self.limits.max_nodes {
            return envelope("$truncated", Json::from("render_size"));
        }
        match value {
            DecodedValue::Null => Json::Null,
            DecodedValue::OmittedArg => envelope("$omitted", Json::Bool(true)),
            DecodedValue::Bool(b) => Json::Bool(*b),
            DecodedValue::Int(n) => Json::from(*n),
            DecodedValue::Float(f) => serde_json::Number::from_f64(*f).map_or_else(
                || envelope("$float", Json::from(non_finite(*f))),
                Json::Number,
            ),
            DecodedValue::String(s) => Json::from(s.to_string()),
            DecodedValue::Bigint(n) => envelope("$bigint", Json::from(n.to_string())),
            DecodedValue::Type(ty) => envelope(
                "$type",
                ty.decoded
                    .as_ref()
                    .map_or(Json::Null, |ty| Json::from(ty.to_string())),
            ),
            DecodedValue::Enum {
                declaration, name, ..
            } => {
                let mut map = Map::new();
                map.insert("$enum".into(), self.declaration_name(*declaration));
                map.insert("$variant".into(), Json::from(name.to_string()));
                Json::Object(map)
            }
            DecodedValue::Truncated(limit) => {
                envelope("$truncated", Json::from(limit_label(*limit)))
            }
            DecodedValue::Object(id) => self.object(*id, depth),
        }
    }

    fn object(&mut self, id: u32, depth: usize) -> Json {
        let shared = self.shared.get(&id).copied();
        if let Some(label) = shared {
            if self.rendered.contains_key(&id) {
                return envelope("$ref", Json::from(label));
            }
            self.rendered.insert(id, label);
        }
        if depth >= self.limits.max_depth {
            return envelope("$truncated", Json::from("render_depth"));
        }
        let mut map = Map::new();
        if let Some(label) = shared {
            map.insert("$id".into(), Json::from(label));
        }
        let snapshot = self.snapshot;
        match snapshot.object(id) {
            DecodedObject::List {
                items,
                original_len,
                ..
            } => {
                let rendered = items.iter().map(|v| self.value(v, depth + 1)).collect();
                if shared.is_none() && items.len() as u64 == *original_len {
                    return Json::Array(rendered);
                }
                map.insert("$list".into(), Json::Array(rendered));
                if items.len() as u64 != *original_len {
                    map.insert("$original_len".into(), Json::from(*original_len));
                }
            }
            DecodedObject::Map {
                entries,
                original_len,
                ..
            } => {
                let mut inner = Map::new();
                for (key, value) in entries {
                    inner.insert(key.to_string(), self.value(value, depth + 1));
                }
                let plain = shared.is_none()
                    && entries.len() as u64 == *original_len
                    && !entries.iter().any(|(key, _)| key.starts_with('$'));
                if plain {
                    return Json::Object(inner);
                }
                map.insert("$map".into(), Json::Object(inner));
                if entries.len() as u64 != *original_len {
                    map.insert("$original_len".into(), Json::from(*original_len));
                }
            }
            DecodedObject::Instance {
                declaration,
                fields,
                original_len,
                ..
            } => {
                map.insert("$class".into(), self.declaration_name(*declaration));
                let mut inner = Map::new();
                for (key, value) in fields {
                    inner.insert(key.to_string(), self.value(value, depth + 1));
                }
                if inner.keys().any(|key| key.starts_with('$')) {
                    map.insert("$fields".into(), Json::Object(inner));
                } else {
                    map.extend(inner);
                }
                if fields.len() as u64 != *original_len {
                    map.insert("$original_len".into(), Json::from(*original_len));
                }
            }
            DecodedObject::Bytes { data, original_len } => {
                use base64::Engine as _;
                if data.len() <= self.limits.max_bytes_inline {
                    map.insert(
                        "$bytes".into(),
                        Json::from(base64::engine::general_purpose::STANDARD.encode(data)),
                    );
                } else {
                    map.insert("$bytes".into(), Json::Null);
                }
                map.insert("$len".into(), Json::from(*original_len));
                if data.len() as u64 != *original_len {
                    map.insert("$captured_len".into(), Json::from(data.len()));
                }
            }
            DecodedObject::Declaration { name, is_enum, .. } => {
                map.insert(
                    if *is_enum {
                        "$enum_type"
                    } else {
                        "$class_type"
                    }
                    .into(),
                    Json::from(name.0.display_name().to_string()),
                );
            }
            DecodedObject::Cell(value) => {
                if shared.is_none() {
                    return self.value(value, depth);
                }
                let inner = self.value(value, depth + 1);
                map.insert("$cell".into(), inner);
            }
            DecodedObject::NonSnapshotable => {
                map.insert("$opaque".into(), Json::from("host_value"));
            }
            DecodedObject::Descriptive { kind, name } => {
                map.insert("$opaque".into(), Json::from(description_label(*kind)));
                if let Some(name) = name {
                    map.insert("$name".into(), Json::from(name.to_string()));
                }
            }
            DecodedObject::Truncated(limit) => {
                map.insert("$truncated".into(), Json::from(limit_label(*limit)));
            }
        }
        Json::Object(map)
    }
}

#[cfg(test)]
mod tests;
