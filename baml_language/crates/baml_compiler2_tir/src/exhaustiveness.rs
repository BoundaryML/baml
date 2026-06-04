//! Pattern exhaustiveness, irrefutability, and witness construction.
//!
//! Single source of truth for "does this set of patterns cover this type" and
//! "what value would not match this pattern." Used for match-arm exhaustiveness,
//! `let` / `for` irrefutability, and unreachable-arm detection.
//!
//! Architecture is a reduced port of rustc's `rustc_pattern_analysis`:
//! - [`Ctor`] is the head of a deconstructed pattern (a value's "shape").
//! - [`Pat`] is a pattern in deconstructed form: ctor + sub-patterns + type.
//! - `Matrix` (private) is a 2-D grid of `Pat`s; rows are arms, columns are positions.
//! - The recursive *usefulness* algorithm specializes the matrix on each ctor of
//!   the leading column, recursing into sub-pattern columns. A column is
//!   exhaustive iff its ctor enumeration is fully covered by the matrix.
//! - Missing cases produce [`Pat`]s — concrete values that no row matches.
//!
//! The interface to the type system is the [`PatCtx`] trait. It abstracts:
//! - enumerating ctors of a type (incl. `NonExhaustive` for infinite alphabets
//!   like raw `int`/`string` and unconstrained generics, per Rust),
//! - resolving sub-pattern types given a ctor + parent type (e.g. class fields
//!   with generic substitution applied, slice element types).
//!
//! Compared to rustc's analysis, this port omits: range constructors, deref/ref
//! patterns, opaque consts, hidden/private-uninhabited bookkeeping, sparse
//! field indexing, and or-pattern-as-ctor (or-patterns are pre-expanded into
//! multiple matrix rows at the call site). Everything else is structurally the
//! same algorithm.

use std::fmt;

use baml_base::Literal;
use rustc_hash::FxHashSet;

use crate::ty::{PrimitiveType, QualifiedTypeName, Ty};

// ── Constructors ─────────────────────────────────────────────────────────────

/// The "head" of a pattern. Determines arity (number of sub-pattern slots) and
/// the coverage relation against other ctors at the same column type.
///
/// Scalars (singletons) collapse into [`Ctor::Single`] carrying their type —
/// BAML's "literals are types" property means individual literal values, enum
/// variants, `null`, and bool literals are all just types, so a single variant
/// can absorb them. Structural ctors ([`Ctor::Slice`], [`Ctor::Class`]) carry
/// only the information that distinguishes them from other ctors at the same
/// type; sub-pattern positions are computed by the [`PatCtx`].
#[derive(Debug, Clone)]
pub enum Ctor {
    /// Any singleton type. Identity is determined by [`ty_ctor_identity`],
    /// which strips `TyAttr`/`Freshness` and canonicalizes float literals.
    /// Absorbs Bool, Null, Int, Float, Str literals, and flat enum variants.
    Single(Ty),
    /// Array shape. Sub-patterns' types come from the array's element type.
    Slice(SliceShape),
    /// Class destructure. Sub-patterns' types come from the class's fields,
    /// with generic substitution applied.
    Class(QualifiedTypeName, Vec<Ty>),
    /// Interface destructure. Sub-patterns' types come from the interface's
    /// field view, with generic substitution applied.
    Interface(Ty),

    /// "Which member of a union" tag, with arity 1. Sub-pattern's type is
    /// the member type carried by the ctor. Specialising on `UnionMember(M)`
    /// projects the column from `Union<...>` down to `M`, after which the
    /// algorithm recurses normally — slice splitting, class destructuring,
    /// etc. apply at that depth. Mirrors rustc's `Variant` ctor for enum
    /// variants. Without this, list/class members of a union can't be
    /// distinguished by the matrix and combined slice patterns aren't
    /// recognised as exhaustive on the list branch.
    UnionMember(Ty),

    /// Or-pattern: alternatives stored in `Pat::fields`. The arity (=
    /// number of alternatives) lives on the `Pat`, not on the ctor.
    /// Specialization on `Or` explodes the row into one row per alternative,
    /// keeping all alternatives' source `ArmId` so or-pattern usefulness
    /// aggregates per source arm. `Or` never appears in witnesses.
    Or,

    /// Matches anything of the column type. Used for `_` patterns and for
    /// elided positions in class/slice patterns.
    Wildcard,
    /// Column type has an effectively infinite alphabet (raw `int`/`string`/
    /// `float`, unconstrained generic type variables, opaque types like maps
    /// or functions). Forces a wildcard arm for exhaustiveness. Produced only
    /// by [`PatCtx::enumerate_ctors`], never by pattern lowering.
    NonExhaustive,
    /// Sentinel used during witness construction when no concrete ctor is
    /// known. Never appears in lowered source patterns.
    Missing,
}

impl PartialEq for Ctor {
    fn eq(&self, other: &Self) -> bool {
        use Ctor::{
            Class, Interface, Missing, NonExhaustive, Or, Single, Slice, UnionMember, Wildcard,
        };
        match (self, other) {
            (Single(a), Single(b)) => ty_ctor_identity(a) == ty_ctor_identity(b),
            (Slice(a), Slice(b)) => a == b,
            (Class(a, _), Class(b, _)) => a == b,
            (Interface(a), Interface(b)) => ty_ctor_identity(a) == ty_ctor_identity(b),
            (UnionMember(a), UnionMember(b)) => ty_ctor_identity(a) == ty_ctor_identity(b),
            (Or, Or)
            | (Wildcard, Wildcard)
            | (NonExhaustive, NonExhaustive)
            | (Missing, Missing) => true,
            _ => false,
        }
    }
}
impl Eq for Ctor {}

impl std::hash::Hash for Ctor {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        use Ctor::{
            Class, Interface, Missing, NonExhaustive, Or, Single, Slice, UnionMember, Wildcard,
        };
        std::mem::discriminant(self).hash(state);
        match self {
            Single(ty) => ty_ctor_identity(ty).hash(state),
            Slice(s) => s.hash(state),
            Class(qtn, _) => {
                qtn.hash(state);
            }
            Interface(ty) => ty_ctor_identity(ty).hash(state),
            UnionMember(ty) => ty_ctor_identity(ty).hash(state),
            Or | Wildcard | NonExhaustive | Missing => {}
        }
    }
}

/// The shape of an array pattern.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SliceShape {
    /// `[a, b, c]` — exactly N elements. Arity = N.
    Fixed(usize),
    /// `[a, b, ..rest, y, z]` — at least prefix+suffix elements. Arity =
    /// prefix + suffix; `rest` is a pattern-side binding, not an algorithm
    /// slot.
    Variable { prefix: usize, suffix: usize },
}

impl SliceShape {
    pub fn arity(&self) -> usize {
        match self {
            SliceShape::Fixed(n) => *n,
            SliceShape::Variable { prefix, suffix } => prefix + suffix,
        }
    }
}

impl Ctor {
    /// Does `self` cover `other` — i.e. is every value matched by `other` also
    /// matched by `self`? Equality for most ctors; slice variable-length covers
    /// fixed slices of compatible length; wildcard covers anything. `Or` is
    /// not handled here — Or rows are handled by matrix-level expansion
    /// before any normal cover check runs.
    pub fn covers(&self, other: &Ctor) -> bool {
        use Ctor::{Missing, Or, Slice, Wildcard};
        match (self, other) {
            (Wildcard, _) => true,
            (_, Wildcard) => false,
            (Slice(a), Slice(b)) => slice_covers(a, b),
            // `Missing` and `Or` never appear as a row-head or split ctor at
            // this point (Or is exploded earlier; Missing is a witness-only
            // sentinel); keep them `false` rather than deferring to `==`.
            (Or | Missing, _) | (_, Or | Missing) => false,
            // Single/Interface/UnionMember/Class/NonExhaustive: coverage is
            // exactly ctor-equality, which `PartialEq` already encodes.
            (a, b) => a == b,
        }
    }
}

fn class_ty_for_ctor(qtn: &QualifiedTypeName, args: &[Ty], fallback: &Ty) -> Ty {
    match fallback {
        Ty::Class(fallback_qtn, _) if fallback_qtn == qtn => fallback.clone(),
        _ => Ty::Class(qtn.clone(), args.to_vec()),
    }
}

fn slice_covers(a: &SliceShape, b: &SliceShape) -> bool {
    use SliceShape::{Fixed, Variable};
    match (a, b) {
        (Fixed(n), Fixed(m)) => n == m,
        (
            Variable {
                prefix: ap,
                suffix: as_,
            },
            Fixed(m),
        ) => *ap + *as_ <= *m,
        (
            Variable {
                prefix: ap,
                suffix: as_,
            },
            Variable {
                prefix: bp,
                suffix: bs,
            },
        ) => ap <= bp && as_ <= bs,
        (Fixed(_), Variable { .. }) => false,
    }
}

/// A canonicalized form of `Ty` used as the identity key for [`Ctor::Single`].
/// Strips `TyAttr` (span/comment baggage), normalizes `Ty::Literal` `Freshness`,
/// and canonicalizes float string forms (`1.0` ≡ `1.00` ≡ `1e0`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CtorIdentity(String);

/// Compute the [`CtorIdentity`] for a type. This is what `Ctor::Single` uses
/// for `Eq`/`Hash`. Two types with the same identity are the same ctor.
pub(crate) fn ty_ctor_identity(ty: &Ty) -> CtorIdentity {
    let mut s = String::new();
    write_ty_identity(&mut s, ty);
    CtorIdentity(s)
}

/// Write a `<tag>:<qtn><args...>` identity fragment. Shared by the `Class`
/// (`tag = 'C'`) and `Interface` (`tag = 'I'`) arms of `write_ty_identity`.
fn write_qtn_args(out: &mut String, tag: char, qtn: &QualifiedTypeName, args: &[Ty]) {
    use std::fmt::Write;
    let _ = write!(out, "{tag}:{qtn}<");
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_ty_identity(out, a);
    }
    out.push('>');
}

fn write_ty_identity(out: &mut String, ty: &Ty) {
    use std::fmt::Write;
    match ty {
        Ty::Literal(lit, _) => {
            out.push_str("L:");
            write_literal_identity(out, lit);
        }
        Ty::EnumVariant(qtn, name) => {
            let _ = write!(out, "EV:{qtn}::{name}");
        }
        Ty::Enum(qtn) => {
            let _ = write!(out, "E:{qtn}");
        }
        Ty::Class(qtn, args) => write_qtn_args(out, 'C', qtn, args),
        Ty::Interface(qtn, args) => write_qtn_args(out, 'I', qtn, args),
        Ty::Primitive(p) => {
            let _ = write!(out, "P:{p:?}");
        }
        Ty::Optional(inner) => {
            out.push_str("O:");
            write_ty_identity(out, inner);
        }
        Ty::Union(members) => {
            out.push_str("U:[");
            for (i, m) in members.iter().enumerate() {
                if i > 0 {
                    out.push('|');
                }
                write_ty_identity(out, m);
            }
            out.push(']');
        }
        Ty::List(elem) | Ty::EvolvingList(elem) => {
            out.push_str("Lst:");
            write_ty_identity(out, elem);
        }
        Ty::Map(k, v) | Ty::EvolvingMap(k, v) => {
            out.push_str("M:");
            write_ty_identity(out, k);
            out.push(',');
            write_ty_identity(out, v);
        }
        Ty::TypeAlias(qtn) => {
            let _ = write!(out, "A:{qtn}");
        }
        Ty::TypeVar(name) => {
            let _ = write!(out, "V:{name}");
        }
        Ty::Never => out.push_str("Never"),
        Ty::Void => out.push_str("Void"),
        Ty::BuiltinUnknown => out.push_str("BUnk"),
        Ty::Unknown => out.push_str("Unk"),
        Ty::Error => out.push_str("Err"),
        Ty::RustType => out.push_str("Rust"),
        Ty::Type => out.push_str("Type"),
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            out.push_str("Fn(");
            for (i, param) in params.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_ty_identity(out, &param.ty);
            }
            out.push_str(")->");
            write_ty_identity(out, ret);
            out.push('!');
            write_ty_identity(out, throws);
        }
        Ty::Future(value, error) => {
            out.push_str("Fut<");
            write_ty_identity(out, value);
            out.push(',');
            write_ty_identity(out, error);
            out.push('>');
        }
    }
}

fn write_literal_identity(out: &mut String, lit: &Literal) {
    use std::fmt::Write;
    match lit {
        Literal::Int(v) => {
            let _ = write!(out, "i{v}");
        }
        Literal::Bigint(v) => {
            let _ = write!(out, "i{v}n");
        }
        Literal::Bool(v) => {
            out.push_str(if *v { "bT" } else { "bF" });
        }
        Literal::String(v) => {
            let _ = write!(out, "s{v:?}");
        }
        Literal::Float(s) => {
            // Canonicalize via parse → bit pattern, normalizing -0 and NaN.
            let canon = match s.parse::<f64>() {
                Ok(f) if f.is_nan() => "nan".to_string(),
                Ok(0.0) => "0".to_string(),
                Ok(f) => format!("{:x}", f.to_bits()),
                Err(_) => format!("raw:{s}"),
            };
            let _ = write!(out, "f{canon}");
        }
    }
}

// ── Pattern ──────────────────────────────────────────────────────────────────

/// A pattern in the form the usefulness algorithm consumes. The ctor arity is
/// `fields.len()`. Wildcards are filled in for elided positions during lowering.
///
/// The same type doubles as a "missing-case" witness: a concrete pattern shape
/// produced by the algorithm (rather than user input) that proves a match is
/// non-exhaustive (or an irrefutable context is refutable).
#[derive(Debug, Clone)]
pub struct Pat {
    pub ctor: Ctor,
    pub fields: Vec<Pat>,
    pub ty: Ty,
}

impl Pat {
    pub fn wildcard(ty: Ty) -> Self {
        Self {
            ctor: Ctor::Wildcard,
            fields: vec![],
            ty,
        }
    }
    pub fn new(ctor: Ctor, fields: Vec<Pat>, ty: Ty) -> Self {
        Self { ctor, fields, ty }
    }
    pub fn single(ty: Ty, scrutinee_ty: Ty) -> Self {
        Self {
            ctor: Ctor::Single(ty),
            fields: vec![],
            ty: scrutinee_ty,
        }
    }
    pub fn class(qtn: QualifiedTypeName, fields: Vec<Pat>, ty: Ty) -> Self {
        Self::class_inst(qtn, Vec::new(), fields, ty)
    }
    pub fn class_inst(qtn: QualifiedTypeName, args: Vec<Ty>, fields: Vec<Pat>, ty: Ty) -> Self {
        Self {
            ctor: Ctor::Class(qtn, args),
            fields,
            ty,
        }
    }
    pub fn interface(iface_ty: Ty, fields: Vec<Pat>, ty: Ty) -> Self {
        Self {
            ctor: Ctor::Interface(iface_ty),
            fields,
            ty,
        }
    }
    pub fn slice(shape: SliceShape, fields: Vec<Pat>, ty: Ty) -> Self {
        debug_assert_eq!(shape.arity(), fields.len());
        Self {
            ctor: Ctor::Slice(shape),
            fields,
            ty,
        }
    }
    /// Or-pattern: `pat1 | pat2 | ...`. Alternatives are stored in `fields`
    /// and must each have type `ty`. At least 2 alternatives required by
    /// convention (one alt is just that alt directly).
    pub fn or(alts: Vec<Pat>, ty: Ty) -> Self {
        debug_assert!(alts.len() >= 2, "Or pattern needs ≥2 alternatives");
        Self {
            ctor: Ctor::Or,
            fields: alts,
            ty,
        }
    }
    /// Union-member tag: marks the pattern as targeting one specific
    /// member of a union scrutinee. The single sub-pat is the
    /// pattern as it would have been against the member type directly.
    pub fn union_member(member_ty: Ty, inner: Pat, scrut_ty: Ty) -> Self {
        Self {
            ctor: Ctor::UnionMember(member_ty),
            fields: vec![inner],
            ty: scrut_ty,
        }
    }
}

impl fmt::Display for Pat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.ctor {
            Ctor::Wildcard | Ctor::NonExhaustive | Ctor::Missing => write!(f, "_"),
            // Or never appears in witnesses (apply for Or is a no-op).
            // Render defensively as `_` if ever produced.
            Ctor::Or => write!(f, "_"),
            // UnionMember is a "which branch" tag. When the inner pat
            // carries a concrete witness (literal, enum variant, class
            // ctor, etc.), render that — it's the most informative form.
            // When the inner collapses to a placeholder (`_` from
            // Wildcard / NonExhaustive / Missing), render the member
            // type name instead so diagnostics like
            // `non-exhaustive match; missing: Mixed { value: int }` say
            // `int` rather than `_`.
            Ctor::UnionMember(member_ty) => match self.fields.first() {
                Some(inner)
                    if !matches!(
                        inner.ctor,
                        Ctor::Wildcard | Ctor::NonExhaustive | Ctor::Missing
                    ) =>
                {
                    write!(f, "{inner}")
                }
                _ => write_member_ty_witness(f, member_ty),
            },
            Ctor::Single(ty) => write_single_witness(f, ty),
            Ctor::Class(qtn, _) => write_braced(f, qtn, &self.fields),
            Ctor::Interface(ty) => write_braced(f, ty, &self.fields),
            Ctor::Slice(shape) => {
                write!(f, "[")?;
                match shape {
                    SliceShape::Fixed(_) => {
                        for (i, fld) in self.fields.iter().enumerate() {
                            if i > 0 {
                                write!(f, ", ")?;
                            }
                            write!(f, "{fld}")?;
                        }
                    }
                    SliceShape::Variable { prefix, suffix: _ } => {
                        for (i, fld) in self.fields.iter().enumerate() {
                            if i > 0 {
                                write!(f, ", ")?;
                            }
                            if i == *prefix {
                                write!(f, "..")?;
                                if !self.fields[i..].is_empty() {
                                    write!(f, ", ")?;
                                }
                            }
                            write!(f, "{fld}")?;
                        }
                        if self.fields.len() == *prefix {
                            // Trailing `..` (suffix is 0). Separate from
                            // any rendered prefix fields with a comma.
                            if !self.fields.is_empty() {
                                write!(f, ", ")?;
                            }
                            write!(f, "..")?;
                        }
                    }
                }
                write!(f, "]")
            }
        }
    }
}

