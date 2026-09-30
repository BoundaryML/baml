//! Captured-value navigation, comparison and bounded rendering over decoded
//! CAS graphs.
//!
//! Navigation follows constant string keys (map entries, instance fields,
//! named arguments) and zero-based list indices through the graph without
//! expanding it. A value may continue in other blobs; they are read only
//! when a path or a rendering reaches them. Three outcomes stay distinct:
//! - a captured BAML `null` is data;
//! - an absent path, an omitted argument or an incompatible step is
//!   `Missing`, a SQL-NULL-like non-match that keeps an answer complete;
//! - truncated evidence, unknown argument names or a blob that cannot be
//!   read are `Unavailable`: the recording cannot answer, and callers report
//!   that separately.
use std::{cmp::Ordering, collections::HashMap, sync::Arc};

use btel_snapshot::{
    CasId, DecodedMediaSource, DecodedObject, DecodedSnapshot, DecodedValue, Description, Limit,
    NodeId,
};
use num_bigint::BigInt;
use serde_json::{Map, Value as Json};

use crate::evidence::ArgumentNames;

pub mod equality;
mod span;
pub use span::{BlobSource, Found, Located, media_content};
use span::{Examine, Span, reach};

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
    /// A blob the value continues in cannot be read: its CAS diagnostic code.
    Blob(&'static str),
    /// The value continues in more blobs than one operation may read.
    BlobBudget,
}

impl Unavailable {
    pub fn code(self) -> &'static str {
        match self {
            Self::Truncated(_) => "value_truncated",
            Self::ArgumentNamesUnknown => "argument_names_unknown",
            Self::ArgumentLayoutMismatch => "argument_layout_mismatch",
            Self::WrongRoot => "capture_root_mismatch",
            Self::CellCycle => "value_cycle",
            Self::Blob(code) => code,
            Self::BlobBudget => "value_blob_budget",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Nav {
    /// The whole argument root (a bare `args`), in its blob.
    Arguments(Arc<DecodedSnapshot>),
    Value(Located),
    Missing,
    Unavailable(Unavailable),
}

fn argument_slot<'a>(
    slots: &'a [DecodedValue],
    names: Option<&ArgumentNames>,
    segment: &Segment,
) -> Result<&'a DecodedValue, Nav> {
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
    slots.get(index).ok_or(Nav::Missing)
}

/// Follow `path` from a snapshot root, entering at most `max_blobs` other
/// blobs. Cells are an execution detail: navigation sees through them, and a
/// cycle of cells is unavailable rather than looping or becoming absent data.
pub fn navigate(
    source: &(impl BlobSource + ?Sized),
    snapshot: &Arc<DecodedSnapshot>,
    root: Root,
    names: Option<&ArgumentNames>,
    path: &[Segment],
    max_blobs: usize,
) -> Nav {
    let (first, rest) = match (root, &snapshot.root) {
        (Root::Arguments, btel_snapshot::DecodedRoot::FunctionArgs { slots, .. }) => {
            let Some((first, rest)) = path.split_first() else {
                return Nav::Arguments(Arc::clone(snapshot));
            };
            match argument_slot(slots, names, first) {
                Ok(value) => (value, rest),
                Err(nav) => return nav,
            }
        }
        (Root::Value, btel_snapshot::DecodedRoot::Value(value)) => (value, path),
        (Root::Arguments, btel_snapshot::DecodedRoot::Value(_))
        | (Root::Value, btel_snapshot::DecodedRoot::FunctionArgs { .. }) => {
            return Nav::Unavailable(Unavailable::WrongRoot);
        }
    };
    let mut blobs_left = max_blobs;
    let mut nav = reach(source, snapshot, first, &mut blobs_left);
    for segment in rest {
        let Nav::Value(current) = nav else {
            return nav;
        };
        nav = match step(&current, segment) {
            Ok(value) => reach(source, current.blob(), value, &mut blobs_left),
            Err(nav) => nav,
        };
    }
    nav
}

fn step<'a>(current: &'a Located, segment: &Segment) -> Result<&'a DecodedValue, Nav> {
    let id = match current.value() {
        Found::Object(id) => *id,
        // Scalars, null and enum values have no children.
        Found::Null
        | Found::Bool(_)
        | Found::Int(_)
        | Found::Float(_)
        | Found::String(_)
        | Found::Bigint(_)
        | Found::Type(_)
        | Found::Enum { .. } => return Err(Nav::Missing),
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
    match (current.blob().object(id), segment) {
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
        (DecodedObject::List { .. }, Segment::Key(_))
        | (DecodedObject::Map { .. } | DecodedObject::Instance { .. }, Segment::Index(_))
        | (
            DecodedObject::Uint8Array { .. }
            | DecodedObject::Declaration { .. }
            | DecodedObject::Cell(_)
            | DecodedObject::NonSnapshotable
            | DecodedObject::Descriptive { .. }
            | DecodedObject::Media(_),
            _,
        ) => Err(Nav::Missing),
    }
}

/// What a path finds, as `baml_value_state` reports it: `present`, `null`
/// (a captured null), `missing` (absent key or index), `omitted` (an
/// argument the caller did not pass) or an unavailable-evidence code such as
/// `value_truncated`. Distinguishes the cases navigation folds together.
pub fn state(
    source: &(impl BlobSource + ?Sized),
    snapshot: &Arc<DecodedSnapshot>,
    root: Root,
    names: Option<&ArgumentNames>,
    path: &[Segment],
    max_blobs: usize,
) -> &'static str {
    if let (Root::Arguments, btel_snapshot::DecodedRoot::FunctionArgs { slots, .. }, [first]) =
        (root, &snapshot.root, path)
        && let Ok(DecodedValue::OmittedArg) = argument_slot(slots, names, first)
    {
        return "omitted";
    }
    match navigate(source, snapshot, root, names, path, max_blobs) {
        Nav::Arguments(_) => "present",
        Nav::Value(found) => match found.value() {
            Found::Null => "null",
            Found::Bool(_)
            | Found::Int(_)
            | Found::Float(_)
            | Found::String(_)
            | Found::Bigint(_)
            | Found::Type(_)
            | Found::Enum { .. }
            | Found::Object(_) => "present",
        },
        Nav::Missing => "missing",
        Nav::Unavailable(reason) => reason.code(),
    }
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

/// A navigation result as SQL presents it.
#[derive(Clone, Debug, PartialEq)]
pub struct Presented {
    pub scalar: Scalar,
    pub kind: Kind,
    /// Why part of a structured value is marked unavailable in its rendering.
    pub incomplete: Option<Unavailable>,
}