/// Render `Header {}` (no fields) or `Header { f0, f1, ... }` — shared by the
/// `Class` and `Interface` witness arms, which differ only in the header.
fn write_braced(
    f: &mut fmt::Formatter<'_>,
    head: &dyn fmt::Display,
    fields: &[Pat],
) -> fmt::Result {
    if fields.is_empty() {
        return write!(f, "{head} {{}}");
    }
    write!(f, "{head} {{ ")?;
    for (i, fld) in fields.iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{fld}")?;
    }
    write!(f, " }}")
}

fn write_single_witness(f: &mut fmt::Formatter<'_>, ty: &Ty) -> fmt::Result {
    match ty {
        Ty::Literal(lit, _) => match lit {
            Literal::Int(v) => write!(f, "{v}"),
            Literal::Bigint(v) => write!(f, "{v}n"),
            Literal::Bool(v) => write!(f, "{v}"),
            Literal::String(v) => write!(f, "{v:?}"),
            Literal::Float(s) => write!(f, "{s}"),
        },
        Ty::EnumVariant(qtn, variant) => write!(f, "{qtn}.{variant}"),
        Ty::Primitive(PrimitiveType::Null) => write!(f, "null"),
        _ => write!(f, "{ty:?}"),
    }
}

/// Render a `UnionMember` witness's member type when the inner pat is a
/// placeholder (no concrete value to print). Surfaces the member's runtime
/// shape — `int`, `string`, `Foo`, etc. — rather than `_`.
fn write_member_ty_witness(f: &mut fmt::Formatter<'_>, ty: &Ty) -> fmt::Result {
    match ty {
        Ty::Primitive(p) => write!(f, "{p}"),
        Ty::Class(qtn, _) | Ty::Enum(qtn) => write!(f, "{qtn}"),
        Ty::EnumVariant(qtn, variant) => write!(f, "{qtn}.{variant}"),
        Ty::Literal(_, _) => write_single_witness(f, ty),
        _ => write!(f, "_"),
    }
}

// ── Type-system interface ────────────────────────────────────────────────────

/// Operations the algorithm needs from the type system. The TIR builder
/// implements this. Keeping the algorithm trait-bound lets it stay in this
/// module without depending on builder internals.
pub trait PatCtx {
    /// Enumerate the ctors that inhabit `ty`. This is the merge point with
    /// "literals are types": for finite-alphabet types we list every singleton
    /// (`Single(EnumVariant)` per variant, `Single(Bool(true))` and
    /// `Single(Bool(false))` for `bool`, etc.). For infinite alphabets (raw
    /// `int`/`string`/`float`) and opaque types (generics, maps, functions) we
    /// return `[NonExhaustive]`. For `Never`, the empty list (vacuously
    /// exhaustive). Type aliases are expanded.
    fn enumerate_ctors(&self, ty: &Ty) -> Vec<Ctor>;

    /// For a class ctor applied at column type `ty` (which may carry generic
    /// type arguments), return the ordered field types after substitution. The
    /// `Vec` length is the class's field count.
    fn class_field_types(&self, qtn: &QualifiedTypeName, ty: &Ty) -> Vec<Ty>;

    /// For an interface ctor, return the ordered field-view types after
    /// substitution. Test contexts that do not model interfaces can use the
    /// empty default.
    fn interface_field_types(&self, _ty: &Ty) -> Vec<Ty> {
        Vec::new()
    }

    /// When an interface pattern row is specialized through an implementing
    /// class ctor, map each interface field slot to the class field slot that
    /// supplies it. Test contexts that do not model interfaces can decline.
    fn interface_field_projection_for_class(
        &self,
        _iface_ty: &Ty,
        _class_qtn: &QualifiedTypeName,
        _class_type_args: &[Ty],
    ) -> Option<Vec<usize>> {
        None
    }

    /// The element type of an array/list type. Used to derive a slice ctor's
    /// sub-pattern types (the element type repeated `arity` times).
    fn list_element_type(&self, ty: &Ty) -> Ty;

    /// Is this type inhabited (does it admit at least one value)?
    ///
    /// Default impl: structurally walks the type using `class_field_types`
    /// for class definitions, with cycle protection (recursive types like
    /// `class Node { next: Optional<Node> }` are inhabited via the null
    /// branch; direct cycles default to *inhabited*, since uninhabitedness
    /// is anti-monotone).
    ///
    /// Real implementations should override with a Salsa-cached query — the
    /// algorithm calls this on every column and every missing-ctor field
    /// during exhaustiveness, so caching pays off quickly.
    fn is_inhabited(&self, ty: &Ty) -> bool {
        is_inhabited_default(ty, self, &mut FxHashSet::default())
    }
}

fn is_inhabited_default<C: PatCtx + ?Sized>(
    ty: &Ty,
    cx: &C,
    seen: &mut FxHashSet<QualifiedTypeName>,
) -> bool {
    match ty {
        Ty::Never => false,
        Ty::Class(qtn, _) => {
            if !seen.insert(qtn.clone()) {
                // Cycle: assume inhabited. Uninhabitedness is only
                // *proven* by reaching a Never; we never assume it.
                return true;
            }
            let r = cx
                .class_field_types(qtn, ty)
                .iter()
                .all(|f| is_inhabited_default(f, cx, seen));
            seen.remove(qtn);
            r
        }
        Ty::Union(members) => members.iter().any(|m| is_inhabited_default(m, cx, seen)),
        // Everything else is inhabited: e.g. `T?` always inhabits null and
        // `T[]` always inhabits `[]`, regardless of T.
        _ => true,
    }
}

// ── Matrix and rows ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Row<'p> {
    pats: Vec<&'p Pat>,
    /// Source-arm index (for unreachable-arm reporting). Multiple rows can
    /// share an arm idx if the arm contains an or-pattern that was expanded.
    arm: ArmId,
    /// Has any later ctor-witness exposed this row as useful?
    useful: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ArmId(pub usize);

#[derive(Debug, Clone)]
struct Matrix<'p> {
    rows: Vec<Row<'p>>,
    /// Column types. Length = width of the matrix. As we specialize on a
    /// column-0 ctor, this shifts — column 0's sub-pattern types take its
    /// place.
    col_tys: Vec<Ty>,
}

impl<'p> Matrix<'p> {
    fn new(arms: &'p [Pat], scrut_ty: Ty) -> Self {
        let rows = arms
            .iter()
            .enumerate()
            .map(|(i, p)| Row {
                pats: vec![p],
                arm: ArmId(i),
                useful: false,
            })
            .collect();
        Self {
            rows,
            col_tys: vec![scrut_ty],
        }
    }

    fn col_count(&self) -> usize {
        self.col_tys.len()
    }
    fn first_col_ty(&self) -> &Ty {
        &self.col_tys[0]
    }

    /// Specialize: keep only rows whose column-0 ctor is covered by `ctor`,
    /// projecting their sub-patterns into the leading columns. Wildcards in
    /// column 0 are expanded to wildcards in each new sub-column.
    ///
    /// Special case: when `ctor` is `Or`, expand each row whose head is `Or`
    /// into one row per alternative (sharing source `ArmId`), and pass
    /// non-Or rows through unchanged. Column count stays the same. This
    /// matches rustc's approach — Or-pattern alternatives become independent
    /// rows, but their `useful` flags aggregate at the source-arm level
    /// because they share an `ArmId`.
    fn specialize<'a>(
        &self,
        cx: &dyn PatCtx,
        ctor: &Ctor,
        sub_tys: &[Ty],
        wild_pad: &'a [Pat],
    ) -> Matrix<'a>
    where
        'p: 'a,
    {
        if matches!(ctor, Ctor::Or) {
            let mut new_rows = Vec::new();
            for row in &self.rows {
                let head = row.pats[0];
                let tail = &row.pats[1..];
                match &head.ctor {
                    Ctor::Or => {
                        for alt in &head.fields {
                            let mut pats: Vec<&Pat> = vec![alt];
                            pats.extend_from_slice(tail);
                            new_rows.push(Row {
                                pats,
                                arm: row.arm,
                                useful: row.useful,
                            });
                        }
                    }
                    _ => {
                        new_rows.push(Row {
                            pats: row.pats.clone(),
                            arm: row.arm,
                            useful: row.useful,
                        });
                    }
                }
            }
            return Matrix {
                rows: new_rows,
                col_tys: self.col_tys.clone(),
            };
        }

        let arity = sub_tys.len();
        let mut new_rows = Vec::with_capacity(self.rows.len());
        for row in &self.rows {
            let head = row.pats[0];
            let tail = &row.pats[1..];
            match &head.ctor {
                Ctor::Wildcard => {
                    let mut pats: Vec<&Pat> = (0..arity).map(|i| &wild_pad[i]).collect();
                    pats.extend_from_slice(tail);
                    new_rows.push(Row {
                        pats,
                        arm: row.arm,
                        useful: row.useful,
                    });
                }
                _ => {
                    let interface_projection = match (&head.ctor, ctor) {
                        (Ctor::Interface(iface_ty), Ctor::Class(class_qtn, class_args)) => {
                            cx.interface_field_projection_for_class(iface_ty, class_qtn, class_args)
                        }
                        _ => None,
                    };
                    if !head.ctor.covers(ctor) && interface_projection.is_none() {
                        // Row's pattern doesn't accept any value of shape
                        // `ctor`. Skip — this row can't contribute to this
                        // specialization.
                        continue;
                    }

                    // Project the row's fields into the `arity` slots.
                    // Default to wildcards; then place the head's actual
                    // fields at their correct positions.
                    //
                    // For a `Variable{prefix, suffix}` slice projecting onto
                    // a wider arity (Fixed(N) with N > prefix+suffix, or
                    // a wider Variable), prefix fields stay at indices
                    // 0..prefix; suffix fields shift to the rightmost
                    // positions: arity-suffix..arity. The middle gets
                    // wildcards. Without this shift, suffix patterns would
                    // misalign — e.g., `[..rest, true]` would (wrongly)
                    // cover `[true, false]`.
                    let mut pats: Vec<&Pat> = (0..arity).map(|i| &wild_pad[i]).collect();
                    let head_arity = head.fields.len();
                    match interface_projection {
                        Some(projection) => {
                            for (interface_idx, class_idx) in projection.into_iter().enumerate() {
                                if let Some(fld) = head.fields.get(interface_idx)
                                    && class_idx < arity
                                {
                                    pats[class_idx] = fld;
                                }
                            }
                        }
                        None => match &head.ctor {
                            Ctor::Slice(SliceShape::Variable { prefix, suffix })
                                if head_arity != arity =>
                            {
                                for (i, fld) in head.fields.iter().enumerate() {
                                    let new_idx = if i < *prefix {
                                        i
                                    } else {
                                        // Suffix slot j (counted from end of
                                        // head's suffix): j = i - prefix from
                                        // start; rightmost position is
                                        // arity - suffix + j.
                                        debug_assert!(i >= *prefix && i < prefix + suffix);
                                        i + arity - head_arity
                                    };
                                    pats[new_idx] = fld;
                                }
                            }
                            _ => {
                                for (i, fld) in head.fields.iter().enumerate() {
                                    if i < arity {
                                        pats[i] = fld;
                                    }
                                }
                            }
                        },
                    }
                    pats.extend_from_slice(tail);
                    new_rows.push(Row {
                        pats,
                        arm: row.arm,
                        useful: row.useful,
                    });
                }
            }
        }
        let mut col_tys = sub_tys.to_vec();
        col_tys.extend_from_slice(&self.col_tys[1..]);
        Matrix {
            rows: new_rows,
            col_tys,
        }
    }
}

// ── Algorithm ────────────────────────────────────────────────────────────────

/// Top-level result of running usefulness on a match.
#[derive(Debug, Clone)]
pub struct UsefulnessReport {
    /// One witness per missing case at the top level. Empty = exhaustive.
    pub missing: Vec<Pat>,
    /// Indices of arms that are unreachable (no value matches them that wasn't
    /// already matched by an earlier arm).
    pub unreachable_arms: Vec<ArmId>,
}

/// Compute exhaustiveness and per-arm reachability.
///
/// `arms` is one `Pat` per source arm. Or-patterns are represented as
/// `Ctor::Or` nodes and expanded by the algorithm during specialization;
/// usefulness aggregates back to the original source arm via shared
/// `ArmId`, so an or-pattern arm is "useful" if any of its alternatives is.
pub fn compute_match_usefulness(cx: &dyn PatCtx, arms: &[Pat], scrut_ty: Ty) -> UsefulnessReport {
    // Uninhabited scrutinee: no values to match. The match is vacuously
    // exhaustive and every arm is unreachable. Mirrors rustc's notion of
    // "the place being matched is irrelevant" when no inhabitant exists.
    if !cx.is_inhabited(&scrut_ty) {
        return UsefulnessReport {
            missing: vec![],
            unreachable_arms: (0..arms.len()).map(ArmId).collect(),
        };
    }

    let mut matrix = Matrix::new(arms, scrut_ty);
    let mut witness_matrix = WitnessMatrix::empty();
    compute_exhaustiveness(cx, &mut matrix, &mut witness_matrix);

    let used: FxHashSet<ArmId> = matrix
        .rows
        .iter()
        .filter(|r| r.useful)
        .map(|r| r.arm)
        .collect();
    let mut unreachable_arms: Vec<ArmId> = (0..arms.len())
        .map(ArmId)
        .filter(|a| !used.contains(a))
        .collect();
    unreachable_arms.sort();
    unreachable_arms.dedup();

    UsefulnessReport {
        missing: witness_matrix.into_single_column(),
        unreachable_arms,
    }
}

/// Single-pattern irrefutability check. Returns `Ok(())` if the pattern covers
/// every value of `ty`; otherwise returns a witness value the pattern doesn't
/// match.
pub fn check_irrefutable(cx: &dyn PatCtx, pat: &Pat, ty: Ty) -> Result<(), Box<Pat>> {
    let report = compute_match_usefulness(cx, std::slice::from_ref(pat), ty);
    match report.missing.into_iter().next() {
        None => Ok(()),
        Some(w) => Err(Box::new(w)),
    }
}

/// A column-stack of witness patterns, one entry per remaining matrix column.
/// Built up during recursion as ctors are unspecialized back onto the witness.
#[derive(Debug, Clone)]
struct WitnessStack(Vec<Pat>);

impl WitnessStack {
    /// Wrap the top `arity` patterns under `ctor`, replacing them with the
    /// resulting wrapped pattern. Reverses one specialize step.
    ///
    /// The stack holds patterns in reverse-of-recursion-unwind order: the
    /// last pushed is the most recently unwound column, which corresponds
    /// to the *first* sub-position (col 0 of the inner matrix = first
    /// `sub_ty` of the outer ctor). So we reverse the drain to recover
    /// declaration order in the wrapped pat.
    fn apply_ctor(mut self, ctor: &Ctor, arity: usize, ty: &Ty) -> Self {
        let len = self.0.len();
        let fields: Vec<Pat> = self.0.drain((len - arity)..).rev().collect();
        self.0.push(Pat::new(ctor.clone(), fields, ty.clone()));
        self
    }
    /// Push a fresh pattern on top (used to introduce wildcards when applying
    /// the synthetic `Missing` ctor).
    fn push(&mut self, pat: Pat) {
        self.0.push(pat);
    }
}

/// A collection of witness stacks, one per missing-case witness.
#[derive(Debug, Clone)]
struct WitnessMatrix(Vec<WitnessStack>);