/// Convert a navigation result to its SQL presentation. Leaves become native
/// SQL values; structured values become bounded JSON envelope text.
pub fn to_scalar(
    source: &(impl BlobSource + ?Sized),
    nav: &Nav,
    names: Option<&ArgumentNames>,
    limits: &RenderLimits,
) -> Presented {
    let whole = |scalar, kind| Presented {
        scalar,
        kind,
        incomplete: None,
    };
    let rendered = |rendered: Rendered| Presented {
        scalar: Scalar::Text(rendered.json.to_string()),
        kind: Kind::Json,
        incomplete: rendered.incomplete,
    };
    let found = match nav {
        Nav::Missing => return whole(Scalar::Null, Kind::Missing),
        Nav::Unavailable(_) => return whole(Scalar::Null, Kind::Unavailable),
        Nav::Arguments(snapshot) => {
            return rendered(render_arguments(source, snapshot, names, limits));
        }
        Nav::Value(found) => found,
    };
    match found.value() {
        Found::Null => whole(Scalar::Null, Kind::Null),
        Found::Bool(b) => whole(Scalar::Integer(i64::from(*b)), Kind::Bool),
        Found::Int(n) => whole(Scalar::Integer(*n), Kind::Int),
        Found::Float(f) if f.is_finite() => whole(Scalar::Real(*f), Kind::Float),
        Found::Float(f) => whole(Scalar::Text(non_finite(*f).into()), Kind::Float),
        Found::String(s) => whole(Scalar::Text(s.to_string()), Kind::String),
        Found::Bigint(n) => whole(Scalar::Text(n.to_string()), Kind::Bigint),
        Found::Enum { name, .. } => whole(Scalar::Text(name.to_string()), Kind::Enum),
        Found::Object(_) | Found::Type(_) => rendered(render_value(source, found, limits)),
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

pub fn leaf_of(nav: &Nav) -> Option<Leaf<'_>> {
    Some(match nav {
        Nav::Missing | Nav::Unavailable(_) => return None,
        Nav::Arguments(_) => Leaf::Structured,
        Nav::Value(found) => match found.value() {
            Found::Null => Leaf::Null,
            Found::Bool(b) => Leaf::Bool(*b),
            Found::Int(n) => Leaf::Int(*n),
            Found::Float(f) => Leaf::Float(*f),
            Found::Bigint(n) => Leaf::Bigint(n),
            Found::String(s) => Leaf::Text(s),
            Found::Enum { name, .. } => Leaf::Enum(name),
            Found::Object(_) | Found::Type(_) => Leaf::Structured,
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
pub fn is_null(nav: &Nav) -> Option<bool> {
    match nav {
        Nav::Missing => Some(true),
        Nav::Unavailable(_) => None,
        Nav::Arguments(_) => Some(false),
        Nav::Value(found) => Some(matches!(found.value(), Found::Null)),
    }
}

/// Bounds for whole-value rendering.
#[derive(Clone, Copy, Debug)]
pub struct RenderLimits {
    pub max_depth: usize,
    pub max_nodes: usize,
    /// Byte data rendered as base64 per object; longer data reports length.
    pub max_bytes_inline: usize,
    /// Blobs read for one value beyond the one it starts in, for navigation
    /// and for rendering each.
    pub max_blobs: usize,
}
impl Default for RenderLimits {
    fn default() -> Self {
        Self {
            max_depth: 64,
            max_nodes: 100_000,
            max_bytes_inline: 4096,
            max_blobs: 1024,
        }
    }
}

/// A rendering, and why part of it is marked `$unavailable`, if any is.
#[derive(Clone, Debug, PartialEq)]
pub struct Rendered {
    pub json: Json,
    /// The first reason met in output order.
    pub incomplete: Option<Unavailable>,
}

/// Render the argument root: `{name: value}` when names match the slots,
/// otherwise `{"$args": [..]}`.
pub fn render_arguments(
    source: &(impl BlobSource + ?Sized),
    snapshot: &Arc<DecodedSnapshot>,
    names: Option<&ArgumentNames>,
    limits: &RenderLimits,
) -> Rendered {
    let btel_snapshot::DecodedRoot::FunctionArgs { slots, .. } = &snapshot.root else {
        return Rendered {
            json: envelope("$unavailable", Json::from(Unavailable::WrongRoot.code())),
            incomplete: Some(Unavailable::WrongRoot),
        };
    };
    let span = Span::load(
        source,
        slots.iter().map(|slot| (snapshot, slot)),
        Examine::Structure,
        limits.max_blobs,
    );
    let mut renderer = Renderer::new(&span, limits);
    let json = match names.filter(|names| names.slots.len() == slots.len()) {
        Some(names) if names.slots.iter().all(|slot| slot.name.is_some()) => {
            let mut map = Map::new();
            for (slot, value) in names.slots.iter().zip(slots) {
                let name = slot.name.clone().unwrap_or_default();
                map.insert(name, renderer.value(snapshot, value, 1));
            }
            Json::Object(map)
        }
        _ => envelope(
            "$args",
            Json::Array(
                slots
                    .iter()
                    .map(|value| renderer.value(snapshot, value, 1))
                    .collect(),
            ),
        ),
    };
    Rendered {
        json,
        incomplete: renderer.incomplete,
    }
}

/// Render one value as a bounded JSON envelope. Plain JSON where it is
/// unambiguous; `$`-prefixed markers for BAML kinds JSON lacks, truncation,
/// opaque objects, and graph structure (`$id`/`$ref` for shared or cyclic
/// objects). A part in a blob that cannot be read is `$unavailable` with the
/// reason's code. Media is described, never rendered:
/// `{"$media": kind, "mime", "url" | "file", "base64_len"}`, with
/// `base64_len` present when the value holds content, and its content is not
/// read. This is a display format, not a lossless serialization.
pub fn render_value(
    source: &(impl BlobSource + ?Sized),
    found: &Located,
    limits: &RenderLimits,
) -> Rendered {
    let value = DecodedValue::from(found.value());
    let span = Span::load(
        source,
        [(found.blob(), &value)],
        Examine::Structure,
        limits.max_blobs,
    );
    let mut renderer = Renderer::new(&span, limits);
    let json = renderer.value(found.blob(), &value, 0);
    Rendered {
        json,
        incomplete: renderer.incomplete,
    }
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
    span: &'a Span,
    limits: &'a RenderLimits,
    /// Label of each shared object already printed with its `$id`.
    rendered: HashMap<(CasId, NodeId), u32>,
    nodes: usize,
    incomplete: Option<Unavailable>,
}

impl<'a> Renderer<'a> {
    fn new(span: &'a Span, limits: &'a RenderLimits) -> Self {
        Self {
            span,
            limits,
            rendered: HashMap::new(),
            nodes: 0,
            incomplete: None,
        }
    }

    fn unavailable(&mut self, reason: Unavailable) -> Json {
        self.incomplete.get_or_insert(reason);
        envelope("$unavailable", Json::from(reason.code()))
    }

    fn declaration_name(blob: &DecodedSnapshot, id: NodeId) -> Json {
        match blob.object(id) {
            DecodedObject::Declaration { name, .. } => {
                Json::from(name.0.display_name().to_string())
            }
            DecodedObject::Uint8Array { .. }
            | DecodedObject::List { .. }
            | DecodedObject::Map { .. }
            | DecodedObject::Instance { .. }
            | DecodedObject::Cell(_)
            | DecodedObject::NonSnapshotable
            | DecodedObject::Descriptive { .. }
            | DecodedObject::Media(_)
            | DecodedObject::Truncated(_) => Json::Null,
        }
    }

    fn value(&mut self, blob: &'a DecodedSnapshot, value: &'a DecodedValue, depth: usize) -> Json {
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
                map.insert("$enum".into(), Self::declaration_name(blob, *declaration));
                map.insert("$variant".into(), Json::from(name.to_string()));
                Json::Object(map)
            }
            DecodedValue::Truncated(limit) => {
                envelope("$truncated", Json::from(limit_label(*limit)))
            }
            DecodedValue::Object(id) => self.object(blob, *id, depth),
            DecodedValue::External(child) => match self.span.root_of(blob, *child) {
                // A blob's root value is stored in it, so this ends there.
                Ok((child, value)) => self.value(child, value, depth),
                Err(reason) => self.unavailable(reason),
            },
            DecodedValue::ExternalNode { child, node } => {
                match self.span.node_of(blob, *child, *node) {
                    Ok((child, id)) => self.object(child, id, depth),
                    Err(reason) => self.unavailable(reason),
                }
            }
        }
    }

    fn object(&mut self, blob: &'a DecodedSnapshot, id: NodeId, depth: usize) -> Json {
        let key = (blob.id, id);
        let is_shared = self.span.shared(blob, id);
        if is_shared && let Some(label) = self.rendered.get(&key) {
            return envelope("$ref", Json::from(*label));
        }
        // A truncated visit prints no `$id`, so it must not take the label.
        if depth >= self.limits.max_depth {
            return envelope("$truncated", Json::from("render_depth"));
        }
        // Labels count up in output order, so a capture renders the same way
        // in every process.
        let shared = is_shared.then(|| {
            let label = u32::try_from(self.rendered.len()).unwrap_or(u32::MAX);
            self.rendered.insert(key, label);
            label
        });
        let mut map = Map::new();
        if let Some(label) = shared {
            map.insert("$id".into(), Json::from(label));
        }
        match blob.object(id) {
            DecodedObject::List {
                items,
                original_len,
                ..
            } => {
                let rendered = items
                    .iter()
                    .map(|v| self.value(blob, v, depth + 1))
                    .collect();
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
                    inner.insert(key.to_string(), self.value(blob, value, depth + 1));
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
                map.insert("$class".into(), Self::declaration_name(blob, *declaration));
                let mut inner = Map::new();
                for (key, value) in fields {
                    inner.insert(key.to_string(), self.value(blob, value, depth + 1));
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
            DecodedObject::Uint8Array { data, original_len } => {
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
                    return self.value(blob, value, depth);
                }
                let inner = self.value(blob, value, depth + 1);
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
            DecodedObject::Media(media) => {
                map.insert("$media".into(), Json::from(media.kind.tag_str()));
                map.insert(
                    "mime".into(),
                    media.mime_type.as_deref().map_or(Json::Null, Json::from),
                );
                match &media.source {
                    DecodedMediaSource::Url { url, .. } => {
                        map.insert("url".into(), Json::from(&**url));
                    }
                    DecodedMediaSource::File { path, .. } => {
                        map.insert("file".into(), Json::from(&**path));
                    }
                    DecodedMediaSource::Base64 { .. } => {}
                }
                if let Some(data) = media.source.data() {
                    map.insert("base64_len".into(), Json::from(data.text_len()));
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