impl WitnessMatrix {
    fn empty() -> Self {
        WitnessMatrix(Vec::new())
    }
    /// One witness with no columns. Used at the recursion leaf when the
    /// matrix has 0 rows: a value reached this point and no row covered it.
    fn unit() -> Self {
        WitnessMatrix(vec![WitnessStack(Vec::new())])
    }
    fn extend(&mut self, other: WitnessMatrix) {
        self.0.extend(other.0);
    }
    /// Apply `ctor` to every witness, reversing one specialize step.
    fn apply_ctor(&mut self, ctor: &Ctor, arity: usize, ty: &Ty) {
        let stacks = std::mem::take(&mut self.0);
        self.0 = stacks
            .into_iter()
            .map(|s| s.apply_ctor(ctor, arity, ty))
            .collect();
    }
    /// For each missing ctor, clone the witness matrix and prepend a
    /// wildcard-filled pat for that ctor at top. Used for the synthetic
    /// `Missing` ctor case.
    ///
    /// Skips ctors whose value-set is empty: if any sub-field type is
    /// uninhabited (`Ty::Never`), no value can match this ctor, so it is
    /// not a real "missing case" — e.g., `Array<Never>` cannot inhabit
    /// length ≥ 1, so the `[Never, ..]` witness must not be reported.
    fn apply_missing(&mut self, missing: &[Ctor], ty: &Ty, cx: &dyn PatCtx) {
        let original = std::mem::take(&mut self.0);
        for ctor in missing {
            // Sub-pattern types come from the ctor's own slot types, NOT
            // the parent column type. For `Class(Node)`, fields are typed
            // by the class's declared field types.
            let field_tys = ctor_sub_tys(cx, ctor, ty);
            if field_tys.iter().any(|t| !cx.is_inhabited(t)) {
                continue;
            }
            let fields: Vec<Pat> = field_tys.into_iter().map(Pat::wildcard).collect();
            let pat = Pat::new(ctor.clone(), fields, ty.clone());
            for stack in &original {
                let mut new_stack = stack.clone();
                new_stack.push(pat.clone());
                self.0.push(new_stack);
            }
        }
    }
    fn into_single_column(self) -> Vec<Pat> {
        self.0
            .into_iter()
            .map(|mut s| {
                debug_assert_eq!(s.0.len(), 1);
                s.0.pop().unwrap()
            })
            .collect()
    }
}

/// The core recursion. Mutates `matrix` (marking rows useful) and `witnesses`
/// (collecting missing-case witnesses).
///
/// The empty-matrix and all-wildcard shortcuts in `split_ctors` ensure
/// recursion terminates on recursive types without an explicit depth guard.
fn compute_exhaustiveness(cx: &dyn PatCtx, matrix: &mut Matrix<'_>, witnesses: &mut WitnessMatrix) {
    if matrix.col_count() == 0 {
        if matrix.rows.is_empty() {
            // Reachable leaf with no covering row: emit a unit witness that
            // will be wrapped by the unspecialize chain on the way out.
            witnesses.extend(WitnessMatrix::unit());
        } else {
            // First row covers this leaf; later rows are unreachable.
            matrix.rows[0].useful = true;
        }
        return;
    }

    let col_ty = matrix.first_col_ty().clone();
    let (split, missing_in_matrix) = split_ctors(cx, &col_ty, matrix);

    for ctor in &split {
        let sub_tys = ctor_sub_tys(cx, ctor, &col_ty);
        let wild_pad: Vec<Pat> = sub_tys.iter().cloned().map(Pat::wildcard).collect();

        let mut sub_matrix = matrix.specialize(cx, ctor, &sub_tys, &wild_pad);
        let mut sub_witnesses = WitnessMatrix::empty();
        compute_exhaustiveness(cx, &mut sub_matrix, &mut sub_witnesses);

        // Propagate "useful" flag back from sub-matrix rows to parent rows.
        for sub_row in &sub_matrix.rows {
            if sub_row.useful {
                for parent_row in &mut matrix.rows {
                    if parent_row.arm == sub_row.arm {
                        parent_row.useful = true;
                    }
                }
            }
        }

        // Unspecialize: wrap sub-witnesses under this ctor.
        if matches!(ctor, Ctor::Missing) {
            sub_witnesses.apply_missing(&missing_in_matrix, &col_ty, cx);
        } else if matches!(ctor, Ctor::Or) {
            // Or-specialization didn't change column count; witnesses for
            // the expanded sub-matrix are already shaped for *this* level.
            // The original or-pattern doesn't contribute its own ctor to
            // the witness — alternatives stand on their own.
        } else {
            let arity = sub_tys.len();
            sub_witnesses.apply_ctor(ctor, arity, &col_ty);
        }
        witnesses.extend(sub_witnesses);
    }
}

/// Whether `ty` is a list-shaped type (`List` or `EvolvingList`), which the
/// usefulness algorithm handles via slice splitting rather than ctor enumeration.
fn is_list_ty(ty: &Ty) -> bool {
    matches!(ty, Ty::List(_) | Ty::EvolvingList(_))
}

/// Decide which ctors to specialize on. Returns `(split, missing_from_matrix)`:
///
/// - `split` is the list of ctors to recurse with. Every ctor present in the
///   matrix appears here. A synthetic `Missing` ctor stands in for ctors that
///   are missing from the matrix (when no wildcard arm is present), so the
///   recursion through it produces a missing-case witness for each.
/// - `missing_from_matrix` is the actual list of ctors that are missing —
///   used to expand the `Missing` ctor when applying it.
fn split_ctors(cx: &dyn PatCtx, col_ty: &Ty, matrix: &Matrix<'_>) -> (Vec<Ctor>, Vec<Ctor>) {
    // If any row has an Or-pattern at column 0, force expansion via the
    // `Or` ctor before any other split logic. Or-rows can't participate in
    // a normal `head.ctor.covers(ctor)` check; they need to explode first.
    if matrix
        .rows
        .iter()
        .any(|row| matches!(&row.pats[0].ctor, Ctor::Or))
    {
        return (vec![Ctor::Or], vec![]);
    }

    // Empty matrix: nothing matched here. Don't enumerate ctors of
    // `col_ty` — that would recurse forever on recursive types like
    // `class Node { next: Optional<Node> }`. Instead, emit a single
    // synthetic `Missing` ctor that drops the column without descending.
    // The unspecialize step (`apply_missing`) wraps the witness with one
    // wildcard-filled pat per missing ctor of `col_ty`.
    if matrix.rows.is_empty() {
        // Empty matrix on an uninhabited column type → vacuously exhaustive.
        // Catches transitively-uninhabited classes (e.g. `class A { x: Never }`
        // or `class A { x: B }` where `B` is uninhabited) that
        // `enumerate_ctors` alone can't detect (it returns `[Class(A)]`).
        if !cx.is_inhabited(col_ty) {
            return (vec![], vec![]);
        }
        // Lists: a single open-ended `[..]` witness covers "any list".
        // Check this *before* `enumerate_ctors` because List's `enumerate`
        // returns empty (slice splitting normally handles it). Without
        // this short-circuit, the empty result would incorrectly mark the
        // list as vacuously exhaustive.
        if is_list_ty(col_ty) {
            return (
                vec![Ctor::Missing],
                vec![Ctor::Slice(SliceShape::Variable {
                    prefix: 0,
                    suffix: 0,
                })],
            );
        }
        let all = cx.enumerate_ctors(col_ty);
        if all.is_empty() {
            // Vacuously exhaustive (e.g. `Ty::Never` directly).
            return (vec![], vec![]);
        }
        let missing = if all.iter().any(|c| matches!(c, Ctor::NonExhaustive)) {
            vec![Ctor::NonExhaustive]
        } else {
            dedup_ctors(all)
        };
        return (vec![Ctor::Missing], missing);
    }

    let present: Vec<Ctor> = collect_column_ctors(matrix);
    let has_wildcard = present.iter().any(|c| matches!(c, Ctor::Wildcard));
    let present_no_wild: Vec<Ctor> = present
        .into_iter()
        .filter(|c| !matches!(c, Ctor::Wildcard))
        .collect();

    // Slice types need a special split that treats variable-length patterns
    // as covering open-ended length classes — set membership isn't enough.
    if is_list_ty(col_ty) {
        return split_slice_ctors(&present_no_wild);
    }

    let all = cx.enumerate_ctors(col_ty);

    if all.iter().any(|c| matches!(c, Ctor::NonExhaustive)) {
        if matches!(col_ty, Ty::Interface(..))
            && present_no_wild
                .iter()
                .any(|c| matches!(c, Ctor::Interface(_)))
        {
            return (present_no_wild, vec![]);
        }
        // Infinite alphabet (raw int/string/float, generics, opaque types).
        let mut split: Vec<Ctor> = present_no_wild;
        let missing = vec![Ctor::NonExhaustive];
        if has_wildcard {
            // Wildcard rows must be specialized through. NonExhaustive as a
            // pass-through ctor: concrete heads don't cover it (they get
            // skipped), but wildcard rows pad and take.
            split.push(Ctor::NonExhaustive);
        } else {
            split.push(Ctor::Missing);
        }
        (split, missing)
    } else if all.is_empty() {
        // Vacuously exhaustive (e.g., Never).
        (vec![], vec![])
    } else {
        // Finite alphabet of pairwise-disjoint ctors (singletons, classes).
        if present_no_wild.is_empty() && has_wildcard {
            // All rows are pure wildcards at this column. Don't iterate
            // ctors — that would recurse forever on recursive types
            // (`class Node { next: Optional<Node> }`). Pass through with a
            // synthetic ctor that drops the column.
            return (vec![Ctor::NonExhaustive], vec![]);
        }
        // Iterate over *all* ctors of the type. Wildcard rows extend to
        // each; missing ctors recurse into an empty sub-matrix and surface
        // as witnesses naturally. Dedup `all` first — `enumerate_ctors`
        // can produce duplicates (e.g. `Optional<Optional<T>>` pushes a
        // `Single(null)` per Optional layer).
        let all = dedup_ctors(all);
        let present_set: FxHashSet<Ctor> = present_no_wild.iter().cloned().collect();
        let missing: Vec<Ctor> = all
            .iter()
            .filter(|c| !present_set.contains(c))
            .cloned()
            .collect();
        (all, missing)
    }
}

/// Slice-specific split. Mirrors rustc's `Slice::split`: given the slice
/// patterns present in the column, partition the universe of array lengths
/// into a finite set of `Fixed` lengths plus one open-ended `Variable` that
/// covers all longer lengths. Each output slice is tagged as seen/unseen by
/// the column.
fn split_slice_ctors(present: &[Ctor]) -> (Vec<Ctor>, Vec<Ctor>) {
    let column: Vec<&SliceShape> = present
        .iter()
        .filter_map(|c| {
            if let Ctor::Slice(s) = c {
                Some(s)
            } else {
                None
            }
        })
        .collect();

    // Stats over the column.
    let mut min_var_arity: Option<usize> = None;
    let mut max_var_prefix: usize = 0;
    let mut max_var_suffix: usize = 0;
    let mut max_fixed_plus_one: usize = 1;
    let mut seen_fixed: FxHashSet<usize> = FxHashSet::default();
    for s in &column {
        match s {
            SliceShape::Fixed(n) => {
                max_fixed_plus_one = max_fixed_plus_one.max(*n + 1);
                seen_fixed.insert(*n);
            }
            SliceShape::Variable { prefix, suffix } => {
                max_var_prefix = max_var_prefix.max(*prefix);
                max_var_suffix = max_var_suffix.max(*suffix);
                let arity = prefix + suffix;
                min_var_arity = Some(min_var_arity.map_or(arity, |m| m.min(arity)));
            }
        }
    }

    // Build the "max_slice" — the open-ended Variable covering the tail.
    // Its arity must exceed every fixed-length seen, so prefix grows to
    // accommodate.
    let mut max_prefix = max_var_prefix;
    let max_suffix = max_var_suffix;
    if max_prefix + max_suffix < max_fixed_plus_one {
        max_prefix = max_fixed_plus_one.saturating_sub(max_suffix);
    }
    let max_arity = max_prefix + max_suffix;

    // Output: only push *seen* lengths into `split` directly; lengths that
    // aren't seen go into `missing` and are surfaced via the synthetic
    // `Ctor::Missing` (whose `apply_missing` step wraps each missing ctor
    // with one witness). Pushing a length into both `split` AND `missing`
    // would produce duplicate witnesses — once via direct specialization
    // through the unseen ctor (sub-matrix empty → unit witness wrapped),
    // and once via `Missing → apply_missing`.
    let mut split: Vec<Ctor> = Vec::new();
    let mut missing: Vec<Ctor> = Vec::new();

    for n in 0..max_arity {
        let seen = seen_fixed.contains(&n) || min_var_arity.is_some_and(|m| m <= n);
        let c = Ctor::Slice(SliceShape::Fixed(n));
        if seen {
            split.push(c);
        } else {
            missing.push(c);
        }
    }

    let tail = Ctor::Slice(SliceShape::Variable {
        prefix: max_prefix,
        suffix: max_suffix,
    });
    let tail_seen = min_var_arity.is_some_and(|m| m <= max_arity);
    if tail_seen {
        split.push(tail);
    } else {
        missing.push(tail);
    }

    if !missing.is_empty() {
        split.push(Ctor::Missing);
    }
    (split, missing)
}

fn collect_column_ctors(matrix: &Matrix<'_>) -> Vec<Ctor> {
    let mut seen: FxHashSet<Ctor> = FxHashSet::default();
    let mut out = Vec::new();
    let mut has_wildcard = false;
    for row in &matrix.rows {
        let head = row.pats[0];
        match &head.ctor {
            Ctor::Wildcard => has_wildcard = true,
            other => {
                if seen.insert(other.clone()) {
                    out.push(other.clone());
                }
            }
        }
    }
    if has_wildcard {
        out.push(Ctor::Wildcard);
    }
    out
}

/// Stable de-duplication of a ctor list. Order-preserving: keeps the first
/// occurrence of each ctor. `enumerate_ctors` for nested types (e.g.
/// `Optional<Optional<T>>`) can emit the same `Single(null)` multiple times;
/// without dedup, missing-case witnesses get duplicated.
fn dedup_ctors(ctors: Vec<Ctor>) -> Vec<Ctor> {
    let mut seen: FxHashSet<Ctor> = FxHashSet::default();
    ctors
        .into_iter()
        .filter(|c| seen.insert(c.clone()))
        .collect()
}

fn ctor_sub_tys(cx: &dyn PatCtx, ctor: &Ctor, col_ty: &Ty) -> Vec<Ty> {
    match ctor {
        Ctor::Class(qtn, args) => cx.class_field_types(qtn, &class_ty_for_ctor(qtn, args, col_ty)),
        Ctor::Interface(iface_ty) => cx.interface_field_types(iface_ty),
        // A slice's sub-pattern types are just the element type repeated
        // `arity` times.
        Ctor::Slice(shape) => vec![cx.list_element_type(col_ty); shape.arity()],
        // UnionMember projects a column from the union type down to the
        // member type. Specialise recurses with that single sub-column.
        Ctor::UnionMember(member_ty) => vec![member_ty.clone()],
        _ => vec![],
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(
        clippy::default_trait_access,
        clippy::doc_markdown,
        clippy::items_after_statements,
        clippy::cloned_ref_to_slice_refs,
        clippy::many_single_char_names,
        clippy::redundant_clone,
        clippy::redundant_closure_for_method_calls,
        clippy::single_char_pattern,
        clippy::uninlined_format_args,
        clippy::unnested_or_patterns
    )]

    use baml_base::Name;

    use super::*;
    use crate::test_support::*;

    #[test]
    fn bool_exhaustive_with_both_arms() {
        let arms = vec![
            Pat::single(bool_lit(true), bool_ty()),
            Pat::single(bool_lit(false), bool_ty()),
        ];
        let report = compute_match_usefulness(&TestingCtx::new(), &arms, bool_ty());
        assert!(
            report.missing.is_empty(),
            "expected exhaustive: {:?}",
            report.missing
        );
        assert!(report.unreachable_arms.is_empty());
    }

    #[test]
    fn bool_non_exhaustive_missing_false() {
        let arms = vec![Pat::single(bool_lit(true), bool_ty())];
        let report = compute_match_usefulness(&TestingCtx::new(), &arms, bool_ty());
        assert_eq!(report.missing.len(), 1);
        assert!(matches!(report.missing[0].ctor, Ctor::Single(_)));
    }

    #[test]
    fn bool_wildcard_makes_exhaustive() {
        let arms = vec![
            Pat::single(bool_lit(true), bool_ty()),
            Pat::wildcard(bool_ty()),
        ];
        let report = compute_match_usefulness(&TestingCtx::new(), &arms, bool_ty());
        assert!(report.missing.is_empty());
    }

    #[test]
    fn int_requires_wildcard() {
        let arms = vec![Pat::single(int_lit(1), int_ty())];
        let report = compute_match_usefulness(&TestingCtx::new(), &arms, int_ty());
        assert_eq!(report.missing.len(), 1);
    }

    #[test]
    fn int_with_wildcard_is_exhaustive() {
        let arms = vec![Pat::single(int_lit(1), int_ty()), Pat::wildcard(int_ty())];
        let report = compute_match_usefulness(&TestingCtx::new(), &arms, int_ty());
        assert!(report.missing.is_empty());
    }

    #[test]
    fn unreachable_after_wildcard() {
        let arms = vec![
            Pat::wildcard(bool_ty()),
            Pat::single(bool_lit(true), bool_ty()),
        ];
        let report = compute_match_usefulness(&TestingCtx::new(), &arms, bool_ty());
        assert!(report.missing.is_empty());
        assert!(
            report.unreachable_arms.contains(&ArmId(1)),
            "second arm should be unreachable, got {:?}",
            report.unreachable_arms
        );
    }

    #[test]
    fn typevar_requires_wildcard() {
        let tv = Ty::TypeVar(Name::new("T"));
        let arms = vec![Pat::wildcard(tv.clone())];
        let report = compute_match_usefulness(&TestingCtx::new(), &arms, tv);
        assert!(report.missing.is_empty());
    }

    #[test]
    fn never_is_vacuously_exhaustive() {
        let never = Ty::Never;
        let arms: Vec<Pat> = vec![];
        let report = compute_match_usefulness(&TestingCtx::new(), &arms, never);
        assert!(report.missing.is_empty());
    }

    #[test]
    fn float_canonicalization() {
        let a = ty_ctor_identity(&float_lit("1.0"));
        let b = ty_ctor_identity(&float_lit("1.00"));
        let c = ty_ctor_identity(&float_lit("1e0"));
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    /// `Array<bool>` with `[true, _]` and `[false, true]` should report
    /// `[false, false]` as missing.
    #[test]
    fn array_pair_missing_diagonal() {
        let array_bool = Ty::List(Box::new(bool_ty()));

        let arm1 = Pat::slice(
            SliceShape::Fixed(2),
            vec![
                Pat::single(bool_lit(true), bool_ty()),
                Pat::wildcard(bool_ty()),
            ],
            array_bool.clone(),
        );
        let arm2 = Pat::slice(
            SliceShape::Fixed(2),
            vec![
                Pat::single(bool_lit(false), bool_ty()),
                Pat::single(bool_lit(true), bool_ty()),
            ],
            array_bool.clone(),
        );

        let report = compute_match_usefulness(&TestingCtx::new(), &[arm1, arm2], array_bool);
        // Many length-classes are still missing (length 0, 1, 3, variable),
        // but `[false, false]` must be among them.
        let missing = missing_strings(&report);
        assert!(
            missing.iter().any(|s| s.contains("false, false")),
            "expected `[false, false]` in missing, got {:?}",
            missing
        );
    }

    /// `Array<bool>` with `[]`, `[_]`, `[_, _, ..]` should be exhaustive
    /// over all lengths.
    #[test]
    fn array_rest_covers_all_lengths() {
        let array_bool = Ty::List(Box::new(bool_ty()));

        let arms = vec![
            Pat::slice(SliceShape::Fixed(0), vec![], array_bool.clone()),
            Pat::slice(
                SliceShape::Fixed(1),
                vec![Pat::wildcard(bool_ty())],
                array_bool.clone(),
            ),
            Pat::slice(
                SliceShape::Variable {
                    prefix: 2,
                    suffix: 0,
                },
                vec![Pat::wildcard(bool_ty()), Pat::wildcard(bool_ty())],
                array_bool.clone(),
            ),
        ];

        let report = compute_match_usefulness(&TestingCtx::new(), &arms, array_bool);
        assert!(
            report.missing.is_empty(),
            "expected exhaustive, got missing {:?}",
            missing_strings(&report)
        );
    }

    // ── Rustc pattern-analysis ports ───────────────────────────────────

    /// Port of rustc_pattern_analysis `test_nested`.
    ///
    /// Rust shape: `enum E { A(bool), B(bool) }` and scrutinee `(E, E)`.
    /// BAML shape: `type E = A | B`; `class Pair { left E; right E }`.
    #[test]
    fn rustc_port_nested_type_union_pair() {
        let mut cx = TestingCtx::new();
        let a = qtn("A");
        let b = qtn("B");
        let pair = qtn("NestedPair");
        let a_ty = class_ty(&a);
        let b_ty = class_ty(&b);
        let e_ty = union_of(vec![a_ty.clone(), b_ty.clone()]);
        let pair_ty = class_ty(&pair);

        cx.register(a.clone(), vec![bool_ty()]);
        cx.register(b.clone(), vec![bool_ty()]);
        cx.register(pair.clone(), vec![e_ty.clone(), e_ty.clone()]);

        let variant = |q: &QualifiedTypeName| {
            Pat::class(q.clone(), vec![Pat::wildcard(bool_ty())], class_ty(q))
        };
        let mk_pair =
            |left: Pat, right: Pat| Pat::class(pair.clone(), vec![left, right], pair_ty.clone());

        let report = compute_match_usefulness(
            &cx,
            &[mk_pair(variant(&a), Pat::wildcard(e_ty.clone()))],
            pair_ty.clone(),
        );
        assert!(
            !report.missing.is_empty(),
            "A(_) in the left slot must not cover B(_)"
        );

        let report = compute_match_usefulness(
            &cx,
            &[
                mk_pair(variant(&a), Pat::wildcard(e_ty.clone())),
                mk_pair(variant(&b), Pat::wildcard(e_ty.clone())),
            ],
            pair_ty.clone(),
        );
        assert!(
            report.missing.is_empty(),
            "A(_) | B(_) in the left slot covers the whole pair"
        );

        let report = compute_match_usefulness(
            &cx,
            &[
                mk_pair(variant(&a), Pat::wildcard(e_ty.clone())),
                mk_pair(Pat::wildcard(e_ty.clone()), variant(&a)),
            ],
            pair_ty.clone(),
        );
        assert!(
            !report.missing.is_empty(),
            "A(_) in either slot still misses B(_), B(_)"
        );

        let report = compute_match_usefulness(
            &cx,
            &[
                mk_pair(variant(&a), Pat::wildcard(e_ty.clone())),
                mk_pair(Pat::wildcard(e_ty.clone()), variant(&a)),
                mk_pair(variant(&b), variant(&b)),
            ],
            pair_ty,
        );
        assert!(
            report.missing.is_empty(),
            "the explicit B(_), B(_) arm completes coverage"
        );
    }

    /// Port of rustc_pattern_analysis `test_witnesses`.
    ///
    /// Rust uses `(Option<bool>, Option<bool>)`; BAML optional values flatten
    /// to the finite set `true | false | null`, so the same coverage shape is
    /// represented as a two-field class over `Optional<bool>`.
    #[test]
    fn rustc_port_optional_pair_witnesses() {
        let mut cx = TestingCtx::new();
        let pair = qtn("OptionalPair");
        let opt_bool = opt_of(bool_ty());
        let pair_ty = class_ty(&pair);
        cx.register(pair.clone(), vec![opt_bool.clone(), opt_bool.clone()]);

        let mk_pair =
            |left: Pat, right: Pat| Pat::class(pair.clone(), vec![left, right], pair_ty.clone());

        let report = compute_match_usefulness(&cx, &[], pair_ty.clone());
        let witnesses = missing_strings(&report);
        assert_eq!(
            witnesses.len(),
            1,
            "an empty match should produce one wildcard-shaped witness, got {witnesses:?}"
        );
        assert!(
            witnesses[0].contains("OptionalPair") && witnesses[0].matches('_').count() >= 2,
            "expected Pair(_, _) style witness, got {:?}",
            witnesses
        );

        let false_false = mk_pair(
            Pat::single(bool_lit(false), opt_bool.clone()),
            Pat::single(bool_lit(false), opt_bool.clone()),
        );
        let report = compute_match_usefulness(&cx, &[false_false], pair_ty.clone());
        let witnesses = missing_strings(&report);

        assert_eq!(
            witnesses.len(),
            8,
            "all Optional<bool> pairs except false/false should be missing"
        );
        for expected in [
            "user.OptionalPair { true, true }",
            "user.OptionalPair { true, false }",
            "user.OptionalPair { true, null }",
            "user.OptionalPair { false, true }",
            "user.OptionalPair { false, null }",
            "user.OptionalPair { null, true }",
            "user.OptionalPair { null, false }",
            "user.OptionalPair { null, null }",
        ] {
            assert!(
                witnesses.iter().any(|w| w == expected),
                "expected {expected} in witnesses, got {witnesses:?}"
            );
        }

        let any_false = mk_pair(
            Pat::wildcard(opt_bool.clone()),
            Pat::single(bool_lit(false), opt_bool.clone()),
        );
        let report = compute_match_usefulness(&cx, &[any_false], pair_ty);
        let witnesses = missing_strings(&report);
        assert_eq!(
            witnesses.len(),
            2,
            "all pairs with a non-false right side should be missing"
        );
        for expected in [
            "user.OptionalPair { _, true }",
            "user.OptionalPair { _, null }",
        ] {
            assert!(
                witnesses.iter().any(|w| w == expected),
                "expected {expected} in witnesses, got {witnesses:?}"
            );
        }
    }

    /// Port of rustc_pattern_analysis `test_empty`.
    ///
    /// Rust shape: `Result<bool, !>`. BAML shape: `type Result = Ok | Err`
    /// where `Err` has a `Never` payload and is therefore uninhabited.
    #[test]
    fn rustc_port_empty_variant_payloads_are_ignored() {
        let mut cx = TestingCtx::new();
        let ok = qtn("NeverOk");
        let err = qtn("NeverErr");
        let pair = qtn("NeverPair");
        let ok_ty = class_ty(&ok);
        let err_ty = class_ty(&err);
        let result_ty = union_of(vec![ok_ty.clone(), err_ty.clone()]);
        let pair_ty = class_ty(&pair);

        cx.register(ok.clone(), vec![bool_ty()]);
        cx.register(err.clone(), vec![never_ty()]);
        cx.register(pair.clone(), vec![bool_ty(), result_ty.clone()]);

        let ok_any = || Pat::class(ok.clone(), vec![Pat::wildcard(bool_ty())], ok_ty.clone());

        let report = compute_match_usefulness(&cx, &[ok_any()], result_ty.clone());
        assert!(
            report.missing.is_empty(),
            "Ok(_) should exhaust Result<bool, Never>; got {:?}",
            missing_strings(&report)
        );

        let mk_pair = |b: bool| {
            Pat::class(
                pair.clone(),
                vec![Pat::single(bool_lit(b), bool_ty()), ok_any()],
                pair_ty.clone(),
            )
        };
        let report = compute_match_usefulness(&cx, &[mk_pair(true), mk_pair(false)], pair_ty);
        assert!(
            report.missing.is_empty(),
            "covering bool with Ok(_) should exhaust (bool, Result<bool, Never>)"
        );
    }

    /// Port of the finite bool witness check in rustc_pattern_analysis
    /// `test_witnesses`.
    #[test]
    fn rustc_port_bool_empty_match_witnesses() {
        let cx = TestingCtx::new();
        let report = compute_match_usefulness(&cx, &[], bool_ty());
        let witnesses = missing_strings(&report);
        assert_eq!(witnesses, vec!["true", "false"]);
    }

    /// Port of the "large enum" complexity shape: a tagged enum with bool
    /// payloads becomes a type union of classes, and a wildcard after every
    /// variant is unreachable.
    #[test]
    fn rustc_port_large_enum_type_union_with_trailing_wildcard() {
        let mut cx = TestingCtx::new();
        let names = [
            "V00", "V01", "V02", "V03", "V04", "V05", "V06", "V07", "V08", "V09", "V10", "V11",
            "V12", "V13", "V14", "V15", "V16", "V17", "V18", "V19",
        ];
        let variants: Vec<_> = names.iter().map(|name| qtn(name)).collect();
        for variant in &variants {
            cx.register(variant.clone(), vec![bool_ty()]);
        }
        let scrut = union_of(variants.iter().map(class_ty).collect());
        let mut arms: Vec<_> = variants
            .iter()
            .map(|variant| {
                Pat::class(
                    variant.clone(),
                    vec![Pat::wildcard(bool_ty())],
                    class_ty(variant),
                )
            })
            .collect();
        arms.push(Pat::wildcard(scrut.clone()));

        let report = compute_match_usefulness(&cx, &arms, scrut);
        assert!(report.missing.is_empty());
        assert_eq!(report.unreachable_arms, vec![ArmId(20)]);
    }

    /// Regression found while comparing against rustc's slice splitting:
    /// `[_, ..]` misses `[]`, so a following `_` arm is still useful.
    #[test]
    fn rustc_port_slice_prefix_then_wildcard_keeps_short_length_reachable() {
        let cx = TestingCtx::new();
        let scrut = list_of(bool_ty());
        let non_empty = Pat::slice(
            SliceShape::Variable {
                prefix: 1,
                suffix: 0,
            },
            vec![Pat::wildcard(bool_ty())],
            scrut.clone(),
        );
        let wildcard = Pat::wildcard(scrut.clone());

        let report = compute_match_usefulness(&cx, &[non_empty, wildcard], scrut);
        assert!(report.missing.is_empty());
        assert!(
            report.unreachable_arms.is_empty(),
            "the wildcard arm covers the empty-list case"
        );
    }

    // ── 1. Cartesian explosion: 4-bool class ────────────────────────────

    /// `class Quad { a, b, c, d: bool }` — 16 combinations. All present →
    /// exhaustive. Missing one → exact witness reported.
    #[test]
    fn testing_1_cartesian_quad_all_combos() {
        let mut cx = TestingCtx::new();
        let q = qtn("Quad");
        cx.register(q.clone(), vec![bool_ty(), bool_ty(), bool_ty(), bool_ty()]);
        let qty = class_ty(&q);

        let combo = |a, b, c, d| {
            Pat::class(
                q.clone(),
                vec![
                    Pat::single(bool_lit(a), bool_ty()),
                    Pat::single(bool_lit(b), bool_ty()),
                    Pat::single(bool_lit(c), bool_ty()),
                    Pat::single(bool_lit(d), bool_ty()),
                ],
                qty.clone(),
            )
        };

        // All 16.
        let mut arms = Vec::new();
        for i in 0..16 {
            arms.push(combo(i & 1 != 0, i & 2 != 0, i & 4 != 0, i & 8 != 0));
        }
        let report = compute_match_usefulness(&cx, &arms, qty.clone());
        assert!(
            report.missing.is_empty(),
            "expected exhaustive, got {:?}",
            missing_strings(&report)
        );

        // Drop one (e.g. (true,false,true,false)) → exactly that missing.
        let mut arms = Vec::new();
        for i in 0..16 {
            if i == (1 | 4) {
                continue;
            }
            arms.push(combo(i & 1 != 0, i & 2 != 0, i & 4 != 0, i & 8 != 0));
        }
        let report = compute_match_usefulness(&cx, &arms, qty.clone());
        assert_eq!(report.missing.len(), 1, "expected one missing case");
        let w = report.missing[0].to_string();
        assert!(
            w.contains("true") && w.contains("false"),
            "witness should mention both truth values, got {}",
            w
        );
    }

    /// Wildcard mid-pattern expands coverage. `{ a: true, b: _, c: false, d: true }`
    /// covers two cases (b ∈ {true, false}).
    #[test]
    fn testing_1b_wildcard_in_class_field() {
        let mut cx = TestingCtx::new();
        let q = qtn("Pair");
        cx.register(q.clone(), vec![bool_ty(), bool_ty()]);
        let qty = class_ty(&q);

        // arm covers (true, true) and (true, false).
        let arm1 = Pat::class(
            q.clone(),
            vec![
                Pat::single(bool_lit(true), bool_ty()),
                Pat::wildcard(bool_ty()),
            ],
            qty.clone(),
        );
        // arm covers (false, true) and (false, false).
        let arm2 = Pat::class(
            q.clone(),
            vec![
                Pat::single(bool_lit(false), bool_ty()),
                Pat::wildcard(bool_ty()),
            ],
            qty.clone(),
        );

        let report = compute_match_usefulness(&cx, &[arm1, arm2], qty);
        assert!(
            report.missing.is_empty(),
            "expected exhaustive: {:?}",
            missing_strings(&report)
        );
    }

    // ── 2. Variable slice with prefix and suffix ────────────────────────

    /// `[true, ..rest, false]` + `[..rest]` covers everything.
    #[test]
    fn testing_2_variable_prefix_suffix() {
        let cx = TestingCtx::new();
        let arr = list_of(bool_ty());

        let arm1 = Pat::slice(
            SliceShape::Variable {
                prefix: 1,
                suffix: 1,
            },
            vec![
                Pat::single(bool_lit(true), bool_ty()),
                Pat::single(bool_lit(false), bool_ty()),
            ],
            arr.clone(),
        );
        let arm2 = Pat::slice(
            SliceShape::Variable {
                prefix: 0,
                suffix: 0,
            },
            vec![],
            arr.clone(),
        );

        let report = compute_match_usefulness(&cx, &[arm1, arm2], arr);
        assert!(
            report.missing.is_empty(),
            "expected exhaustive: {:?}",
            missing_strings(&report)
        );
        assert!(
            report.unreachable_arms.is_empty(),
            "no arms should be unreachable, got {:?}",
            report.unreachable_arms
        );
    }

    // ── 3. Mixed fixed + variable with overlap ──────────────────────────

    /// `[]`, `[_]`, `[_, _]`, `[_, _, _, ..]` covers all lengths. Drop one,
    /// verify witness; add unreachable arm, verify detection.
    #[test]
    fn testing_3_mixed_lengths() {
        let cx = TestingCtx::new();
        let arr = list_of(int_ty());

        let arm0 = Pat::slice(SliceShape::Fixed(0), vec![], arr.clone());
        let arm1 = Pat::slice(
            SliceShape::Fixed(1),
            vec![Pat::wildcard(int_ty())],
            arr.clone(),
        );
        let arm2 = Pat::slice(
            SliceShape::Fixed(2),
            vec![Pat::wildcard(int_ty()), Pat::wildcard(int_ty())],
            arr.clone(),
        );
        let arm3plus = Pat::slice(
            SliceShape::Variable {
                prefix: 3,
                suffix: 0,
            },
            vec![
                Pat::wildcard(int_ty()),
                Pat::wildcard(int_ty()),
                Pat::wildcard(int_ty()),
            ],
            arr.clone(),
        );

        // (a) All four — exhaustive.
        let report = compute_match_usefulness(
            &cx,
            &[arm0.clone(), arm1.clone(), arm2.clone(), arm3plus.clone()],
            arr.clone(),
        );
        assert!(
            report.missing.is_empty(),
            "expected exhaustive: {:?}",
            missing_strings(&report)
        );

        // (b) Drop arm2 — witness must mention length 2.
        let report = compute_match_usefulness(
            &cx,
            &[arm0.clone(), arm1.clone(), arm3plus.clone()],
            arr.clone(),
        );
        assert!(
            !report.missing.is_empty(),
            "expected non-exhaustive when [_, _] is missing"
        );

        // (c) Add `[1, 2]` after arm2 — unreachable.
        let arm_lit = Pat::slice(
            SliceShape::Fixed(2),
            vec![
                Pat::single(int_lit(1), int_ty()),
                Pat::single(int_lit(2), int_ty()),
            ],
            arr.clone(),
        );
        let report = compute_match_usefulness(&cx, &[arm0, arm1, arm2, arm_lit, arm3plus], arr);
        assert!(
            report.unreachable_arms.contains(&ArmId(3)),
            "arm 3 (literal pair after wildcard pair) should be unreachable, got {:?}",
            report.unreachable_arms
        );
    }

    // ── 4. Array of classes ─────────────────────────────────────────────

    /// `Array<Result>` where Result = { ok: bool, val: bool }. Cover length 0,
    /// length-1 splits on ok, length 2+ via variable. Drop the false-ok branch
    /// and expect a nested witness `[{ ok: false, val: _ }]`.
    #[test]
    fn testing_4_array_of_classes() {
        let mut cx = TestingCtx::new();
        let r = qtn("Result");
        cx.register(r.clone(), vec![bool_ty(), bool_ty()]);
        let result_ty = class_ty(&r);
        let arr = list_of(result_ty.clone());

        let make_class = |ok: bool, val: Option<bool>| {
            Pat::class(
                r.clone(),
                vec![
                    Pat::single(bool_lit(ok), bool_ty()),
                    match val {
                        Some(v) => Pat::single(bool_lit(v), bool_ty()),
                        None => Pat::wildcard(bool_ty()),
                    },
                ],
                result_ty.clone(),
            )
        };

        let len0 = Pat::slice(SliceShape::Fixed(0), vec![], arr.clone());
        let len1_ok = Pat::slice(
            SliceShape::Fixed(1),
            vec![make_class(true, None)],
            arr.clone(),
        );
        let len1_err = Pat::slice(
            SliceShape::Fixed(1),
            vec![make_class(false, None)],
            arr.clone(),
        );
        let len2plus = Pat::slice(
            SliceShape::Variable {
                prefix: 2,
                suffix: 0,
            },
            vec![
                Pat::wildcard(result_ty.clone()),
                Pat::wildcard(result_ty.clone()),
            ],
            arr.clone(),
        );

        // Full coverage — exhaustive.
        let report = compute_match_usefulness(
            &cx,
            &[
                len0.clone(),
                len1_ok.clone(),
                len1_err.clone(),
                len2plus.clone(),
            ],
            arr.clone(),
        );
        assert!(
            report.missing.is_empty(),
            "expected exhaustive: {:?}",
            missing_strings(&report)
        );

        // Drop the false-ok branch — witness must mention `ok: false` inside
        // a length-1 slice.
        let report = compute_match_usefulness(&cx, &[len0, len1_ok, len2plus], arr);
        assert!(
            !report.missing.is_empty(),
            "expected missing when len-1 false-ok arm dropped"
        );
        let strs = missing_strings(&report);
        assert!(
            strs.iter().any(|s| s.contains("false")),
            "witness should mention the missing false-ok class case, got {:?}",
            strs
        );
    }

    // ── 5. Class with array field ───────────────────────────────────────

    /// `class Container { tag: bool, items: Array<bool> }`. Test drop of a
    /// nested fixed-length slice surfaces a nested witness.
    #[test]
    fn testing_5_class_with_array_field() {
        let mut cx = TestingCtx::new();
        let c = qtn("Container");
        let arr = list_of(bool_ty());
        cx.register(c.clone(), vec![bool_ty(), arr.clone()]);
        let cty = class_ty(&c);

        let cont = |tag: bool, items: Pat| {
            Pat::class(
                c.clone(),
                vec![Pat::single(bool_lit(tag), bool_ty()), items],
                cty.clone(),
            )
        };
        let true_empty = cont(true, Pat::slice(SliceShape::Fixed(0), vec![], arr.clone()));
        let true_nonempty = cont(
            true,
            Pat::slice(
                SliceShape::Variable {
                    prefix: 1,
                    suffix: 0,
                },
                vec![Pat::wildcard(bool_ty())],
                arr.clone(),
            ),
        );
        let false_any = cont(false, Pat::wildcard(arr.clone()));

        // (a) All three — exhaustive.
        let report = compute_match_usefulness(
            &cx,
            &[true_empty.clone(), true_nonempty.clone(), false_any.clone()],
            cty.clone(),
        );
        assert!(
            report.missing.is_empty(),
            "expected exhaustive: {:?}",
            missing_strings(&report)
        );

        // (b) Drop `true_empty` — witness must mention `tag: true` and an
        // empty list shape.
        let report = compute_match_usefulness(&cx, &[true_nonempty, false_any], cty);
        assert!(
            !report.missing.is_empty(),
            "expected missing when (tag: true, items: []) dropped"
        );
    }

    // ── 6. Optional<Optional<bool>> ─────────────────────────────────────

    /// Triple-state: outer null, inner null, true, false. Without flattening,
    /// these are 4 distinct cases.
    #[test]
    fn testing_6_double_optional() {
        let cx = TestingCtx::new();
        let inner = opt_of(bool_ty());
        let outer = opt_of(inner.clone());

        let null = Pat::single(null_ty(), outer.clone());
        let bool_true = Pat::single(bool_lit(true), outer.clone());
        let bool_false = Pat::single(bool_lit(false), outer.clone());

        // Optional<Optional<bool>> enumerates to three required cases
        // {true, false, null} (the inner and outer nulls collapse).
        let report = compute_match_usefulness(
            &cx,
            &[null.clone(), bool_true.clone(), bool_false.clone()],
            outer.clone(),
        );
        assert!(
            report.missing.is_empty(),
            "with three-case enumeration this should be exhaustive: {:?}",
            missing_strings(&report)
        );
        // A duplicate-null arm covering the same case as `null` is reported
        // as unreachable.
        let report =
            compute_match_usefulness(&cx, &[null.clone(), null, bool_true, bool_false], outer);
        assert!(
            report.unreachable_arms.contains(&ArmId(1)),
            "duplicate null arm should be unreachable, got {:?}",
            report.unreachable_arms
        );
    }

    // ── 7. Or-pattern expansion + unreachable ───────────────────────────

    /// Or-pattern `1 | 2` lowers to two rows for arm 0; arm 1 is `2` and is
    /// unreachable; arm 2 is `3` and completes coverage.
    #[test]
    fn testing_7_or_pattern_unreachable() {
        let cx = TestingCtx::new();
        let scrut = union_of(vec![int_lit(1), int_lit(2), int_lit(3)]);

        let arm0_a = Pat::single(int_lit(1), scrut.clone());
        let arm0_b = Pat::single(int_lit(2), scrut.clone());
        let arm1 = Pat::single(int_lit(2), scrut.clone()); // unreachable
        let arm2 = Pat::single(int_lit(3), scrut.clone());

        // To use ArmId for arms_for_user, we need a way to share arm-id
        // across or-pattern rows. compute_match_usefulness assigns one ArmId
        // per row, so we lose source-arm grouping here. Verify behavior at
        // the row level: rows 0,1 useful (covering 1,2); row 2 unreachable
        // (matches 2 already covered); row 3 useful (covers 3).
        let report = compute_match_usefulness(&cx, &[arm0_a, arm0_b, arm1, arm2], scrut);
        assert!(
            report.missing.is_empty(),
            "expected exhaustive: {:?}",
            missing_strings(&report)
        );
        assert!(
            report.unreachable_arms.contains(&ArmId(2)),
            "row 2 (duplicate `2`) should be unreachable, got {:?}",
            report.unreachable_arms
        );
    }

    // ── 8. Discriminated union with destructuring ───────────────────────

    /// `match r: Ok | Err { Ok{val:true}, Ok{val:false}, Err{code:_} }`.
    /// Drop one Ok branch → witness `Ok { val: false }`.
    #[test]
    fn testing_8_discriminated_union() {
        let mut cx = TestingCtx::new();
        let ok = qtn("Ok");
        let err = qtn("Err");
        cx.register(ok.clone(), vec![bool_ty()]);
        cx.register(err.clone(), vec![int_ty()]);
        let ok_ty = class_ty(&ok);
        let err_ty = class_ty(&err);
        let scrut = union_of(vec![ok_ty.clone(), err_ty.clone()]);

        let mk_ok = |v: bool| {
            Pat::class(
                ok.clone(),
                vec![Pat::single(bool_lit(v), bool_ty())],
                ok_ty.clone(),
            )
        };
        let any_err = Pat::class(err.clone(), vec![Pat::wildcard(int_ty())], err_ty.clone());

        // (a) Full coverage exhaustive.
        let report = compute_match_usefulness(
            &cx,
            &[mk_ok(true), mk_ok(false), any_err.clone()],
            scrut.clone(),
        );
        assert!(
            report.missing.is_empty(),
            "expected exhaustive: {:?}",
            missing_strings(&report)
        );

        // (b) Drop `Ok{val:false}` → witness mentions Ok and false.
        let report = compute_match_usefulness(&cx, &[mk_ok(true), any_err], scrut);
        let strs = missing_strings(&report);
        assert!(
            strs.iter().any(|s| s.contains("Ok") && s.contains("false")),
            "expected `Ok {{ val: false }}` in missing, got {:?}",
            strs
        );
    }

    // ── 9. TypeVar substitution ─────────────────────────────────────────

    /// `class Wrapper<T> { inner: T }`. With T=int, `{ inner: _ }` requires
    /// wildcard. With T=bool, `{ inner: true }` + `{ inner: false }` exhausts.
    #[test]
    fn testing_9_generic_substitution() {
        // We can't (yet) substitute T → bool through the test ctx, so we
        // simulate: register Wrapper twice with different field types.
        let mut cx = TestingCtx::new();
        let w_int = qtn("WrapperInt");
        let w_bool = qtn("WrapperBool");
        cx.register(w_int.clone(), vec![int_ty()]);
        cx.register(w_bool.clone(), vec![bool_ty()]);

        // Int variant — wildcard required.
        let scrut = class_ty(&w_int);
        let arms = vec![Pat::class(
            w_int.clone(),
            vec![Pat::wildcard(int_ty())],
            scrut.clone(),
        )];
        let report = compute_match_usefulness(&cx, &arms, scrut);
        assert!(
            report.missing.is_empty(),
            "wildcard inner should be exhaustive: {:?}",
            missing_strings(&report)
        );

        // Bool variant — both literals exhausts without wildcard.
        let scrut = class_ty(&w_bool);
        let arms = vec![
            Pat::class(
                w_bool.clone(),
                vec![Pat::single(bool_lit(true), bool_ty())],
                scrut.clone(),
            ),
            Pat::class(
                w_bool.clone(),
                vec![Pat::single(bool_lit(false), bool_ty())],
                scrut.clone(),
            ),
        ];
        let report = compute_match_usefulness(&cx, &arms, scrut);
        assert!(
            report.missing.is_empty(),
            "bool inner with both literals should be exhaustive: {:?}",
            missing_strings(&report)
        );
    }

    // ── 10. Witness rendering equality ──────────────────────────────────

    /// Render a missing witness to a known-good string. Catches Display bugs.
    #[test]
    fn testing_10_witness_exact_rendering() {
        let mut cx = TestingCtx::new();
        let p = qtn("Pair");
        cx.register(p.clone(), vec![bool_ty(), bool_ty()]);
        let pty = class_ty(&p);

        let arm = Pat::class(
            p.clone(),
            vec![
                Pat::single(bool_lit(true), bool_ty()),
                Pat::single(bool_lit(true), bool_ty()),
            ],
            pty.clone(),
        );
        let report = compute_match_usefulness(&cx, &[arm], pty);
        let strs = missing_strings(&report);
        // Three missing combos: (T,F), (F,T), (F,F). Just check rendering
        // includes the class name and both bool words.
        assert_eq!(strs.len(), 3, "expected 3 missing combos, got {:?}", strs);
        for s in &strs {
            assert!(
                s.contains("Pair") || s.contains("user.Pair"),
                "missing should render with class qualified name, got {}",
                s
            );
        }
    }

    // ── 11. Pathological depth ──────────────────────────────────────────

    /// `class A { b: B }; class B { c: Array<C> }; class C { v: bool }`.
    /// Deeply nested missing case unwinding.
    #[test]
    fn testing_11_deep_nesting() {
        let mut cx = TestingCtx::new();
        let a = qtn("A");
        let b = qtn("B");
        let c = qtn("C");
        cx.register(c.clone(), vec![bool_ty()]);
        let c_ty = class_ty(&c);
        cx.register(b.clone(), vec![list_of(c_ty.clone())]);
        let b_ty = class_ty(&b);
        cx.register(a.clone(), vec![b_ty.clone()]);
        let a_ty = class_ty(&a);

        // arm: A { b: B { c: [] } }
        let arm = Pat::class(
            a.clone(),
            vec![Pat::class(
                b.clone(),
                vec![Pat::slice(
                    SliceShape::Fixed(0),
                    vec![],
                    list_of(c_ty.clone()),
                )],
                b_ty.clone(),
            )],
            a_ty.clone(),
        );
        let report = compute_match_usefulness(&cx, &[arm], a_ty);
        // Many missing — at least one length-1 nested case should appear.
        assert!(
            !report.missing.is_empty(),
            "expected missing length-≥1 nested cases"
        );
    }

    // ── 12. Zero-arity edge cases ───────────────────────────────────────

    /// Empty array only arm against `Array<int>` → witness mentions `..`.
    #[test]
    fn testing_12a_empty_only_arm_for_list() {
        let cx = TestingCtx::new();
        let arr = list_of(int_ty());
        let arm = Pat::slice(SliceShape::Fixed(0), vec![], arr.clone());
        let report = compute_match_usefulness(&cx, &[arm], arr);
        assert!(
            !report.missing.is_empty(),
            "non-empty arrays should be missing"
        );
    }

    /// Match on `Never` with zero arms is vacuously exhaustive.
    #[test]
    fn testing_12b_never_zero_arms_exhaustive() {
        let cx = TestingCtx::new();
        let never = never_ty();
        let report = compute_match_usefulness(&cx, &[], never);
        assert!(report.missing.is_empty());
    }

    // ── 13. Variable covers fixed → fixed unreachable ───────────────────

    /// `[..rest]` first, then `[true]`. The variable covers length-1 with
    /// any element, so the fixed arm is dead.
    #[test]
    fn testing_13_var_covers_fixed_unreachable() {
        let cx = TestingCtx::new();
        let arr = list_of(bool_ty());
        let any = Pat::slice(
            SliceShape::Variable {
                prefix: 0,
                suffix: 0,
            },
            vec![],
            arr.clone(),
        );
        let one_true = Pat::slice(
            SliceShape::Fixed(1),
            vec![Pat::single(bool_lit(true), bool_ty())],
            arr.clone(),
        );

        let report = compute_match_usefulness(&cx, &[any, one_true], arr);
        assert!(report.missing.is_empty(), "expected exhaustive");
        assert!(
            report.unreachable_arms.contains(&ArmId(1)),
            "second arm `[true]` must be unreachable, got {:?}",
            report.unreachable_arms
        );
    }

    // ── 14. Two variable slices neither covering the other ──────────────

    /// `[a, ..r]` (Var{1,0}) and `[..r, b]` (Var{0,1}). Both match exactly
    /// `length ≥ 1` — they differ only in which position they bind. From
    /// a coverage standpoint the second arm is redundant. Length 0 remains
    /// missing → witness `[]`.
    #[test]
    fn testing_14_two_vars_redundant_coverage() {
        let cx = TestingCtx::new();
        let arr = list_of(bool_ty());
        let pre = Pat::slice(
            SliceShape::Variable {
                prefix: 1,
                suffix: 0,
            },
            vec![Pat::wildcard(bool_ty())],
            arr.clone(),
        );
        let suf = Pat::slice(
            SliceShape::Variable {
                prefix: 0,
                suffix: 1,
            },
            vec![Pat::wildcard(bool_ty())],
            arr.clone(),
        );

        let report = compute_match_usefulness(&cx, &[pre, suf], arr);
        let strs: Vec<String> = report.missing.iter().map(|w| w.to_string()).collect();
        assert!(
            strs.iter().any(|s| s == "[]"),
            "expected `[]` missing, got {:?}",
            strs
        );
        // Second arm covers no new values → reported redundant.
        assert!(
            report.unreachable_arms.contains(&ArmId(1)),
            "second arm covers no new values; should be unreachable, got {:?}",
            report.unreachable_arms
        );
    }

    // ── 15. Length-0 witness for prefix-only variable ───────────────────

    /// `[_, ..rest]` is Var{1,0}. Length 0 missing → witness `[]`.
    #[test]
    fn testing_15_prefix_only_var_misses_zero() {
        let cx = TestingCtx::new();
        let arr = list_of(bool_ty());
        let arm = Pat::slice(
            SliceShape::Variable {
                prefix: 1,
                suffix: 0,
            },
            vec![Pat::wildcard(bool_ty())],
            arr.clone(),
        );
        let report = compute_match_usefulness(&cx, &[arm], arr);
        let strs: Vec<String> = report.missing.iter().map(|w| w.to_string()).collect();
        assert!(
            strs.iter().any(|s| s == "[]"),
            "expected `[]` missing for `[_, ..rest]` only, got {:?}",
            strs
        );
    }

    // ── 16. Lengths 0 and 1 missing for arity-2 variable ────────────────

    /// `[a, b, ..rest]` is Var{2,0}. Lengths 0 and 1 missing.
    #[test]
    fn testing_16_arity2_var_misses_short_lengths() {
        let cx = TestingCtx::new();
        let arr = list_of(bool_ty());
        let arm = Pat::slice(
            SliceShape::Variable {
                prefix: 2,
                suffix: 0,
            },
            vec![Pat::wildcard(bool_ty()), Pat::wildcard(bool_ty())],
            arr.clone(),
        );
        let report = compute_match_usefulness(&cx, &[arm], arr);
        let strs: Vec<String> = report.missing.iter().map(|w| w.to_string()).collect();
        assert!(
            strs.iter().any(|s| s == "[]"),
            "expected `[]` missing, got {:?}",
            strs
        );
        // length 1 missing too — render varies, look for a length-1-ish witness.
        let has_length_one = strs.iter().any(|s| {
            // `[_]` or `[true]` or `[false]` — anything single-element.
            s.starts_with('[')
                && s.ends_with(']')
                && !s.contains(',')
                && !s.contains("..")
                && s.len() > 2
        });
        assert!(
            has_length_one,
            "expected a length-1 witness in missing, got {:?}",
            strs
        );
    }

    // ── 17. Variable's arity already exceeds any fixed ──────────────────

    /// `[a, b]` (Fixed(2)) + `[..r]` (Var{0,0}). Var covers Fixed(2). Fixed
    /// should be unreachable.
    #[test]
    fn testing_17_var_covers_long_fixed() {
        let cx = TestingCtx::new();
        let arr = list_of(int_ty());
        let any = Pat::slice(
            SliceShape::Variable {
                prefix: 0,
                suffix: 0,
            },
            vec![],
            arr.clone(),
        );
        let pair = Pat::slice(
            SliceShape::Fixed(2),
            vec![Pat::wildcard(int_ty()), Pat::wildcard(int_ty())],
            arr.clone(),
        );
        let report = compute_match_usefulness(&cx, &[any, pair], arr);
        assert!(report.missing.is_empty());
        assert!(
            report.unreachable_arms.contains(&ArmId(1)),
            "fixed pair after variable any must be unreachable, got {:?}",
            report.unreachable_arms
        );
    }

    // ── 18. Asymmetric prefix/suffix variable ───────────────────────────

    /// `[a, b, c, ..r, d]` (Var{3,1}). Lengths 0..=3 missing on this arm
    /// alone. With `[..r]` added, exhaustive.
    #[test]
    fn testing_18_asymmetric_var() {
        let cx = TestingCtx::new();
        let arr = list_of(bool_ty());
        let var31 = Pat::slice(
            SliceShape::Variable {
                prefix: 3,
                suffix: 1,
            },
            vec![
                Pat::wildcard(bool_ty()),
                Pat::wildcard(bool_ty()),
                Pat::wildcard(bool_ty()),
                Pat::wildcard(bool_ty()),
            ],
            arr.clone(),
        );
        let report = compute_match_usefulness(&cx, &[var31.clone()], arr.clone());
        assert!(
            !report.missing.is_empty(),
            "Var{{3,1}} alone should leave shorter lengths missing"
        );

        // Add a catch-all `[..r]` — exhaustive.
        let any = Pat::slice(
            SliceShape::Variable {
                prefix: 0,
                suffix: 0,
            },
            vec![],
            arr.clone(),
        );
        let report = compute_match_usefulness(&cx, &[var31, any], arr);
        assert!(
            report.missing.is_empty(),
            "with `[..r]` added, should be exhaustive: {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );
    }

    // ── 19. Slice of slices ─────────────────────────────────────────────

    /// `Array<Array<bool>>` with `[[true, _]]`, `[[false, true]]`, `[..rest]`.
    /// Outer length-1 splits on inner length-2 patterns; outer ≥2 catchall.
    /// Length-1 inner-`[false, false]` should be missing.
    #[test]
    fn testing_19_slice_of_slices() {
        let cx = TestingCtx::new();
        let inner_ty = list_of(bool_ty());
        let outer_ty = list_of(inner_ty.clone());

        let outer_one_inner =
            |inner_pat: Pat| Pat::slice(SliceShape::Fixed(1), vec![inner_pat], outer_ty.clone());
        let inner_pat = |a: bool, b: Option<bool>| {
            Pat::slice(
                SliceShape::Fixed(2),
                vec![
                    Pat::single(bool_lit(a), bool_ty()),
                    match b {
                        Some(v) => Pat::single(bool_lit(v), bool_ty()),
                        None => Pat::wildcard(bool_ty()),
                    },
                ],
                inner_ty.clone(),
            )
        };
        let arm1 = outer_one_inner(inner_pat(true, None));
        let arm2 = outer_one_inner(inner_pat(false, Some(true)));
        let arm3 = Pat::slice(
            SliceShape::Variable {
                prefix: 0,
                suffix: 0,
            },
            vec![],
            outer_ty.clone(),
        );

        let report = compute_match_usefulness(&cx, &[arm1, arm2, arm3], outer_ty);
        // arm3 (Var{0,0}) covers everything missed; whole match is exhaustive.
        // But arms 1 & 2 are *not* unreachable — they were covered before
        // arm3, so they're useful.
        assert!(
            report.missing.is_empty(),
            "expected exhaustive: {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );
        assert!(
            !report.unreachable_arms.contains(&ArmId(0)),
            "arm 0 should be useful"
        );
        assert!(
            !report.unreachable_arms.contains(&ArmId(1)),
            "arm 1 should be useful"
        );
    }

    // ── 20. Array of class with class field destructure ─────────────────

    /// `Array<Pair>` where Pair = {a, b: bool}. Arms cover length 0,
    /// length-1 with a=true, and length≥2 catchall. Drop length-1-a=false →
    /// witness mentions inner class with a=false in length-1 slice.
    #[test]
    fn testing_20_array_of_class_destructure() {
        let mut cx = TestingCtx::new();
        let p = qtn("Pair");
        cx.register(p.clone(), vec![bool_ty(), bool_ty()]);
        let pty = class_ty(&p);
        let arr = list_of(pty.clone());

        let pair = |a: bool| {
            Pat::class(
                p.clone(),
                vec![
                    Pat::single(bool_lit(a), bool_ty()),
                    Pat::wildcard(bool_ty()),
                ],
                pty.clone(),
            )
        };
        let len0 = Pat::slice(SliceShape::Fixed(0), vec![], arr.clone());
        let len1_true = Pat::slice(SliceShape::Fixed(1), vec![pair(true)], arr.clone());
        let len1_false = Pat::slice(SliceShape::Fixed(1), vec![pair(false)], arr.clone());
        let len2plus = Pat::slice(
            SliceShape::Variable {
                prefix: 2,
                suffix: 0,
            },
            vec![Pat::wildcard(pty.clone()), Pat::wildcard(pty.clone())],
            arr.clone(),
        );

        // Full: exhaustive.
        let report = compute_match_usefulness(
            &cx,
            &[
                len0.clone(),
                len1_true.clone(),
                len1_false.clone(),
                len2plus.clone(),
            ],
            arr.clone(),
        );
        assert!(
            report.missing.is_empty(),
            "expected exhaustive: {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );

        // Drop length-1-a=false: missing length-1 with a=false.
        let report = compute_match_usefulness(&cx, &[len0, len1_true, len2plus], arr);
        let strs: Vec<String> = report.missing.iter().map(|w| w.to_string()).collect();
        assert!(
            strs.iter().any(|s| s.contains("false")),
            "expected witness mentioning `false` (the missing pair), got {:?}",
            strs
        );
    }

    // ── 21. Class with slice field, variable-only catchall ──────────────

    /// `class Holder { items: Array<bool> }` matched as `{ items: [..rest] }`
    /// only. The variable covers all lengths → exhaustive.
    #[test]
    fn testing_21_class_with_var_slice_field() {
        let mut cx = TestingCtx::new();
        let h = qtn("Holder");
        let arr = list_of(bool_ty());
        cx.register(h.clone(), vec![arr.clone()]);
        let hty = class_ty(&h);

        let arm = Pat::class(
            h.clone(),
            vec![Pat::slice(
                SliceShape::Variable {
                    prefix: 0,
                    suffix: 0,
                },
                vec![],
                arr,
            )],
            hty.clone(),
        );
        let report = compute_match_usefulness(&cx, &[arm], hty);
        assert!(
            report.missing.is_empty(),
            "expected exhaustive: {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );
    }

    // ── 22. Empty class destructure ─────────────────────────────────────

    /// `class Empty {}` — zero fields. Match `{}` exhaustive.
    #[test]
    fn testing_22_empty_class() {
        let mut cx = TestingCtx::new();
        let e = qtn("Empty");
        cx.register(e.clone(), vec![]);
        let ety = class_ty(&e);

        let arm = Pat::class(e.clone(), vec![], ety.clone());
        let report = compute_match_usefulness(&cx, &[arm], ety);
        assert!(report.missing.is_empty());
    }

    // ── 23. Class with slice-of-class field ─────────────────────────────

    /// `class Outer { rows: Array<Inner> }; class Inner { val: bool }`.
    /// Drop a deep case → witness reconstructs the full path.
    #[test]
    fn testing_23_class_slice_class_nesting() {
        let mut cx = TestingCtx::new();
        let outer = qtn("Outer");
        let inner = qtn("Inner");
        cx.register(inner.clone(), vec![bool_ty()]);
        let inner_ty = class_ty(&inner);
        let inner_arr = list_of(inner_ty.clone());
        cx.register(outer.clone(), vec![inner_arr.clone()]);
        let outer_ty = class_ty(&outer);

        let mk_inner = |v: bool| {
            Pat::class(
                inner.clone(),
                vec![Pat::single(bool_lit(v), bool_ty())],
                inner_ty.clone(),
            )
        };
        let arm = Pat::class(
            outer.clone(),
            vec![Pat::slice(
                SliceShape::Fixed(1),
                vec![mk_inner(true)],
                inner_arr.clone(),
            )],
            outer_ty.clone(),
        );
        let report = compute_match_usefulness(&cx, &[arm], outer_ty);
        // Several length-classes missing; at minimum a length-1 inner-false
        // case should appear.
        assert!(
            !report.missing.is_empty(),
            "expected non-exhaustive; only one deep path covered"
        );
    }

    // ── 24. Wildcard outer makes structural inner unreachable ───────────

    /// `_` first, then `{a: true, b: false}` — second arm dead.
    #[test]
    fn testing_24_wildcard_makes_structural_unreachable() {
        let mut cx = TestingCtx::new();
        let p = qtn("Pair");
        cx.register(p.clone(), vec![bool_ty(), bool_ty()]);
        let pty = class_ty(&p);

        let any = Pat::wildcard(pty.clone());
        let specific = Pat::class(
            p.clone(),
            vec![
                Pat::single(bool_lit(true), bool_ty()),
                Pat::single(bool_lit(false), bool_ty()),
            ],
            pty.clone(),
        );
        let report = compute_match_usefulness(&cx, &[any, specific], pty);
        assert!(report.missing.is_empty());
        assert!(
            report.unreachable_arms.contains(&ArmId(1)),
            "specific class arm after wildcard must be unreachable, got {:?}",
            report.unreachable_arms
        );
    }

    // ── 25. Two structural arms, no wildcards, full Cartesian via partial wildcards ─

    /// `class Pair{a,b: bool}` with `{a: true, b: _}` and `{a: false, b: _}`
    /// — exhaustive without any top-level wildcard (each arm wildcards b).
    #[test]
    fn testing_25_no_top_wildcard_exhaustive() {
        let mut cx = TestingCtx::new();
        let p = qtn("Pair");
        cx.register(p.clone(), vec![bool_ty(), bool_ty()]);
        let pty = class_ty(&p);

        let mk = |a: bool| {
            Pat::class(
                p.clone(),
                vec![
                    Pat::single(bool_lit(a), bool_ty()),
                    Pat::wildcard(bool_ty()),
                ],
                pty.clone(),
            )
        };
        let report = compute_match_usefulness(&cx, &[mk(true), mk(false)], pty);
        assert!(
            report.missing.is_empty(),
            "expected exhaustive: {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );
    }

    // ── 27. 8-bit cartesian (256 combinations) ──────────────────────────

    /// `class Octet { f0..f7: bool }`. All 256 combos cover → exhaustive.
    /// Drop one arbitrary combo (e.g. `0b10110100`) → exactly one missing.
    /// Stresses: matrix scale, deep specialization recursion.
    #[test]
    fn testing_27_8bit_cartesian() {
        let mut cx = TestingCtx::new();
        let q = qtn("Octet");
        cx.register(q.clone(), vec![bool_ty(); 8]);
        let qty = class_ty(&q);

        let combo = |bits: u8| {
            Pat::class(
                q.clone(),
                (0..8)
                    .map(|i| Pat::single(bool_lit((bits >> i) & 1 != 0), bool_ty()))
                    .collect(),
                qty.clone(),
            )
        };

        // All 256 → exhaustive.
        let arms: Vec<Pat> = (0u8..=255).map(combo).collect();
        let report = compute_match_usefulness(&cx, &arms, qty.clone());
        assert!(
            report.missing.is_empty(),
            "256 arms should be exhaustive: {} missing",
            report.missing.len()
        );
        assert!(report.unreachable_arms.is_empty());

        // Drop combo 0b10110100 (180) → exactly one missing.
        let arms: Vec<Pat> = (0u8..=255).filter(|b| *b != 180).map(combo).collect();
        let report = compute_match_usefulness(&cx, &arms, qty);
        assert_eq!(
            report.missing.len(),
            1,
            "expected exactly one missing case after dropping combo 180; got {}",
            report.missing.len()
        );
    }

    // ── 28. Recursive linked list ───────────────────────────────────────

    /// `class Node { val: bool, next: Optional<Node> }`. Match null at top,
    /// leaf node (next: null), and deeper-than-leaf (next: non-null) → exhaustive.
    /// Stresses: type recursion through Optional.
    #[test]
    fn testing_28_linked_list_recursion() {
        let mut cx = TestingCtx::new();
        let n = qtn("Node");
        let n_ty = class_ty(&n);
        let opt_n = opt_of(n_ty.clone());
        cx.register(n.clone(), vec![bool_ty(), opt_n.clone()]);

        // Scrutinee: Optional<Node>.
        let scrut = opt_n.clone();

        let null_top = Pat::single(null_ty(), scrut.clone());
        // Some(node) with any val and any next.
        let some_any = Pat::class(
            n.clone(),
            vec![Pat::wildcard(bool_ty()), Pat::wildcard(opt_n.clone())],
            n_ty.clone(),
        );
        let arms = vec![null_top.clone(), some_any.clone()];

        let report = compute_match_usefulness(&cx, &arms, scrut.clone());
        assert!(
            report.missing.is_empty(),
            "expected exhaustive (null + Some(_)): {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );

        // Drop `some_any` → witness must mention the Node class.
        let report = compute_match_usefulness(&cx, &[null_top], scrut);
        let strs: Vec<String> = report.missing.iter().map(|w| w.to_string()).collect();
        assert!(
            strs.iter().any(|s| s.contains("Node")),
            "expected Node-related witness, got {:?}",
            strs
        );
    }

    // ── 29. 5-level deep class nesting ──────────────────────────────────

    /// `A.b: B.c: C.d: D.e: E.v: bool`. Cover only `{b:{c:{d:{e:{v:true}}}}}` →
    /// missing `v: false` deeply nested. Verify witness preserves the chain.
    #[test]
    fn testing_29_5level_deep_nesting() {
        let mut cx = TestingCtx::new();
        let e = qtn("E");
        cx.register(e.clone(), vec![bool_ty()]);
        let e_ty = class_ty(&e);
        let d = qtn("D");
        cx.register(d.clone(), vec![e_ty.clone()]);
        let d_ty = class_ty(&d);
        let c = qtn("C");
        cx.register(c.clone(), vec![d_ty.clone()]);
        let c_ty = class_ty(&c);
        let b = qtn("B");
        cx.register(b.clone(), vec![c_ty.clone()]);
        let b_ty = class_ty(&b);
        let a = qtn("A");
        cx.register(a.clone(), vec![b_ty.clone()]);
        let a_ty = class_ty(&a);

        let mk = |v: bool| {
            Pat::class(
                a.clone(),
                vec![Pat::class(
                    b.clone(),
                    vec![Pat::class(
                        c.clone(),
                        vec![Pat::class(
                            d.clone(),
                            vec![Pat::class(
                                e.clone(),
                                vec![Pat::single(bool_lit(v), bool_ty())],
                                e_ty.clone(),
                            )],
                            d_ty.clone(),
                        )],
                        c_ty.clone(),
                    )],
                    b_ty.clone(),
                )],
                a_ty.clone(),
            )
        };

        // Both bool inner values → exhaustive.
        let report = compute_match_usefulness(&cx, &[mk(true), mk(false)], a_ty.clone());
        assert!(
            report.missing.is_empty(),
            "both leaf bools should be exhaustive: {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );

        // Drop false → witness mentions full chain and `false`.
        let report = compute_match_usefulness(&cx, &[mk(true)], a_ty);
        assert_eq!(
            report.missing.len(),
            1,
            "exactly one missing — the inner-false case"
        );
        let s = report.missing[0].to_string();
        assert!(
            s.contains("A")
                && s.contains("B")
                && s.contains("C")
                && s.contains("D")
                && s.contains("E"),
            "witness should mention all 5 class levels, got {}",
            s
        );
        assert!(
            s.contains("false"),
            "witness should mention false at the leaf, got {}",
            s
        );
    }

    // ── 30. 3D slice (Array<Array<Array<bool>>>) ────────────────────────

    /// 3-level slice nesting. Outer/middle wildcards, only innermost
    /// constrained. Catch-all `[..]` at outer makes whole match exhaustive.
    #[test]
    fn testing_30_3d_slice() {
        let cx = TestingCtx::new();
        let lvl1 = list_of(bool_ty());
        let lvl2 = list_of(lvl1.clone());
        let lvl3 = list_of(lvl2.clone());

        // Specific deep arm: [[[true]]] (length 1 / 1 / 1).
        let inner_true = Pat::slice(
            SliceShape::Fixed(1),
            vec![Pat::single(bool_lit(true), bool_ty())],
            lvl1.clone(),
        );
        let mid = Pat::slice(SliceShape::Fixed(1), vec![inner_true], lvl2.clone());
        let outer = Pat::slice(SliceShape::Fixed(1), vec![mid], lvl3.clone());

        // Catch-all.
        let any = Pat::slice(
            SliceShape::Variable {
                prefix: 0,
                suffix: 0,
            },
            vec![],
            lvl3.clone(),
        );

        let report = compute_match_usefulness(&cx, &[outer, any], lvl3.clone());
        assert!(
            report.missing.is_empty(),
            "expected exhaustive with `[..]` catch-all: {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );
        assert!(report.unreachable_arms.is_empty());

        // Without the catch-all, MANY missing cases.
        let inner_true2 = Pat::slice(
            SliceShape::Fixed(1),
            vec![Pat::single(bool_lit(true), bool_ty())],
            lvl1.clone(),
        );
        let mid2 = Pat::slice(SliceShape::Fixed(1), vec![inner_true2], lvl2.clone());
        let outer2 = Pat::slice(SliceShape::Fixed(1), vec![mid2], lvl3.clone());
        let report = compute_match_usefulness(&cx, &[outer2], lvl3);
        assert!(!report.missing.is_empty());
    }

    // ── 31. Subsumption redundancy ──────────────────────────────────────

    /// `{a: T, b: _}` covers a-true × any-b. `{a: _, b: F}` covers any-a × b-false.
    /// Together: covers everything except `{a: F, b: T}`. So adding
    /// `{a: T, b: F}` after both → redundant. Adding `{a: F, b: T}` → useful
    /// (the only missing case). Then a final `{a: T, b: F}` after that → also
    /// redundant.
    #[test]
    fn testing_31_subsumption_redundancy() {
        let mut cx = TestingCtx::new();
        let p = qtn("Pair");
        cx.register(p.clone(), vec![bool_ty(), bool_ty()]);
        let pty = class_ty(&p);

        let mk = |a: Option<bool>, b: Option<bool>| {
            Pat::class(
                p.clone(),
                vec![
                    a.map_or(Pat::wildcard(bool_ty()), |v| {
                        Pat::single(bool_lit(v), bool_ty())
                    }),
                    b.map_or(Pat::wildcard(bool_ty()), |v| {
                        Pat::single(bool_lit(v), bool_ty())
                    }),
                ],
                pty.clone(),
            )
        };
        let a_true_any = mk(Some(true), None);
        let any_b_false = mk(None, Some(false));
        let t_f = mk(Some(true), Some(false));
        let f_t = mk(Some(false), Some(true));

        // (a) After two arms, `(F, T)` is missing.
        let report =
            compute_match_usefulness(&cx, &[a_true_any.clone(), any_b_false.clone()], pty.clone());
        assert_eq!(report.missing.len(), 1);
        let s = report.missing[0].to_string();
        assert!(s.contains("false") && s.contains("true"), "got {}", s);

        // (b) Adding `(T, F)` is subsumed by both prior arms — redundant.
        let report = compute_match_usefulness(
            &cx,
            &[a_true_any.clone(), any_b_false.clone(), t_f.clone()],
            pty.clone(),
        );
        assert!(
            report.unreachable_arms.contains(&ArmId(2)),
            "third arm `(T,F)` is subsumed; expected unreachable, got {:?}",
            report.unreachable_arms
        );

        // (c) Adding `(F, T)` plugs the missing hole — exhaustive.
        let report = compute_match_usefulness(&cx, &[a_true_any, any_b_false, f_t], pty);
        assert!(report.missing.is_empty());
    }

    // ── 32. 15-class alphabet union ─────────────────────────────────────

    /// Union of 15 classes, each distinct. Cover all 15 → exhaustive.
    /// Drop one → exact missing.
    #[test]
    fn testing_32_alphabet_union() {
        let mut cx = TestingCtx::new();
        let names = [
            "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O",
        ];
        let qtns: Vec<_> = names.iter().map(|n| qtn(n)).collect();
        for q in &qtns {
            cx.register(q.clone(), vec![]);
        }
        let scrut = union_of(qtns.iter().map(class_ty).collect());

        let arms_all: Vec<_> = qtns
            .iter()
            .map(|q| Pat::class(q.clone(), vec![], class_ty(q)))
            .collect();
        let report = compute_match_usefulness(&cx, &arms_all, scrut.clone());
        assert!(report.missing.is_empty());

        // Drop "G" → exactly G missing.
        let arms: Vec<_> = qtns
            .iter()
            .filter(|q| q.name().as_str() != "G")
            .map(|q| Pat::class(q.clone(), vec![], class_ty(q)))
            .collect();
        let report = compute_match_usefulness(&cx, &arms, scrut);
        assert_eq!(report.missing.len(), 1);
        let s = report.missing[0].to_string();
        assert!(s.contains("G"), "expected G in missing witness, got {}", s);
    }

    // ── 33. Alternating structural (class > array > class > array > class) ─

    /// `class O { rows: Array<class M { items: Array<class I { v: bool }> }> }`.
    /// Drop a deep case → witness threads through every level.
    #[test]
    fn testing_33_alternating_structural() {
        let mut cx = TestingCtx::new();
        let i = qtn("I");
        cx.register(i.clone(), vec![bool_ty()]);
        let i_ty = class_ty(&i);
        let i_arr = list_of(i_ty.clone());
        let m = qtn("M");
        cx.register(m.clone(), vec![i_arr.clone()]);
        let m_ty = class_ty(&m);
        let m_arr = list_of(m_ty.clone());
        let o = qtn("O");
        cx.register(o.clone(), vec![m_arr.clone()]);
        let o_ty = class_ty(&o);

        // Single arm at the deepest specific case: O { rows: [M { items: [I{v:true}] }] }
        let inner = Pat::class(
            i.clone(),
            vec![Pat::single(bool_lit(true), bool_ty())],
            i_ty.clone(),
        );
        let inner_arr_one = Pat::slice(SliceShape::Fixed(1), vec![inner], i_arr.clone());
        let mid = Pat::class(m.clone(), vec![inner_arr_one], m_ty.clone());
        let mid_arr_one = Pat::slice(SliceShape::Fixed(1), vec![mid], m_arr.clone());
        let outer = Pat::class(o.clone(), vec![mid_arr_one], o_ty.clone());

        let report = compute_match_usefulness(&cx, &[outer], o_ty);
        assert!(
            !report.missing.is_empty(),
            "single deep arm should leave many missing"
        );
    }

    // ── 34. Wide variable slice (prefix=5, suffix=3) ────────────────────

    /// Variable slice with arity 8. Lengths 0..8 missing on this arm; with
    /// `[..]` catch-all, exhaustive.
    #[test]
    fn testing_34_wide_variable_slice() {
        let cx = TestingCtx::new();
        let arr = list_of(bool_ty());
        let var53 = Pat::slice(
            SliceShape::Variable {
                prefix: 5,
                suffix: 3,
            },
            (0..8).map(|_| Pat::wildcard(bool_ty())).collect(),
            arr.clone(),
        );
        let any = Pat::slice(
            SliceShape::Variable {
                prefix: 0,
                suffix: 0,
            },
            vec![],
            arr.clone(),
        );
        let report = compute_match_usefulness(&cx, &[var53, any], arr);
        assert!(
            report.missing.is_empty(),
            "expected exhaustive: {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );
    }

    // ── 35. Pathological subset arms, no top wildcard ───────────────────

    /// `class T3 { a, b, c: bool }`. 8 combos. Cover via 4 partial-wildcard
    /// arms (each covering 2 combos): exhaustive without any top-level
    /// wildcard.
    #[test]
    fn testing_35_partial_wildcard_cartesian() {
        let mut cx = TestingCtx::new();
        let t = qtn("Triple");
        cx.register(t.clone(), vec![bool_ty(), bool_ty(), bool_ty()]);
        let tty = class_ty(&t);

        let mk = |a: bool, b: bool| {
            Pat::class(
                t.clone(),
                vec![
                    Pat::single(bool_lit(a), bool_ty()),
                    Pat::single(bool_lit(b), bool_ty()),
                    Pat::wildcard(bool_ty()),
                ],
                tty.clone(),
            )
        };
        let arms = vec![
            mk(true, true),
            mk(true, false),
            mk(false, true),
            mk(false, false),
        ];
        let report = compute_match_usefulness(&cx, &arms, tty);
        assert!(
            report.missing.is_empty(),
            "expected exhaustive: {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );
    }

    // ── 36. Many-arm scale stress ───────────────────────────────────────

    /// 64 arms over a 6-bool class (64 combos = exhaustive). Verify no
    /// blow-up.
    #[test]
    fn testing_36_64_arm_scale() {
        let mut cx = TestingCtx::new();
        let q = qtn("Sextet");
        cx.register(q.clone(), vec![bool_ty(); 6]);
        let qty = class_ty(&q);

        let combo = |bits: u8| {
            Pat::class(
                q.clone(),
                (0..6)
                    .map(|i| Pat::single(bool_lit((bits >> i) & 1 != 0), bool_ty()))
                    .collect(),
                qty.clone(),
            )
        };
        let arms: Vec<Pat> = (0u8..64).map(combo).collect();
        let report = compute_match_usefulness(&cx, &arms, qty);
        assert!(report.missing.is_empty());
        assert!(report.unreachable_arms.is_empty());
    }

    // ── 37. Witness dedup: only one missing case rendered once ──────────

    /// `class P { a, b: bool }` with 3 of 4 combos. Verify exactly one
    /// missing witness (no duplicates from algorithmic paths).
    #[test]
    fn testing_37_witness_uniqueness() {
        let mut cx = TestingCtx::new();
        let p = qtn("P");
        cx.register(p.clone(), vec![bool_ty(), bool_ty()]);
        let pty = class_ty(&p);
        let mk = |a: bool, b: bool| {
            Pat::class(
                p.clone(),
                vec![
                    Pat::single(bool_lit(a), bool_ty()),
                    Pat::single(bool_lit(b), bool_ty()),
                ],
                pty.clone(),
            )
        };
        let arms = vec![mk(true, true), mk(true, false), mk(false, true)];
        let report = compute_match_usefulness(&cx, &arms, pty);
        assert_eq!(
            report.missing.len(),
            1,
            "expected exactly one witness; got {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );
    }

    // ── 38. Mixed deep + wide ───────────────────────────────────────────

    /// `class A { x, y, z: B }; class B { p, q: bool }`. 2*2*2*2*2*2 = 64
    /// combos in three nested 2-bool classes. Cover all 8 outer × inner
    /// combinations through structural patterns with partial wildcards:
    /// exhaustive via 8 arms.
    #[test]
    fn testing_38_mixed_deep_wide() {
        let mut cx = TestingCtx::new();
        let b = qtn("B");
        cx.register(b.clone(), vec![bool_ty(), bool_ty()]);
        let b_ty = class_ty(&b);
        let a = qtn("A");
        cx.register(a.clone(), vec![b_ty.clone(), b_ty.clone(), b_ty.clone()]);
        let a_ty = class_ty(&a);

        let mk_b = |p: bool, q: bool| {
            Pat::class(
                b.clone(),
                vec![
                    Pat::single(bool_lit(p), bool_ty()),
                    Pat::single(bool_lit(q), bool_ty()),
                ],
                b_ty.clone(),
            )
        };
        // Cover all 64 = 4^3 combos.
        let mk_b_n = |bits: usize| mk_b((bits & 1) != 0, (bits & 2) != 0);
        let mut arms = Vec::new();
        for px in 0..4 {
            for py in 0..4 {
                for pz in 0..4 {
                    arms.push(Pat::class(
                        a.clone(),
                        vec![mk_b_n(px), mk_b_n(py), mk_b_n(pz)],
                        a_ty.clone(),
                    ));
                }
            }
        }
        assert_eq!(arms.len(), 64);
        let report = compute_match_usefulness(&cx, &arms, a_ty);
        assert!(
            report.missing.is_empty(),
            "expected exhaustive over all 64 combos: {} missing",
            report.missing.len()
        );
        assert!(report.unreachable_arms.is_empty());
    }

    // ── 39. Suffix-slice doesn't cover a wrong fixed tail ───────────────

    /// `[..rest, true]` only matches arrays ending in `true`. The fixed
    /// pair `[true, false]` ends in `false`, so the suffix arm does NOT
    /// cover it. The fixed arm must remain reachable. Exposes whether
    /// `specialize` correctly aligns suffix fields to the rightmost
    /// positions of a matching `Fixed(N)`.
    #[test]
    fn testing_39_suffix_slice_does_not_cover_wrong_fixed_tail() {
        let cx = TestingCtx::new();
        let arr = list_of(bool_ty());
        let ends_true = Pat::slice(
            SliceShape::Variable {
                prefix: 0,
                suffix: 1,
            },
            vec![Pat::single(bool_lit(true), bool_ty())],
            arr.clone(),
        );
        let true_false = Pat::slice(
            SliceShape::Fixed(2),
            vec![
                Pat::single(bool_lit(true), bool_ty()),
                Pat::single(bool_lit(false), bool_ty()),
            ],
            arr.clone(),
        );
        let report = compute_match_usefulness(&cx, &[ends_true, true_false], arr);
        assert!(
            !report.unreachable_arms.contains(&ArmId(1)),
            "`[true, false]` ends in false; `[..rest, true]` cannot cover it. \
             second arm must be reachable, got {:?}",
            report.unreachable_arms
        );
    }

    // ── 40. Or-pattern alts share their source arm id ───────────────────

    /// `match x: 1 | 2 { 1 | 2 => a; 2 => b }`: source arm 0's or-pattern
    /// covers both values; arm 1 is then dead. Reported unreachable arm
    /// must be `ArmId(1)` (the *source* arm). Or-pattern is encoded as
    /// `Pat::or`; expansion happens inside the algorithm.
    #[test]
    fn testing_40_or_pattern_source_arm_ids() {
        let cx = TestingCtx::new();
        let scrut = union_of(vec![int_lit(1), int_lit(2)]);

        let arm0 = Pat::or(
            vec![
                Pat::single(int_lit(1), scrut.clone()),
                Pat::single(int_lit(2), scrut.clone()),
            ],
            scrut.clone(),
        );
        let arm1 = Pat::single(int_lit(2), scrut.clone());

        let report = compute_match_usefulness(&cx, &[arm0, arm1], scrut);
        assert_eq!(
            report.unreachable_arms,
            vec![ArmId(1)],
            "expected source arm 1 unreachable, got {:?}",
            report.unreachable_arms
        );
    }

    // ── 41. Deep nesting must not silently mark a missing leaf exhaustive ─

    /// Build a chain of 258 nested classes, each with one field of the
    /// next class type, the innermost being `bool`. A single arm matches
    /// the deepest `true` leaf — `false` at the leaf is missing. The
    /// algorithm must report at least one missing case even at this depth.
    #[test]
    fn testing_41_deep_chain_missing_leaf() {
        let mut cx = TestingCtx::new();
        let qs: Vec<_> = (0..=257).map(|i| qtn(&format!("C{i}"))).collect();

        for i in 0..qs.len() {
            let field = if i + 1 == qs.len() {
                bool_ty()
            } else {
                class_ty(&qs[i + 1])
            };
            cx.register(qs[i].clone(), vec![field]);
        }

        let mut pat = Pat::single(bool_lit(true), bool_ty());
        for q in qs.iter().rev() {
            pat = Pat::class(q.clone(), vec![pat], class_ty(q));
        }

        let report = compute_match_usefulness(&cx, &[pat], class_ty(&qs[0]));
        assert!(
            !report.missing.is_empty(),
            "deep missing leaf must surface; depth guard must not silently \
             return exhaustive"
        );
    }

    // ── 42. Array<Never> with `[]` arm only is exhaustive ───────────────

    /// `Array<Never>` is uninhabited at length ≥ 1 (Never has no values to
    /// fill positions). The only reachable value is `[]`. Matching `[]`
    /// alone must therefore be exhaustive — no length-≥1 witness should be
    /// reported. Exposes whether `apply_missing` skips ctors with
    /// uninhabited field types.
    #[test]
    fn testing_42_list_of_never_empty_arm_is_exhaustive() {
        let cx = TestingCtx::new();
        let never = never_ty();
        let arr = list_of(never);
        let empty = Pat::slice(SliceShape::Fixed(0), vec![], arr.clone());

        let report = compute_match_usefulness(&cx, &[empty], arr);
        assert!(
            report.missing.is_empty(),
            "Array<Never> only inhabits []; got missing: {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );
    }

    // ── 43. Duplicate ctors deduped in missing witnesses ────────────────

    /// `Optional<Optional<bool>>`: enumerate pushes `null` once per Optional
    /// layer, so `all` ends up with two `Single(null)` entries — semantically
    /// the same case. The missing list and the resulting witnesses must be
    /// deduplicated; otherwise `null` would be reported twice.
    #[test]
    fn testing_43_duplicate_ctors_deduped_in_missing() {
        let cx = TestingCtx::new();
        let outer = opt_of(opt_of(bool_ty()));

        let arms = vec![
            Pat::single(bool_lit(true), outer.clone()),
            Pat::single(bool_lit(false), outer.clone()),
        ];

        let report = compute_match_usefulness(&cx, &arms, outer);
        assert_eq!(
            report.missing.len(),
            1,
            "expected exactly one missing witness; got {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(report.missing[0].to_string(), "null");
    }

    // ── 46. Transitively-uninhabited class is exhaustive with zero arms ─

    /// `class Inner { f: Never }` cannot be constructed (no `Never` value
    /// to fill the field). `class Outer { f: Inner }` cannot be constructed
    /// either. A match on `Outer` with zero arms is vacuously exhaustive.
    /// Requires `PatCtx::is_inhabited` to walk class fields recursively.
    #[test]
    fn testing_46_nested_uninhabited_class_empty_match_exhaustive() {
        let mut cx = TestingCtx::new();
        let inner = qtn("Inner");
        let outer = qtn("Outer");
        cx.register(inner.clone(), vec![never_ty()]);
        cx.register(outer.clone(), vec![class_ty(&inner)]);

        let report = compute_match_usefulness(&cx, &[], class_ty(&outer));
        assert!(
            report.missing.is_empty(),
            "Outer is uninhabited via Inner; got missing {:?}",
            report
                .missing
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        );
    }

    /// Wildcard arm over an uninhabited scrutinee is unreachable. No value
    /// of `Empty` exists (its only field is `Never`), so the wildcard never
    /// fires. Match is vacuously exhaustive AND the wildcard arm is dead.
    #[test]
    fn testing_46c_wildcard_over_uninhabited_class_unreachable() {
        let mut cx = TestingCtx::new();
        let empty = qtn("Empty");
        cx.register(empty.clone(), vec![never_ty()]);
        let ty = class_ty(&empty);
        let report = compute_match_usefulness(&cx, &[Pat::wildcard(ty.clone())], ty);
        assert!(
            report.missing.is_empty(),
            "got missing {:?}",
            report.missing
        );
        assert_eq!(
            report.unreachable_arms,
            vec![ArmId(0)],
            "wildcard arm over uninhabited scrutinee must be unreachable"
        );
    }

    // ── UnionMember ctor: discriminate union branches ───────────────────

    /// `match val: Class | int[] { Class{} | [..] => ... }` — the slice
    /// arm covers the entire list branch (Variable{0,0} covers everything).
    /// With the `UnionMember` ctor, specialising on `UnionMember(int[])`
    /// recurses into a column of type `int[]`, where slice-splitting fires
    /// and recognises `[..]` as exhaustive.
    #[test]
    fn testing_47_union_member_slice_wildcard_exhaustive() {
        let mut cx = TestingCtx::new().with_union_members();
        let cls = qtn("Cls");
        cx.register(cls.clone(), vec![]);
        let cls_ty = class_ty(&cls);
        let arr = list_of(int_ty());
        let scrut = union_of(vec![cls_ty.clone(), arr.clone()]);

        let class_arm = Pat::union_member(
            cls_ty.clone(),
            Pat::class(cls.clone(), vec![], cls_ty.clone()),
            scrut.clone(),
        );
        let slice_arm = Pat::union_member(
            arr.clone(),
            Pat::slice(
                SliceShape::Variable {
                    prefix: 0,
                    suffix: 0,
                },
                vec![],
                arr.clone(),
            ),
            scrut.clone(),
        );

        let report = compute_match_usefulness(&cx, &[class_arm, slice_arm], scrut);
        assert!(
            report.missing.is_empty(),
            "expected exhaustive: {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );
    }

    /// `match val: Class | int[] { Class{} | [length-1-only] => ... }` —
    /// the slice arm covers length-1 only. Lengths 0 and 2+ remain
    /// missing on the list branch. Algorithm should report
    /// non-exhaustive (matching rustc behaviour for partial slice
    /// coverage inside an enum variant).
    #[test]
    fn testing_48_union_member_partial_slice_non_exhaustive() {
        let mut cx = TestingCtx::new().with_union_members();
        let cls = qtn("Cls");
        cx.register(cls.clone(), vec![]);
        let cls_ty = class_ty(&cls);
        let arr = list_of(int_ty());
        let scrut = union_of(vec![cls_ty.clone(), arr.clone()]);

        let class_arm = Pat::union_member(
            cls_ty.clone(),
            Pat::class(cls.clone(), vec![], cls_ty.clone()),
            scrut.clone(),
        );
        let slice_arm = Pat::union_member(
            arr.clone(),
            Pat::slice(
                SliceShape::Fixed(1),
                vec![Pat::wildcard(int_ty())],
                arr.clone(),
            ),
            scrut.clone(),
        );

        let report = compute_match_usefulness(&cx, &[class_arm, slice_arm], scrut);
        assert!(
            !report.missing.is_empty(),
            "expected non-exhaustive: only length-1 covered on list branch"
        );
    }

    /// `match val: Class | int[] { Class{} | [] | [_, ..] => ... }` —
    /// the combined slice arms `[]` + `[_, ..]` cover all lengths on the
    /// list branch via slice-splitting at the `UnionMember(int[])`
    /// recursion depth. This is the test case rustc handles natively
    /// thanks to specialise-then-recurse; we now do the same.
    #[test]
    fn testing_49_union_member_combined_slices_exhaustive() {
        let mut cx = TestingCtx::new().with_union_members();
        let cls = qtn("Cls");
        cx.register(cls.clone(), vec![]);
        let cls_ty = class_ty(&cls);
        let arr = list_of(int_ty());
        let scrut = union_of(vec![cls_ty.clone(), arr.clone()]);

        let class_arm = Pat::union_member(
            cls_ty.clone(),
            Pat::class(cls.clone(), vec![], cls_ty.clone()),
            scrut.clone(),
        );
        let empty_arm = Pat::union_member(
            arr.clone(),
            Pat::slice(SliceShape::Fixed(0), vec![], arr.clone()),
            scrut.clone(),
        );
        let nonempty_arm = Pat::union_member(
            arr.clone(),
            Pat::slice(
                SliceShape::Variable {
                    prefix: 1,
                    suffix: 0,
                },
                vec![Pat::wildcard(int_ty())],
                arr.clone(),
            ),
            scrut.clone(),
        );

        let report = compute_match_usefulness(&cx, &[class_arm, empty_arm, nonempty_arm], scrut);
        assert!(
            report.missing.is_empty(),
            "expected exhaustive (Class + len-0 + len-≥1 covers all): {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );
    }

    /// Missing one branch entirely: `match val: Class | int[] { Class{} => ... }`.
    /// The list branch is not covered. Algorithm reports a missing
    /// `UnionMember(int[])` witness.
    #[test]
    fn testing_50_union_member_missing_branch() {
        let mut cx = TestingCtx::new().with_union_members();
        let cls = qtn("Cls");
        cx.register(cls.clone(), vec![]);
        let cls_ty = class_ty(&cls);
        let arr = list_of(int_ty());
        let scrut = union_of(vec![cls_ty.clone(), arr.clone()]);

        let class_arm = Pat::union_member(
            cls_ty.clone(),
            Pat::class(cls.clone(), vec![], cls_ty.clone()),
            scrut.clone(),
        );

        let report = compute_match_usefulness(&cx, &[class_arm], scrut);
        assert!(!report.missing.is_empty());
    }

    /// Wildcard arm covers any union member: exhaustive.
    #[test]
    fn testing_51_union_member_wildcard_covers_all() {
        let mut cx = TestingCtx::new().with_union_members();
        let cls = qtn("Cls");
        cx.register(cls.clone(), vec![]);
        let cls_ty = class_ty(&cls);
        let arr = list_of(int_ty());
        let scrut = union_of(vec![cls_ty, arr]);

        let report = compute_match_usefulness(&cx, &[Pat::wildcard(scrut.clone())], scrut);
        assert!(report.missing.is_empty());
    }

    /// Recursive class with no escape (`class A { x: A }`) is uninhabited,
    /// but the cycle-protection in `is_inhabited` defaults to *inhabited*
    /// — uninhabitedness is anti-monotone. Verify the algorithm doesn't
    /// loop and treats `A` as inhabited (conservative; same as rustc).
    #[test]
    fn testing_46b_self_recursive_class_treated_as_inhabited() {
        let mut cx = TestingCtx::new();
        let a = qtn("A");
        let a_ty = class_ty(&a);
        cx.register(a.clone(), vec![a_ty.clone()]);

        // No arms — without the cycle guard would either loop or, with
        // a strict "uninhabited via cycle" rule, report exhaustive.
        // Conservative treatment: A is "inhabited," so missing is reported.
        let report = compute_match_usefulness(&cx, &[], a_ty);
        assert!(
            !report.missing.is_empty(),
            "self-recursive class is conservatively treated as inhabited"
        );
    }

    // ── 45. Type-alias expanded by ctx is fully checkable ───────────────

    /// A `Ty::TypeAlias("Foo")` where `Foo = bool`. The algorithm itself
    /// doesn't expand aliases — that's the `PatCtx` impl's job, mirroring
    /// the real builder's `expand_alias_chains`. Once expanded:
    ///   - matching with both `true` and `false` is exhaustive.
    ///   - matching with only `true` is non-exhaustive.
    ///
    /// Witness type stays the alias (the original column type), so
    /// diagnostics can render the alias name; only the *ctor enumeration*
    /// follows the alias.
    #[test]
    fn testing_45_type_alias_expansion() {
        let mut cx = TestingCtx::new();
        let foo = qtn("Foo");
        cx.register_alias(foo.clone(), bool_ty());

        let alias_ty = Ty::TypeAlias(foo.clone());

        // Both branches → exhaustive.
        let arms = vec![
            Pat::single(bool_lit(true), alias_ty.clone()),
            Pat::single(bool_lit(false), alias_ty.clone()),
        ];
        let report = compute_match_usefulness(&cx, &arms, alias_ty.clone());
        assert!(
            report.missing.is_empty(),
            "expected exhaustive once alias expands; got {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );

        // Only `true` → `false` missing.
        let arms = vec![Pat::single(bool_lit(true), alias_ty.clone())];
        let report = compute_match_usefulness(&cx, &arms, alias_ty);
        assert_eq!(report.missing.len(), 1);
        assert!(
            report.missing[0].to_string().contains("false"),
            "missing witness should mention `false`, got {}",
            report.missing[0]
        );
    }

    /// Aliasing a finite literal-union: `type Tri = 1 | 2 | 3`. Coverage of
    /// all three through the alias should be exhaustive.
    #[test]
    fn testing_45b_alias_to_literal_union() {
        let mut cx = TestingCtx::new();
        let tri = qtn("Tri");
        cx.register_alias(
            tri.clone(),
            union_of(vec![int_lit(1), int_lit(2), int_lit(3)]),
        );
        let tri_ty = Ty::TypeAlias(tri);

        let arms = vec![
            Pat::single(int_lit(1), tri_ty.clone()),
            Pat::single(int_lit(2), tri_ty.clone()),
            Pat::single(int_lit(3), tri_ty.clone()),
        ];
        let report = compute_match_usefulness(&cx, &arms, tri_ty);
        assert!(
            report.missing.is_empty(),
            "expected exhaustive: {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );
    }

    /// Aliasing a recursive type: `type Tree = Optional<Class<Tree>>`.
    /// Mainly verifying alias cycles don't infinite-loop.
    #[test]
    fn testing_45c_alias_with_recursion_terminates() {
        let mut cx = TestingCtx::new();
        let n = qtn("Node");
        let n_ty = class_ty(&n);
        let opt_n = opt_of(n_ty.clone());
        cx.register(n.clone(), vec![bool_ty(), opt_n.clone()]);
        let alias = qtn("Tree");
        cx.register_alias(alias.clone(), opt_n.clone());
        let alias_ty = Ty::TypeAlias(alias);

        let null_top = Pat::single(null_ty(), alias_ty.clone());
        let some_any = Pat::class(
            n.clone(),
            vec![Pat::wildcard(bool_ty()), Pat::wildcard(opt_n.clone())],
            n_ty.clone(),
        );
        let arms = vec![null_top, some_any];

        let report = compute_match_usefulness(&cx, &arms, alias_ty);
        assert!(
            report.missing.is_empty(),
            "expected exhaustive (null + Some(_)); got {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );
    }

    // ── 44. Variable-slice witness with trailing `..` has comma ─────────

    /// `Variable{prefix: 2, suffix: 0}` with two wildcard fields renders as
    /// `[_, _, ..]`. The trailing `..` must have a `, ` separator from the
    /// last prefix field. Exposes a Display formatting bug.
    #[test]
    fn testing_44_prefix_only_var_witness_comma_before_rest() {
        let arr = list_of(bool_ty());
        let witness = Pat::new(
            Ctor::Slice(SliceShape::Variable {
                prefix: 2,
                suffix: 0,
            }),
            vec![Pat::wildcard(bool_ty()), Pat::wildcard(bool_ty())],
            arr,
        );

        assert_eq!(witness.to_string(), "[_, _, ..]");
    }

    // ── 26. Or-pattern shared arm-id useful tracking ────────────────────

    /// Simulate `[1 | 2, _]`: pre-expand to `[1, _]` and `[2, _]` rows. Both
    /// rows useful; not unreachable. Subsequent `[3, _]` also useful.
    /// Verify per-row useful tracking aligns to ArmId.
    #[test]
    fn testing_26_or_pattern_simulated_rows() {
        let cx = TestingCtx::new();
        let arr = list_of(int_ty());
        let arm = |v: i64| {
            Pat::slice(
                SliceShape::Fixed(2),
                vec![Pat::single(int_lit(v), int_ty()), Pat::wildcard(int_ty())],
                arr.clone(),
            )
        };
        // Two rows simulating `[1 | 2, _]`.
        let arms = vec![arm(1), arm(2), arm(3), Pat::wildcard(arr.clone())];
        let report = compute_match_usefulness(&cx, &arms, arr);
        assert!(report.missing.is_empty());
        // None unreachable.
        assert!(
            report.unreachable_arms.is_empty(),
            "all rows should be reachable, got {:?}",
            report.unreachable_arms
        );
    }

    /// `class Pair { a: bool, b: bool }` with `{ a: true, b: _ }` and
    /// `{ a: false, b: true }` should report missing `{ a: false, b: false }`.
    #[test]
    fn class_pair_missing_one_combo() {
        let q = qtn("Pair");
        let pair_ty = class_ty(&q);

        let arm1 = Pat::class(
            q.clone(),
            vec![
                Pat::single(bool_lit(true), bool_ty()),
                Pat::wildcard(bool_ty()),
            ],
            pair_ty.clone(),
        );
        let arm2 = Pat::class(
            q.clone(),
            vec![
                Pat::single(bool_lit(false), bool_ty()),
                Pat::single(bool_lit(true), bool_ty()),
            ],
            pair_ty.clone(),
        );

        let mut cx = TestingCtx::new();
        cx.register(q, vec![bool_ty(), bool_ty()]);
        let report = compute_match_usefulness(&cx, &[arm1, arm2], pair_ty);
        let missing_strings: Vec<String> = report.missing.iter().map(|w| w.to_string()).collect();
        assert_eq!(
            report.missing.len(),
            1,
            "expected exactly one missing case, got {:?}",
            missing_strings
        );
        assert!(
            missing_strings[0].contains("false, false"),
            "expected `false, false` in missing, got {:?}",
            missing_strings
        );
    }
}
