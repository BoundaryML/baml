//! Pattern exhaustiveness, irrefutability, and witness construction.
//!
//! Single source of truth for "does this set of patterns cover this type" and
//! "what value would not match this pattern." Used for match-arm exhaustiveness,
//! `let` / `for` irrefutability, and unreachable-arm detection.
//!
//! Architecture is a reduced port of rustc's `rustc_pattern_analysis`:
//! - [`Ctor`] is the head of a deconstructed pattern (a value's "shape").
//! - [`DPat`] is a pattern in deconstructed form: ctor + sub-patterns + type.
//! - `Matrix` (private) is a 2-D grid of `DPat`s; rows are arms, columns are positions.
//! - The recursive *usefulness* algorithm specializes the matrix on each ctor of
//!   the leading column, recursing into sub-pattern columns. A column is
//!   exhaustive iff its ctor enumeration is fully covered by the matrix.
//! - Missing cases produce [`WitnessPat`]s — concrete values that no row matches.
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
use baml_type::{PrimitiveType, QualifiedTypeName, Ty, TyAttr};
use rustc_hash::FxHashSet;

/// Does `ty` carry an error-recovery sentinel (`Ty::Error` or `Ty::Unknown`)
/// anywhere in its structure? Inlined from TIR's `generics.rs` - the one
/// crate-internal dependency the lifted algorithm had.
fn contains_error_recovery(ty: &Ty) -> bool {
    if matches!(ty, Ty::Error { .. } | Ty::Unknown { .. }) {
        return true;
    }
    match ty {
        Ty::AssociatedTypeProjection {
            base, interface, ..
        } => contains_error_recovery(base) || interface.tys().any(contains_error_recovery),
        Ty::List(inner, _) => contains_error_recovery(inner),
        Ty::Map {
            key: k, value: v, ..
        } => contains_error_recovery(k) || contains_error_recovery(v),
        Ty::Union(tys, _) => tys.iter().any(contains_error_recovery),
        Ty::Future(value, error, _) => {
            contains_error_recovery(value) || contains_error_recovery(error)
        }
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            params
                .iter()
                .any(|param| contains_error_recovery(&param.ty))
                || contains_error_recovery(ret)
                || contains_error_recovery(throws)
        }
        Ty::Class(_, type_args, _) => type_args.iter().any(contains_error_recovery),
        Ty::Interface(_, type_args, associated_bindings, _) => {
            type_args.iter().any(contains_error_recovery)
                || associated_bindings
                    .iter()
                    .any(|(_, ty)| contains_error_recovery(ty))
        }
        _ => false,
    }
}

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

    /// Or-pattern: alternatives stored in `DPat::fields`. The arity (=
    /// number of alternatives) lives on the `DPat`, not on the ctor.
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

/// Type-argument identity for [`Ctor::Class`]: generics are invariant, so
/// `Box<T>` and `Box<int>` are distinct ctors (a rigid-arg row is a
/// possible-but-not-covering row in a `Box<int>` column, never a cover).
/// Compared per-arg via [`ty_ctor_identity`] — never raw `Ty` equality, whose
/// `TyAttr`/`Freshness` baggage would split identical ctors. Builders
/// canonicalize the args via the canonical type algebra
/// (`TypeContext::normalize`), whose guarantee makes identical spellings here
/// exactly `equivalent` on both the pattern and column sides.
fn class_args_identity_eq(a: &[Ty], b: &[Ty]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(x, y)| ty_ctor_identity(x) == ty_ctor_identity(y))
}

impl PartialEq for Ctor {
    fn eq(&self, other: &Self) -> bool {
        use Ctor::{
            Class, Interface, Missing, NonExhaustive, Or, Single, Slice, UnionMember, Wildcard,
        };
        match (self, other) {
            (Single(a), Single(b)) => ty_ctor_identity(a) == ty_ctor_identity(b),
            (Slice(a), Slice(b)) => a == b,
            (Class(a, aa), Class(b, ba)) => a == b && class_args_identity_eq(aa, ba),
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
            Class(qtn, args) => {
                qtn.hash(state);
                for arg in args {
                    ty_ctor_identity(arg).hash(state);
                }
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
    /// Arity for ctor types whose arity is determined by the ctor + ty
    /// alone. `Or`'s arity comes from the `DPat`'s `fields.len()` and is not
    /// queried through this method; callers special-case Or.
    pub fn arity(&self, ty: &Ty, cx: &dyn PatCtx) -> usize {
        match self {
            Ctor::Single(_) | Ctor::Wildcard | Ctor::NonExhaustive | Ctor::Missing | Ctor::Or => 0,
            Ctor::UnionMember(_) => 1,
            Ctor::Slice(shape) => shape.arity(),
            Ctor::Class(qtn, args) => cx
                .class_field_types(qtn, &class_ty_for_ctor(qtn, args, ty))
                .len(),
            Ctor::Interface(iface_ty) => cx.interface_field_types(iface_ty).len(),
        }
    }

    /// Does `self` cover `other` — i.e. is every value matched by `other` also
    /// matched by `self`? Equality for most ctors; slice variable-length covers
    /// fixed slices of compatible length; wildcard covers anything. `Or` is
    /// not handled here — Or rows are handled by matrix-level expansion
    /// before any normal cover check runs.
    pub fn covers(&self, other: &Ctor) -> bool {
        use Ctor::{
            Class, Interface, Missing, NonExhaustive, Or, Single, Slice, UnionMember, Wildcard,
        };
        match (self, other) {
            (Wildcard, _) => true,
            (_, Wildcard) => false,
            (Single(a), Single(b)) => ty_ctor_identity(a) == ty_ctor_identity(b),
            // Arg-sensitive, but only where decidable: a ctor pair is
            // non-covering only when the args *provably* differ. An arity
            // mismatch (a bare `Box` pattern missing its required type args)
            // or an `Unknown`/`Error` recovery arg means the pattern already
            // failed resolution at its own site — such a ctor covers like the
            // old qtn-only identity did, so coverage does not stack a
            // non-exhaustive error on top of the resolution error.
            (Class(a, aa), Class(b, ba)) => a == b && !class_args_definitely_distinct(aa, ba),
            (Interface(a), Interface(b)) => ty_ctor_identity(a) == ty_ctor_identity(b),
            (Slice(a), Slice(b)) => slice_covers(a, b),
            (UnionMember(a), UnionMember(b)) => ty_ctor_identity(a) == ty_ctor_identity(b),
            (NonExhaustive, NonExhaustive) => true,
            (Missing, Missing) => true,
            (Or, Or) => true,
            _ => false,
        }
    }
}

/// Whether two `Ctor::Class` arg lists are *provably* distinct instantiations:
/// same arity, no error-recovery arg on either side, and some position's
/// identities differ. Anything undecidable (arity mismatch, recovery arg —
/// both mean the pattern already failed resolution and carries its own
/// diagnostic) is treated as not-distinct, so `covers` stays lenient there.
fn class_args_definitely_distinct(a: &[Ty], b: &[Ty]) -> bool {
    a.len() == b.len()
        && !class_args_have_recovery(a)
        && !class_args_have_recovery(b)
        && a.iter()
            .zip(b.iter())
            .any(|(x, y)| ty_ctor_identity(x) != ty_ctor_identity(y))
}

/// Whether any of a `Ctor::Class`'s type args carries an error-recovery
/// sentinel anywhere in its structure — see the `covers` leniency above.
fn class_args_have_recovery(args: &[Ty]) -> bool {
    args.iter().any(contains_error_recovery)
}

fn class_ty_for_ctor(qtn: &QualifiedTypeName, args: &[Ty], fallback: &Ty) -> Ty {
    match fallback {
        Ty::Class(fallback_qtn, _, _) if fallback_qtn == qtn => fallback.clone(),
        _ => Ty::Class(qtn.clone(), args.to_vec(), TyAttr::default()),
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
        ) => ap + as_ <= bp + bs && (ap <= bp && as_ <= bs),
        (Fixed(_), Variable { .. }) => false,
    }
}

/// A canonicalized form of `Ty` used as the identity key for [`Ctor::Single`].
/// Strips `TyAttr` (span/comment baggage), normalizes `Ty::Literal` `Freshness`,
/// and canonicalizes float string forms (`1.0` ≡ `1.00` ≡ `1e0`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CtorIdentity(String);

/// Compute the [`CtorIdentity`] for a type. This is what `Ctor::Single` uses
/// for `Eq`/`Hash`. Two types with the same identity are the same ctor.
pub fn ty_ctor_identity(ty: &Ty) -> CtorIdentity {
    let mut s = String::new();
    write_ty_identity(&mut s, ty);
    CtorIdentity(s)
}

fn write_ty_identity(out: &mut String, ty: &Ty) {
    use std::fmt::Write;
    match ty {
        Ty::Literal(lit, _, _) => {
            out.push_str("L:");
            write_literal_identity(out, lit);
        }
        Ty::EnumVariant(qtn, name, _) => {
            let _ = write!(out, "EV:{qtn}::{name}");
        }
        Ty::Enum(qtn, _) => {
            let _ = write!(out, "E:{qtn}");
        }
        Ty::Class(qtn, args, _) => {
            let _ = write!(out, "C:{qtn}<");
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_ty_identity(out, a);
            }
            out.push('>');
        }
        Ty::Interface(qtn, args, associated_bindings, _) => {
            let _ = write!(out, "I:{qtn}<");
            let mut wrote_any = false;
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_ty_identity(out, a);
                wrote_any = true;
            }
            for (name, ty) in associated_bindings {
                if wrote_any {
                    out.push(',');
                }
                let _ = write!(out, "{name}=");
                write_ty_identity(out, ty);
                wrote_any = true;
            }
            out.push('>');
        }
        Ty::AssociatedTypeProjection {
            base,
            interface,
            member,
            ..
        } => {
            out.push_str("Assoc<");
            write_ty_identity(out, base);
            out.push_str(" as ");
            write_ty_identity(out, &interface.to_ty());
            let _ = write!(out, ".{member}>");
        }
        Ty::Int { .. } => out.push_str("P:Int"),
        Ty::Bigint { .. } => out.push_str("P:Bigint"),
        Ty::Float { .. } => out.push_str("P:Float"),
        Ty::String { .. } => out.push_str("P:String"),
        Ty::Bool { .. } => out.push_str("P:Bool"),
        Ty::Null { .. } => out.push_str("P:Null"),
        Ty::Uint8Array { .. } => out.push_str("P:Uint8Array"),
        Ty::Media(kind, _) => {
            let _ = write!(out, "P:{kind:?}");
        }
        Ty::Union(members, _) => {
            out.push_str("U:[");
            for (i, m) in members.iter().enumerate() {
                if i > 0 {
                    out.push('|');
                }
                write_ty_identity(out, m);
            }
            out.push(']');
        }
        Ty::List(elem, _) => {
            out.push_str("Lst:");
            write_ty_identity(out, elem);
        }
        Ty::Map {
            key: k, value: v, ..
        } => {
            out.push_str("M:");
            write_ty_identity(out, k);
            out.push(',');
            write_ty_identity(out, v);
        }
        Ty::TypeAlias(qtn, _) => {
            let _ = write!(out, "A:{qtn}");
        }
        Ty::TypeVar(name, _) => {
            let _ = write!(out, "V:{name}");
        }
        Ty::Never { .. } => out.push_str("Never"),
        Ty::Void { .. } => out.push_str("Void"),
        Ty::BuiltinUnknown { .. } => out.push_str("BUnk"),
        Ty::Unknown { .. } => out.push_str("Unk"),
        Ty::Error { .. } => out.push_str("Err"),
        Ty::Infer { .. } => out.push_str("Inf"),
        Ty::RustType { .. } => out.push_str("Rust"),
        Ty::Type { .. } => out.push_str("Type"),
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
        Ty::Future(value, error, _) => {
            out.push_str("Fut<");
            write_ty_identity(out, value);
            out.push(',');
            write_ty_identity(out, error);
            out.push('>');
        }
        // `Resource`/`PromptAst` are never produced by TIR; the
        // arms exist only so the match stays exhaustive over the shared
        // `baml_type::Ty`.
        Ty::Resource { .. } => out.push_str("Res"),
        Ty::PromptAst { .. } => out.push_str("PAst"),
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

// ── Deconstructed pattern ────────────────────────────────────────────────────

/// A pattern in the form the usefulness algorithm consumes. `fields.len() ==
/// arity`. Wildcards are filled in for elided positions during lowering.
#[derive(Debug, Clone)]
pub struct DPat {
    pub ctor: Ctor,
    pub fields: Vec<DPat>,
    pub arity: usize,
    pub ty: Ty,
}

impl DPat {
    pub fn wildcard(ty: Ty) -> Self {
        Self {
            ctor: Ctor::Wildcard,
            fields: vec![],
            arity: 0,
            ty,
        }
    }
    pub fn single(ty: Ty, scrutinee_ty: Ty) -> Self {
        Self {
            ctor: Ctor::Single(ty),
            fields: vec![],
            arity: 0,
            ty: scrutinee_ty,
        }
    }
    pub fn class(qtn: QualifiedTypeName, fields: Vec<DPat>, ty: Ty) -> Self {
        Self {
            ctor: Ctor::Class(qtn, Vec::new()),
            arity: fields.len(),
            fields,
            ty,
        }
    }
    pub fn class_inst(qtn: QualifiedTypeName, args: Vec<Ty>, fields: Vec<DPat>, ty: Ty) -> Self {
        Self {
            ctor: Ctor::Class(qtn, args),
            arity: fields.len(),
            fields,
            ty,
        }
    }
    pub fn interface(iface_ty: Ty, fields: Vec<DPat>, ty: Ty) -> Self {
        Self {
            ctor: Ctor::Interface(iface_ty),
            arity: fields.len(),
            fields,
            ty,
        }
    }
    pub fn slice(shape: SliceShape, fields: Vec<DPat>, ty: Ty) -> Self {
        debug_assert_eq!(shape.arity(), fields.len());
        Self {
            ctor: Ctor::Slice(shape),
            arity: fields.len(),
            fields,
            ty,
        }
    }
    /// Or-pattern: `pat1 | pat2 | ...`. Alternatives are stored in `fields`
    /// and must each have type `ty`. At least 2 alternatives required by
    /// convention (one alt is just that alt directly).
    pub fn or(alts: Vec<DPat>, ty: Ty) -> Self {
        debug_assert!(alts.len() >= 2, "Or pattern needs ≥2 alternatives");
        Self {
            ctor: Ctor::Or,
            arity: alts.len(),
            fields: alts,
            ty,
        }
    }
    /// Union-member tag: marks the pattern as targeting one specific
    /// member of a union scrutinee. The single sub-pat is the
    /// pattern as it would have been against the member type directly.
    pub fn union_member(member_ty: Ty, inner: DPat, scrut_ty: Ty) -> Self {
        Self {
            ctor: Ctor::UnionMember(member_ty),
            arity: 1,
            fields: vec![inner],
            ty: scrut_ty,
        }
    }
}

// ── Witness pattern ──────────────────────────────────────────────────────────

/// A "missing-case" witness: a concrete pattern shape that proves a match is
/// non-exhaustive (or an irrefutable context is refutable). Same shape as
/// [`DPat`] but produced by the algorithm rather than user input.
#[derive(Debug, Clone)]
pub struct WitnessPat {
    pub ctor: Ctor,
    pub fields: Vec<WitnessPat>,
    pub ty: Ty,
}

impl WitnessPat {
    pub fn wildcard(ty: Ty) -> Self {
        Self {
            ctor: Ctor::Wildcard,
            fields: vec![],
            ty,
        }
    }
    pub fn new(ctor: Ctor, fields: Vec<WitnessPat>, ty: Ty) -> Self {
        Self { ctor, fields, ty }
    }
}

/// Render a missing-case witness for the E0062 message: class witnesses
/// carry their field NAMES (declaration order via ppir), union members
/// cascade so nested classes keep labels, everything else takes the
/// standard `Display`.
pub fn render_witness_pat(db: &dyn baml_compiler2_ppir::Db, w: &WitnessPat) -> String {
    use std::fmt::Write as _;
    match &w.ctor {
        Ctor::Class(qtn, args) => {
            let names: Vec<baml_type::Name> = {
                let package =
                    baml_compiler2_hir::package::PackageId::new(db, qtn.package().clone());
                match baml_compiler2_ppir::package_items(db, package)
                    .lookup_type(qtn.namespace(), qtn.name())
                {
                    Some(baml_compiler2_hir::contributions::Definition::Class(class_loc)) => {
                        baml_compiler2_ppir::item_data::class_data(db, class_loc)
                            .fields
                            .iter()
                            .map(|f| f.name.clone())
                            .collect()
                    }
                    _ => Vec::new(),
                }
            };
            let qtn_str = class_witness_head(qtn, args);
            if w.fields.is_empty() {
                return format!("{qtn_str} {{}}");
            }
            let mut out = format!("{qtn_str} {{ ");
            for (i, fld) in w.fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                let rendered = render_witness_pat(db, fld);
                if let Some(name) = names.get(i) {
                    let _ = write!(out, "{name}: {rendered}");
                } else {
                    out.push_str(&rendered);
                }
            }
            out.push_str(" }");
            out
        }
        Ctor::UnionMember(_) => match w.fields.first() {
            Some(inner) => {
                let s = render_witness_pat(db, inner);
                if matches!(s.as_str(), "_") {
                    w.to_string()
                } else {
                    s
                }
            }
            None => w.to_string(),
        },
        _ => w.to_string(),
    }
}

/// The head of a class witness: the class name, plus its type arguments when it
/// has any.
///
/// The arguments are part of the ctor's identity — in a generic or opaque
/// column each instantiation present is its own alphabet branch, so two of them
/// can each produce a witness. Omitting the arguments would render those two
/// identically and tell the reader to add one arm where two distinct ones are
/// missing. A non-generic class renders as the bare name, exactly as before.
pub(crate) fn class_witness_head(qtn: &QualifiedTypeName, args: &[Ty]) -> String {
    let name = qtn.render_user_facing();
    if args.is_empty() {
        return name;
    }
    let args = args
        .iter()
        .map(Ty::render_user_facing)
        .collect::<Vec<_>>()
        .join(", ");
    format!("{name}<{args}>")
}

impl fmt::Display for WitnessPat {
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
            Ctor::Class(qtn, args) => {
                let qtn = class_witness_head(qtn, args);
                if self.fields.is_empty() {
                    return write!(f, "{qtn} {{}}");
                }
                write!(f, "{qtn} {{ ")?;
                for (i, fld) in self.fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{fld}")?;
                }
                write!(f, " }}")
            }
            Ctor::Interface(ty) => {
                let ty = ty.render_user_facing();
                if self.fields.is_empty() {
                    return write!(f, "{ty} {{}}");
                }
                write!(f, "{ty} {{ ")?;
                for (i, fld) in self.fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{fld}")?;
                }
                write!(f, " }}")
            }
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

fn write_single_witness(f: &mut fmt::Formatter<'_>, ty: &Ty) -> fmt::Result {
    match ty {
        Ty::Literal(lit, _, _) => match lit {
            Literal::Int(v) => write!(f, "{v}"),
            Literal::Bigint(v) => write!(f, "{v}n"),
            Literal::Bool(v) => write!(f, "{v}"),
            Literal::String(v) => write!(f, "{v:?}"),
            Literal::Float(s) => write!(f, "{s}"),
        },
        Ty::EnumVariant(qtn, variant, _) => write!(f, "{qtn}.{variant}"),
        Ty::Null { .. } => write!(f, "null"),
        _ => write!(f, "{ty:?}"),
    }
}

/// Render a `UnionMember` witness's member type when the inner pat is a
/// placeholder (no concrete value to print). Surfaces the member's runtime
/// shape — `int`, `string`, `Foo`, etc. — rather than `_`.
fn write_member_ty_witness(f: &mut fmt::Formatter<'_>, ty: &Ty) -> fmt::Result {
    match ty {
        Ty::Int { .. } => write!(f, "{}", PrimitiveType::Int),
        Ty::Bigint { .. } => write!(f, "{}", PrimitiveType::Bigint),
        Ty::Float { .. } => write!(f, "{}", PrimitiveType::Float),
        Ty::String { .. } => write!(f, "{}", PrimitiveType::String),
        Ty::Bool { .. } => write!(f, "{}", PrimitiveType::Bool),
        Ty::Null { .. } => write!(f, "{}", PrimitiveType::Null),
        Ty::Uint8Array { .. } => write!(f, "{}", PrimitiveType::Uint8Array),
        Ty::Media(kind, _) => write!(f, "{kind}"),
        // Interfaces are open-world, so a union-member witness names the
        // uncovered interface (`Animal`, `Slot<int>`) rather than collapsing to
        // a bare `_`. Rendered user-facing (no `user.` package prefix).
        Ty::Interface(_, _, _, _) => write!(f, "{}", ty.render_user_facing()),
        Ty::Class(qtn, _, _) | Ty::Enum(qtn, _) => write!(f, "{}", qtn.render_user_facing()),
        Ty::EnumVariant(qtn, variant, _) => write!(f, "{}.{variant}", qtn.render_user_facing()),
        Ty::Literal(_, _, _) => write_single_witness(f, ty),
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

    /// Whether an interface-pattern ctor covers every value in an interface
    /// column. A narrower associated-type binding (for example
    /// `Iterator<Item = int>` against a plain `Iterator`) must not make an
    /// open interface column exhaustive.
    fn interface_ctor_covers_column(&self, iface_ty: &Ty, col_ty: &Ty) -> bool {
        ty_ctor_identity(iface_ty) == ty_ctor_identity(col_ty)
    }

    /// For a slice ctor at column type `ty` (an array/list type), return the
    /// sub-pattern types — which is just the element type repeated `arity`
    /// times.
    fn slice_field_types(&self, shape: &SliceShape, ty: &Ty) -> Vec<Ty> {
        let elem = self.list_element_type(ty);
        vec![elem; shape.arity()]
    }

    /// The element type of an array/list type. Helper for default
    /// [`PatCtx::slice_field_types`].
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
        Ty::Never { .. } => false,
        Ty::Class(qtn, _, _) => {
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
        Ty::Union(members, _) => members.iter().any(|m| is_inhabited_default(m, cx, seen)),
        // `T[]` always inhabits `[]`, so it is inhabited regardless of T.
        Ty::List(_, _) => true,
        _ => true,
    }
}

// ── Matrix and rows ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Row<'p> {
    pats: Vec<&'p DPat>,
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
    fn new(arms: &'p [DPat], scrut_ty: Ty) -> Self {
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
        wild_pad: &'a [DPat],
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
                            let mut pats: Vec<&DPat> = vec![alt];
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
                    let mut pats: Vec<&DPat> = (0..arity).map(|i| &wild_pad[i]).collect();
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
                    let mut pats: Vec<&DPat> = (0..arity).map(|i| &wild_pad[i]).collect();
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
    pub missing: Vec<WitnessPat>,
    /// Indices of arms that are unreachable (no value matches them that wasn't
    /// already matched by an earlier arm).
    pub unreachable_arms: Vec<ArmId>,
}

/// Compute exhaustiveness and per-arm reachability.
///
/// `arms` is one `DPat` per source arm. Or-patterns are represented as
/// `Ctor::Or` nodes and expanded by the algorithm during specialization;
/// usefulness aggregates back to the original source arm via shared
/// `ArmId`, so an or-pattern arm is "useful" if any of its alternatives is.
pub fn compute_match_usefulness(cx: &dyn PatCtx, arms: &[DPat], scrut_ty: Ty) -> UsefulnessReport {
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

    // Witnesses come only from alphabet branches (reachability-only branches
    // are excluded in `compute_exhaustiveness`), whose ctors are disjoint and
    // pre-deduplicated by `Ctor` identity — so no rendered-form
    // de-duplication is needed here, and distinct witnesses are never silently
    // merged.
    //
    // That rests on rendering being faithful to ctor identity. It is not
    // automatic: an infinite-alphabet column (a generic or opaque scrutinee)
    // makes *every* present ctor its own branch, so `Box<int>` and
    // `Box<string>` are two branches that can each emit a witness, and a
    // renderer that dropped the type arguments would print the same text
    // twice. `class_witness_head` keeps them, which is what lets this hold.
    let missing: Vec<WitnessPat> = witness_matrix.into_single_column();

    UsefulnessReport {
        missing,
        unreachable_arms,
    }
}

/// A column-stack of witness patterns, one entry per remaining matrix column.
/// Built up during recursion as ctors are unspecialized back onto the witness.
#[derive(Debug, Clone)]
struct WitnessStack(Vec<WitnessPat>);

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
        let fields: Vec<WitnessPat> = self.0.drain((len - arity)..).rev().collect();
        self.0
            .push(WitnessPat::new(ctor.clone(), fields, ty.clone()));
        self
    }
    /// Push a fresh pattern on top (used to introduce wildcards when applying
    /// the synthetic `Missing` ctor).
    fn push(&mut self, pat: WitnessPat) {
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
            let fields: Vec<WitnessPat> = field_tys.into_iter().map(WitnessPat::wildcard).collect();
            let pat = WitnessPat::new(ctor.clone(), fields, ty.clone());
            for stack in &original {
                let mut new_stack = stack.clone();
                new_stack.push(pat.clone());
                self.0.push(new_stack);
            }
        }
    }
    fn into_single_column(self) -> Vec<WitnessPat> {
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
    let CtorSplit {
        split,
        missing: missing_in_matrix,
        reachability_only,
    } = split_ctors(cx, &col_ty, matrix);

    for (ctor, in_alphabet) in split
        .iter()
        .map(|c| (c, true))
        .chain(reachability_only.iter().map(|c| (c, false)))
    {
        let sub_tys = ctor_sub_tys(cx, ctor, &col_ty);
        let wild_pad: Vec<DPat> = sub_tys.iter().cloned().map(DPat::wildcard).collect();

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

        if !in_alphabet {
            // Reachability-only branch: its rows were marked useful above, but
            // its witnesses are dropped. The branch's value set is a subset of
            // the alphabet's, so the alphabet branches already report every
            // true missing case — while this branch excludes covering rows
            // whose ctor doesn't `covers`-join it (an arg-mismatched class
            // ctor, a slice row against a rigid `Single` head) and would
            // report their values as false missing cases.
            debug_assert!(
                !matches!(ctor, Ctor::Missing | Ctor::Or | Ctor::Wildcard),
                "synthetic ctors are never reachability-only"
            );
            continue;
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
            let arity = ctor.arity(&col_ty, cx);
            sub_witnesses.apply_ctor(ctor, arity, &col_ty);
        }
        witnesses.extend(sub_witnesses);
    }
}

/// The outcome of [`split_ctors`]: which ctors to specialize on, partitioned
/// by how each branch participates in the exhaustiveness verdict.
struct CtorSplit {
    /// Alphabet branches: together they cover the column's entire value set.
    /// Missing-case witnesses propagate only from these. A synthetic
    /// `Missing` ctor stands in for ctors absent from the matrix (when no
    /// wildcard arm is present), so the recursion through it produces a
    /// missing-case witness for each.
    split: Vec<Ctor>,
    /// The actual list of absent alphabet ctors — used to expand the
    /// synthetic `Missing` ctor when applying it to a witness.
    missing: Vec<Ctor>,
    /// Present-but-outside-the-alphabet branches: possible-but-not-covering
    /// rows, i.e. a rigid `Single` or an arg-mismatched `Class(Box,[T])` in a
    /// `Box<int>` column, or a non-slice head in a list column. They are
    /// specialized only so their rows are marked reachable. Their value sets
    /// are subsets of the alphabet's, so the alphabet branches already
    /// produce every true witness; a witness from one of these branches is at
    /// best a duplicate, and — when the actual covering row does not
    /// `covers`-join the branch, as an arg-mismatched class ctor or a slice
    /// row never does — a false non-exhaustive.
    reachability_only: Vec<Ctor>,
}

impl CtorSplit {
    /// A split whose branches are all alphabet branches.
    fn alphabet(split: Vec<Ctor>, missing: Vec<Ctor>) -> Self {
        Self {
            split,
            missing,
            reachability_only: vec![],
        }
    }
}

/// Decide which ctors to specialize on. See [`CtorSplit`] for how the
/// returned branches participate in the verdict.
fn split_ctors(cx: &dyn PatCtx, col_ty: &Ty, matrix: &Matrix<'_>) -> CtorSplit {
    // If any row has an Or-pattern at column 0, force expansion via the
    // `Or` ctor before any other split logic. Or-rows can't participate in
    // a normal `head.ctor.covers(ctor)` check; they need to explode first.
    if matrix
        .rows
        .iter()
        .any(|row| matches!(&row.pats[0].ctor, Ctor::Or))
    {
        return CtorSplit::alphabet(vec![Ctor::Or], vec![]);
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
            return CtorSplit::alphabet(vec![], vec![]);
        }
        // Lists: a single open-ended `[..]` witness covers "any list".
        // Check this *before* `enumerate_ctors` because List's `enumerate`
        // returns empty (slice splitting normally handles it). Without
        // this short-circuit, the empty result would incorrectly mark the
        // list as vacuously exhaustive.
        if matches!(col_ty, Ty::List(_, _)) {
            return CtorSplit::alphabet(
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
            return CtorSplit::alphabet(vec![], vec![]);
        }
        let missing = if all.iter().any(|c| matches!(c, Ctor::NonExhaustive)) {
            vec![Ctor::NonExhaustive]
        } else {
            dedup_ctors(all)
        };
        return CtorSplit::alphabet(vec![Ctor::Missing], missing);
    }

    let present: Vec<Ctor> = collect_column_ctors(matrix);
    let has_wildcard = present.iter().any(|c| matches!(c, Ctor::Wildcard));
    let present_no_wild: Vec<Ctor> = present
        .into_iter()
        .filter(|c| !matches!(c, Ctor::Wildcard))
        .collect();

    // Slice types need a special split that treats variable-length patterns
    // as covering open-ended length classes — set membership isn't enough.
    if matches!(col_ty, Ty::List(_, _)) {
        return split_slice_ctors(&present_no_wild, has_wildcard);
    }

    let all = cx.enumerate_ctors(col_ty);

    if all.iter().any(|c| matches!(c, Ctor::NonExhaustive)) {
        if matches!(col_ty, Ty::Interface(..))
            && present_no_wild.iter().any(|c| {
                matches!(c, Ctor::Interface(iface_ty) if cx.interface_ctor_covers_column(iface_ty, col_ty))
            })
        {
            // Present ctors are the branches here; class branches keep their
            // covering interface rows through `interface_field_projection_for_class`
            // joins in `specialize`, so they stay verdict-bearing.
            return CtorSplit::alphabet(present_no_wild, vec![]);
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
        CtorSplit::alphabet(split, missing)
    } else if all.is_empty() {
        // Vacuously exhaustive (e.g., Never).
        CtorSplit::alphabet(vec![], vec![])
    } else {
        // Finite alphabet of pairwise-disjoint ctors (singletons, classes).
        if present_no_wild.is_empty() && has_wildcard {
            // All rows are pure wildcards at this column. Don't iterate
            // ctors — that would recurse forever on recursive types
            // (`class Node { next: Optional<Node> }`). Pass through with a
            // synthetic ctor that drops the column.
            return CtorSplit::alphabet(vec![Ctor::NonExhaustive], vec![]);
        }
        // Iterate over *all* ctors of the type. Wildcard rows extend to
        // each; missing ctors recurse into an empty sub-matrix and surface
        // as witnesses naturally. Dedup `all` first — `enumerate_ctors`
        // can produce duplicates (e.g. `Optional<Optional<T>>` pushes a
        // `Single(null)` per Optional layer).
        let all = dedup_ctors(all);
        // Membership by `covers`, not equality: a present ctor with
        // error-recovery args (`Box` with unresolved type args) covers its
        // column ctor without being identity-equal to it, and must not
        // resurface the ctor as missing.
        let missing: Vec<Ctor> = all
            .iter()
            .filter(|c| !present_no_wild.iter().any(|p| p.covers(c)))
            .cloned()
            .collect();
        // Possible-but-not-covering rows — a rigid `Single` (`let t: T` in a
        // `bool` column) or an arg-mismatched `Class` (`Box<T>` in a `Box<int>`
        // column) — are outside the column's alphabet, so they need their own
        // split branch: without one they are never specialized and would be
        // falsely flagged unreachable. The branch is reachability-only — it
        // cannot manufacture coverage (only its own rows plus wildcards join
        // it) and must not manufacture witnesses (see
        // `CtorSplit::reachability_only`); `missing` stays computed against
        // `all` alone.
        let all_set: FxHashSet<Ctor> = all.iter().cloned().collect();
        let reachability_only: Vec<Ctor> = present_no_wild
            .into_iter()
            .filter(|c| !all_set.contains(c))
            .collect();
        CtorSplit {
            split: all,
            missing,
            reachability_only,
        }
    }
}

/// Slice-specific split. Mirrors rustc's `Slice::split`: given the slice
/// patterns present in the column, partition the universe of array lengths
/// into a finite set of `Fixed` lengths plus one open-ended `Variable` that
/// covers all longer lengths. Each output slice is tagged as seen/unseen by
/// the column.
fn split_slice_ctors(present: &[Ctor], _has_wildcard: bool) -> CtorSplit {
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

    // Non-slice heads in a list column — a rigid-carrying `Single` type-pattern
    // row like `let t: T[]` in a `string[]` column — cover no length class, but
    // still need a split branch of their own: without one they are never
    // specialized and would be falsely flagged unreachable. The branch is
    // reachability-only: a slice row never `covers`-joins a `Single` head's
    // branch, so a witness from it (reachable through refutable sibling
    // columns) would fabricate a missing case the slice rows actually cover
    // (see `CtorSplit::reachability_only`).
    let reachability_only: Vec<Ctor> = present
        .iter()
        .filter(|c| !matches!(c, Ctor::Slice(_)))
        .cloned()
        .collect();

    if !missing.is_empty() {
        split.push(Ctor::Missing);
    }
    CtorSplit {
        split,
        missing,
        reachability_only,
    }
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
        Ctor::Slice(shape) => cx.slice_field_types(shape, col_ty),
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
    use baml_type::Freshness;

    use super::*;

    /// A trivial `PatCtx` for unit tests. No real class/list lookup; tests
    /// either avoid those types or stub them.
    struct StubCtx;

    impl PatCtx for StubCtx {
        fn enumerate_ctors(&self, ty: &Ty) -> Vec<Ctor> {
            match ty {
                Ty::Bool { .. } => {
                    vec![Ctor::Single(bool_lit(true)), Ctor::Single(bool_lit(false))]
                }
                Ty::Int { .. } | Ty::Float { .. } | Ty::String { .. } => vec![Ctor::NonExhaustive],
                Ty::Null { .. } => vec![Ctor::Single(ty.clone())],
                Ty::Union(members, _) => members
                    .iter()
                    .flat_map(|m| self.enumerate_ctors(m))
                    .collect(),
                Ty::Literal(_, _, _) | Ty::EnumVariant(_, _, _) => vec![Ctor::Single(ty.clone())],
                Ty::Never { .. } => vec![],
                Ty::TypeVar(_, _) => vec![Ctor::NonExhaustive],
                _ => vec![Ctor::NonExhaustive],
            }
        }

        fn class_field_types(&self, _qtn: &QualifiedTypeName, _ty: &Ty) -> Vec<Ty> {
            vec![]
        }

        fn list_element_type(&self, ty: &Ty) -> Ty {
            match ty {
                Ty::List(elem, _) => (**elem).clone(),
                _ => ty.clone(),
            }
        }
    }

    fn bool_lit(v: bool) -> Ty {
        Ty::Literal(Literal::Bool(v), Freshness::Regular, Default::default())
    }
    fn int_lit(v: i64) -> Ty {
        Ty::Literal(Literal::Int(v), Freshness::Regular, Default::default())
    }
    fn float_lit(s: &str) -> Ty {
        Ty::Literal(
            Literal::Float(s.into()),
            Freshness::Regular,
            Default::default(),
        )
    }
    fn bool_ty() -> Ty {
        Ty::Bool {
            attr: Default::default(),
        }
    }
    fn int_ty() -> Ty {
        Ty::Int {
            attr: Default::default(),
        }
    }

    #[test]
    fn bool_exhaustive_with_both_arms() {
        let arms = vec![
            DPat::single(bool_lit(true), bool_ty()),
            DPat::single(bool_lit(false), bool_ty()),
        ];
        let report = compute_match_usefulness(&StubCtx, &arms, bool_ty());
        assert!(
            report.missing.is_empty(),
            "expected exhaustive: {:?}",
            report.missing
        );
        assert!(report.unreachable_arms.is_empty());
    }

    #[test]
    fn bool_non_exhaustive_missing_false() {
        let arms = vec![DPat::single(bool_lit(true), bool_ty())];
        let report = compute_match_usefulness(&StubCtx, &arms, bool_ty());
        assert_eq!(report.missing.len(), 1);
        assert!(matches!(report.missing[0].ctor, Ctor::Single(_)));
    }

    #[test]
    fn bool_wildcard_makes_exhaustive() {
        let arms = vec![
            DPat::single(bool_lit(true), bool_ty()),
            DPat::wildcard(bool_ty()),
        ];
        let report = compute_match_usefulness(&StubCtx, &arms, bool_ty());
        assert!(report.missing.is_empty());
    }

    #[test]
    fn int_requires_wildcard() {
        let arms = vec![DPat::single(int_lit(1), int_ty())];
        let report = compute_match_usefulness(&StubCtx, &arms, int_ty());
        assert_eq!(report.missing.len(), 1);
    }

    #[test]
    fn int_with_wildcard_is_exhaustive() {
        let arms = vec![DPat::single(int_lit(1), int_ty()), DPat::wildcard(int_ty())];
        let report = compute_match_usefulness(&StubCtx, &arms, int_ty());
        assert!(report.missing.is_empty());
    }

    #[test]
    fn unreachable_after_wildcard() {
        let arms = vec![
            DPat::wildcard(bool_ty()),
            DPat::single(bool_lit(true), bool_ty()),
        ];
        let report = compute_match_usefulness(&StubCtx, &arms, bool_ty());
        assert!(report.missing.is_empty());
        assert!(
            report.unreachable_arms.contains(&ArmId(1)),
            "second arm should be unreachable, got {:?}",
            report.unreachable_arms
        );
    }

    #[test]
    fn typevar_requires_wildcard() {
        let tv = Ty::type_var("T");
        let arms = vec![DPat::wildcard(tv.clone())];
        let report = compute_match_usefulness(&StubCtx, &arms, tv);
        assert!(report.missing.is_empty());
    }

    #[test]
    fn never_is_vacuously_exhaustive() {
        let never = Ty::Never {
            attr: Default::default(),
        };
        let arms: Vec<DPat> = vec![];
        let report = compute_match_usefulness(&StubCtx, &arms, never);
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
        let array_bool = Ty::List(Box::new(bool_ty()), Default::default());

        let arm1 = DPat::slice(
            SliceShape::Fixed(2),
            vec![
                DPat::single(bool_lit(true), bool_ty()),
                DPat::wildcard(bool_ty()),
            ],
            array_bool.clone(),
        );
        let arm2 = DPat::slice(
            SliceShape::Fixed(2),
            vec![
                DPat::single(bool_lit(false), bool_ty()),
                DPat::single(bool_lit(true), bool_ty()),
            ],
            array_bool.clone(),
        );

        // The stub must enumerate Array<bool> ctors. We override below.
        struct ArrayCtx;
        impl PatCtx for ArrayCtx {
            fn enumerate_ctors(&self, ty: &Ty) -> Vec<Ctor> {
                match ty {
                    Ty::Bool { .. } => vec![
                        Ctor::Single(Ty::Literal(
                            Literal::Bool(true),
                            Freshness::Regular,
                            Default::default(),
                        )),
                        Ctor::Single(Ty::Literal(
                            Literal::Bool(false),
                            Freshness::Regular,
                            Default::default(),
                        )),
                    ],
                    Ty::List(_, _) => {
                        // Enumerate length-0..=N for tests; rely on Variable as catchall.
                        let mut out = Vec::new();
                        for n in 0..=3 {
                            out.push(Ctor::Slice(SliceShape::Fixed(n)));
                        }
                        out.push(Ctor::Slice(SliceShape::Variable {
                            prefix: 0,
                            suffix: 0,
                        }));
                        out
                    }
                    Ty::Literal(_, _, _) => vec![Ctor::Single(ty.clone())],
                    _ => vec![Ctor::NonExhaustive],
                }
            }
            fn class_field_types(&self, _q: &QualifiedTypeName, _t: &Ty) -> Vec<Ty> {
                vec![]
            }
            fn list_element_type(&self, ty: &Ty) -> Ty {
                match ty {
                    Ty::List(e, _) => (**e).clone(),
                    _ => ty.clone(),
                }
            }
        }

        let report = compute_match_usefulness(&ArrayCtx, &[arm1, arm2], array_bool);
        // Many length-classes are still missing (length 0, 1, 3, variable),
        // but `[false, false]` must be among them.
        let missing_strings: Vec<String> = report.missing.iter().map(|w| w.to_string()).collect();
        assert!(
            missing_strings.iter().any(|s| s.contains("false, false")),
            "expected `[false, false]` in missing, got {:?}",
            missing_strings
        );
    }

    /// `Array<bool>` with `[]`, `[_]`, `[_, _, ..]` should be exhaustive
    /// over all lengths.
    #[test]
    fn array_rest_covers_all_lengths() {
        let array_bool = Ty::List(Box::new(bool_ty()), Default::default());

        let arms = vec![
            DPat::slice(SliceShape::Fixed(0), vec![], array_bool.clone()),
            DPat::slice(
                SliceShape::Fixed(1),
                vec![DPat::wildcard(bool_ty())],
                array_bool.clone(),
            ),
            DPat::slice(
                SliceShape::Variable {
                    prefix: 2,
                    suffix: 0,
                },
                vec![DPat::wildcard(bool_ty()), DPat::wildcard(bool_ty())],
                array_bool.clone(),
            ),
        ];

        struct ArrayCtx;
        impl PatCtx for ArrayCtx {
            fn enumerate_ctors(&self, ty: &Ty) -> Vec<Ctor> {
                match ty {
                    Ty::Bool { .. } => vec![
                        Ctor::Single(Ty::Literal(
                            Literal::Bool(true),
                            Freshness::Regular,
                            Default::default(),
                        )),
                        Ctor::Single(Ty::Literal(
                            Literal::Bool(false),
                            Freshness::Regular,
                            Default::default(),
                        )),
                    ],
                    Ty::List(_, _) => {
                        // Enumerate the length classes appearing in the
                        // matrix plus a variable catchall.
                        vec![
                            Ctor::Slice(SliceShape::Fixed(0)),
                            Ctor::Slice(SliceShape::Fixed(1)),
                            Ctor::Slice(SliceShape::Variable {
                                prefix: 2,
                                suffix: 0,
                            }),
                        ]
                    }
                    Ty::Literal(_, _, _) => vec![Ctor::Single(ty.clone())],
                    _ => vec![Ctor::NonExhaustive],
                }
            }
            fn class_field_types(&self, _q: &QualifiedTypeName, _t: &Ty) -> Vec<Ty> {
                vec![]
            }
            fn list_element_type(&self, ty: &Ty) -> Ty {
                match ty {
                    Ty::List(e, _) => (**e).clone(),
                    _ => ty.clone(),
                }
            }
        }

        let report = compute_match_usefulness(&ArrayCtx, &arms, array_bool);
        assert!(
            report.missing.is_empty(),
            "expected exhaustive, got missing {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );
    }

    // ── testing-test PatCtx ─────────────────────────────────────────────
    //
    // A reusable test ctx that supports a class registry. Every test below
    // builds on this. Classes are registered by name with their field types
    // in declaration order. `Optional<T>` is enumerated as inner-ctors + null;
    // `List<T>` returns NonExhaustive in `enumerate_ctors` because slice
    // splitting is handled in `split_ctors` (which special-cases List).

    struct TestingCtx {
        classes: std::collections::HashMap<QualifiedTypeName, Vec<Ty>>,
        /// Type alias map: `Ty::TypeAlias(qtn)` resolves to the target Ty.
        /// Mirrors the real builder's `expand_alias_chains` behaviour.
        aliases: std::collections::HashMap<QualifiedTypeName, Ty>,
    }
    impl TestingCtx {
        fn new() -> Self {
            Self {
                classes: std::collections::HashMap::new(),
                aliases: std::collections::HashMap::new(),
            }
        }
        fn register(&mut self, qtn: QualifiedTypeName, fields: Vec<Ty>) {
            self.classes.insert(qtn, fields);
        }
        fn register_alias(&mut self, qtn: QualifiedTypeName, target: Ty) {
            self.aliases.insert(qtn, target);
        }
        /// Walk through `Ty::TypeAlias` chains to a non-alias type. Cycles
        /// fall through to the original (caller treats as opaque).
        fn expand_alias(&self, ty: &Ty) -> Ty {
            let mut current = ty.clone();
            let mut seen: std::collections::HashSet<QualifiedTypeName> =
                std::collections::HashSet::new();
            while let Ty::TypeAlias(qtn, _) = &current {
                if !seen.insert(qtn.clone()) {
                    return current;
                }
                match self.aliases.get(qtn) {
                    Some(target) => current = target.clone(),
                    None => return current,
                }
            }
            current
        }
    }
    impl PatCtx for TestingCtx {
        fn enumerate_ctors(&self, ty: &Ty) -> Vec<Ctor> {
            // Peel aliases first, the same way the real builder will.
            let ty = self.expand_alias(ty);
            match &ty {
                Ty::Bool { .. } => {
                    vec![Ctor::Single(bool_lit(true)), Ctor::Single(bool_lit(false))]
                }
                Ty::Int { .. } | Ty::Float { .. } | Ty::String { .. } => vec![Ctor::NonExhaustive],
                Ty::Null { .. } => vec![Ctor::Single(ty.clone())],
                Ty::Union(members, _) => members
                    .iter()
                    .flat_map(|m| self.enumerate_ctors(m))
                    .collect(),
                Ty::Literal(_, _, _) | Ty::EnumVariant(_, _, _) => {
                    vec![Ctor::Single(ty.clone())]
                }
                Ty::Class(qtn, args, _) => vec![Ctor::Class(qtn.clone(), args.clone())],
                // For slices, split_ctors handles enumeration via slice splitting;
                // returning NonExhaustive here is OK because the slice path is taken
                // before this is consulted.
                Ty::List(_, _) => vec![Ctor::NonExhaustive],
                Ty::Never { .. } => vec![],
                Ty::TypeVar(_, _) => vec![Ctor::NonExhaustive],
                _ => vec![Ctor::NonExhaustive],
            }
        }
        fn class_field_types(&self, qtn: &QualifiedTypeName, _ty: &Ty) -> Vec<Ty> {
            self.classes.get(qtn).cloned().unwrap_or_default()
        }
        fn list_element_type(&self, ty: &Ty) -> Ty {
            match self.expand_alias(ty) {
                Ty::List(e, _) => (*e).clone(),
                t => t,
            }
        }
    }

    fn qtn(name: &str) -> QualifiedTypeName {
        QualifiedTypeName::new(Name::new("user"), vec![], Name::new(name))
    }
    fn class_ty(q: &QualifiedTypeName) -> Ty {
        Ty::Class(q.clone(), vec![], Default::default())
    }
    fn list_of(elem: Ty) -> Ty {
        Ty::List(Box::new(elem), Default::default())
    }
    fn opt_of(t: Ty) -> Ty {
        Ty::optional(t)
    }
    fn union_of(ts: Vec<Ty>) -> Ty {
        Ty::Union(ts, Default::default())
    }
    fn null_ty() -> Ty {
        Ty::Null {
            attr: Default::default(),
        }
    }
    fn never_ty() -> Ty {
        Ty::Never {
            attr: Default::default(),
        }
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
            DPat::class(q.clone(), vec![DPat::wildcard(bool_ty())], class_ty(q))
        };
        let mk_pair =
            |left: DPat, right: DPat| DPat::class(pair.clone(), vec![left, right], pair_ty.clone());

        let report = compute_match_usefulness(
            &cx,
            &[mk_pair(variant(&a), DPat::wildcard(e_ty.clone()))],
            pair_ty.clone(),
        );
        assert!(
            !report.missing.is_empty(),
            "A(_) in the left slot must not cover B(_)"
        );

        let report = compute_match_usefulness(
            &cx,
            &[
                mk_pair(variant(&a), DPat::wildcard(e_ty.clone())),
                mk_pair(variant(&b), DPat::wildcard(e_ty.clone())),
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
                mk_pair(variant(&a), DPat::wildcard(e_ty.clone())),
                mk_pair(DPat::wildcard(e_ty.clone()), variant(&a)),
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
                mk_pair(variant(&a), DPat::wildcard(e_ty.clone())),
                mk_pair(DPat::wildcard(e_ty.clone()), variant(&a)),
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
            |left: DPat, right: DPat| DPat::class(pair.clone(), vec![left, right], pair_ty.clone());

        let report = compute_match_usefulness(&cx, &[], pair_ty.clone());
        let witnesses: Vec<String> = report.missing.iter().map(ToString::to_string).collect();
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
            DPat::single(bool_lit(false), opt_bool.clone()),
            DPat::single(bool_lit(false), opt_bool.clone()),
        );
        let report = compute_match_usefulness(&cx, &[false_false], pair_ty.clone());
        let witnesses: Vec<String> = report.missing.iter().map(ToString::to_string).collect();

        assert_eq!(
            witnesses.len(),
            8,
            "all Optional<bool> pairs except false/false should be missing"
        );
        for expected in [
            "OptionalPair { true, true }",
            "OptionalPair { true, false }",
            "OptionalPair { true, null }",
            "OptionalPair { false, true }",
            "OptionalPair { false, null }",
            "OptionalPair { null, true }",
            "OptionalPair { null, false }",
            "OptionalPair { null, null }",
        ] {
            assert!(
                witnesses.iter().any(|w| w == expected),
                "expected {expected} in witnesses, got {witnesses:?}"
            );
        }

        let any_false = mk_pair(
            DPat::wildcard(opt_bool.clone()),
            DPat::single(bool_lit(false), opt_bool.clone()),
        );
        let report = compute_match_usefulness(&cx, &[any_false], pair_ty);
        let witnesses: Vec<String> = report.missing.iter().map(ToString::to_string).collect();
        assert_eq!(
            witnesses.len(),
            2,
            "all pairs with a non-false right side should be missing"
        );
        for expected in ["OptionalPair { _, true }", "OptionalPair { _, null }"] {
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

        let ok_any = || DPat::class(ok.clone(), vec![DPat::wildcard(bool_ty())], ok_ty.clone());

        let report = compute_match_usefulness(&cx, &[ok_any()], result_ty.clone());
        assert!(
            report.missing.is_empty(),
            "Ok(_) should exhaust Result<bool, Never>; got {:?}",
            report
                .missing
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        );

        let mk_pair = |b: bool| {
            DPat::class(
                pair.clone(),
                vec![DPat::single(bool_lit(b), bool_ty()), ok_any()],
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
        let witnesses: Vec<String> = report.missing.iter().map(ToString::to_string).collect();
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
                DPat::class(
                    variant.clone(),
                    vec![DPat::wildcard(bool_ty())],
                    class_ty(variant),
                )
            })
            .collect();
        arms.push(DPat::wildcard(scrut.clone()));

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
        let non_empty = DPat::slice(
            SliceShape::Variable {
                prefix: 1,
                suffix: 0,
            },
            vec![DPat::wildcard(bool_ty())],
            scrut.clone(),
        );
        let wildcard = DPat::wildcard(scrut.clone());

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
            DPat::class(
                q.clone(),
                vec![
                    DPat::single(bool_lit(a), bool_ty()),
                    DPat::single(bool_lit(b), bool_ty()),
                    DPat::single(bool_lit(c), bool_ty()),
                    DPat::single(bool_lit(d), bool_ty()),
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
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
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
        let arm1 = DPat::class(
            q.clone(),
            vec![
                DPat::single(bool_lit(true), bool_ty()),
                DPat::wildcard(bool_ty()),
            ],
            qty.clone(),
        );
        // arm covers (false, true) and (false, false).
        let arm2 = DPat::class(
            q.clone(),
            vec![
                DPat::single(bool_lit(false), bool_ty()),
                DPat::wildcard(bool_ty()),
            ],
            qty.clone(),
        );

        let report = compute_match_usefulness(&cx, &[arm1, arm2], qty);
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

    // ── 2. Variable slice with prefix and suffix ────────────────────────

    /// `[true, ..rest, false]` + `[..rest]` covers everything.
    #[test]
    fn testing_2_variable_prefix_suffix() {
        let cx = TestingCtx::new();
        let arr = list_of(bool_ty());

        let arm1 = DPat::slice(
            SliceShape::Variable {
                prefix: 1,
                suffix: 1,
            },
            vec![
                DPat::single(bool_lit(true), bool_ty()),
                DPat::single(bool_lit(false), bool_ty()),
            ],
            arr.clone(),
        );
        let arm2 = DPat::slice(
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
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
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

        let arm0 = DPat::slice(SliceShape::Fixed(0), vec![], arr.clone());
        let arm1 = DPat::slice(
            SliceShape::Fixed(1),
            vec![DPat::wildcard(int_ty())],
            arr.clone(),
        );
        let arm2 = DPat::slice(
            SliceShape::Fixed(2),
            vec![DPat::wildcard(int_ty()), DPat::wildcard(int_ty())],
            arr.clone(),
        );
        let arm3plus = DPat::slice(
            SliceShape::Variable {
                prefix: 3,
                suffix: 0,
            },
            vec![
                DPat::wildcard(int_ty()),
                DPat::wildcard(int_ty()),
                DPat::wildcard(int_ty()),
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
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
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
        let arm_lit = DPat::slice(
            SliceShape::Fixed(2),
            vec![
                DPat::single(int_lit(1), int_ty()),
                DPat::single(int_lit(2), int_ty()),
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
            DPat::class(
                r.clone(),
                vec![
                    DPat::single(bool_lit(ok), bool_ty()),
                    match val {
                        Some(v) => DPat::single(bool_lit(v), bool_ty()),
                        None => DPat::wildcard(bool_ty()),
                    },
                ],
                result_ty.clone(),
            )
        };

        let len0 = DPat::slice(SliceShape::Fixed(0), vec![], arr.clone());
        let len1_ok = DPat::slice(
            SliceShape::Fixed(1),
            vec![make_class(true, None)],
            arr.clone(),
        );
        let len1_err = DPat::slice(
            SliceShape::Fixed(1),
            vec![make_class(false, None)],
            arr.clone(),
        );
        let len2plus = DPat::slice(
            SliceShape::Variable {
                prefix: 2,
                suffix: 0,
            },
            vec![
                DPat::wildcard(result_ty.clone()),
                DPat::wildcard(result_ty.clone()),
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
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );

        // Drop the false-ok branch — witness must mention `ok: false` inside
        // a length-1 slice.
        let report = compute_match_usefulness(&cx, &[len0, len1_ok, len2plus], arr);
        assert!(
            !report.missing.is_empty(),
            "expected missing when len-1 false-ok arm dropped"
        );
        let strs: Vec<String> = report.missing.iter().map(|w| w.to_string()).collect();
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

        let cont = |tag: bool, items: DPat| {
            DPat::class(
                c.clone(),
                vec![DPat::single(bool_lit(tag), bool_ty()), items],
                cty.clone(),
            )
        };
        let true_empty = cont(true, DPat::slice(SliceShape::Fixed(0), vec![], arr.clone()));
        let true_nonempty = cont(
            true,
            DPat::slice(
                SliceShape::Variable {
                    prefix: 1,
                    suffix: 0,
                },
                vec![DPat::wildcard(bool_ty())],
                arr.clone(),
            ),
        );
        let false_any = cont(false, DPat::wildcard(arr.clone()));

        // (a) All three — exhaustive.
        let report = compute_match_usefulness(
            &cx,
            &[true_empty.clone(), true_nonempty.clone(), false_any.clone()],
            cty.clone(),
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

        let null = DPat::single(null_ty(), outer.clone());
        let inner_null = DPat::single(null_ty(), outer.clone());
        let bool_true = DPat::single(bool_lit(true), outer.clone());
        let bool_false = DPat::single(bool_lit(false), outer.clone());

        // The current enumerate_ctors for Optional<Optional<bool>> returns
        // ctors of inner (true, false, null) plus null. The two `null`s
        // collapse to one because Single(null) has the same identity. So
        // expected required cases: {true, false, null} — three.
        // (This documents current behavior; if BAML doesn't flatten, this
        // is actually a correctness concern to flag.)
        let report = compute_match_usefulness(
            &cx,
            &[null.clone(), bool_true.clone(), bool_false.clone()],
            outer.clone(),
        );
        assert!(
            report.missing.is_empty(),
            "with three-case enumeration this should be exhaustive: {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );
        // The duplicate-null arm `inner_null` covering the same case as
        // `null` is reported as unreachable.
        let report =
            compute_match_usefulness(&cx, &[null, inner_null, bool_true, bool_false], outer);
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

        let arm0_a = DPat::single(int_lit(1), scrut.clone());
        let arm0_b = DPat::single(int_lit(2), scrut.clone());
        let arm1 = DPat::single(int_lit(2), scrut.clone()); // unreachable
        let arm2 = DPat::single(int_lit(3), scrut.clone());

        // To use ArmId for arms_for_user, we need a way to share arm-id
        // across or-pattern rows. compute_match_usefulness assigns one ArmId
        // per row, so we lose source-arm grouping here. Verify behavior at
        // the row level: rows 0,1 useful (covering 1,2); row 2 unreachable
        // (matches 2 already covered); row 3 useful (covers 3).
        let report = compute_match_usefulness(&cx, &[arm0_a, arm0_b, arm1, arm2], scrut);
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
            DPat::class(
                ok.clone(),
                vec![DPat::single(bool_lit(v), bool_ty())],
                ok_ty.clone(),
            )
        };
        let any_err = DPat::class(err.clone(), vec![DPat::wildcard(int_ty())], err_ty.clone());

        // (a) Full coverage exhaustive.
        let report = compute_match_usefulness(
            &cx,
            &[mk_ok(true), mk_ok(false), any_err.clone()],
            scrut.clone(),
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

        // (b) Drop `Ok{val:false}` → witness mentions Ok and false.
        let report = compute_match_usefulness(&cx, &[mk_ok(true), any_err], scrut);
        let strs: Vec<String> = report.missing.iter().map(|w| w.to_string()).collect();
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
        let arms = vec![DPat::class(
            w_int.clone(),
            vec![DPat::wildcard(int_ty())],
            scrut.clone(),
        )];
        let report = compute_match_usefulness(&cx, &arms, scrut);
        assert!(
            report.missing.is_empty(),
            "wildcard inner should be exhaustive: {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );

        // Bool variant — both literals exhausts without wildcard.
        let scrut = class_ty(&w_bool);
        let arms = vec![
            DPat::class(
                w_bool.clone(),
                vec![DPat::single(bool_lit(true), bool_ty())],
                scrut.clone(),
            ),
            DPat::class(
                w_bool.clone(),
                vec![DPat::single(bool_lit(false), bool_ty())],
                scrut.clone(),
            ),
        ];
        let report = compute_match_usefulness(&cx, &arms, scrut);
        assert!(
            report.missing.is_empty(),
            "bool inner with both literals should be exhaustive: {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
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

        let arm = DPat::class(
            p.clone(),
            vec![
                DPat::single(bool_lit(true), bool_ty()),
                DPat::single(bool_lit(true), bool_ty()),
            ],
            pty.clone(),
        );
        let report = compute_match_usefulness(&cx, &[arm], pty);
        let strs: Vec<String> = report.missing.iter().map(|w| w.to_string()).collect();
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
        let arm = DPat::class(
            a.clone(),
            vec![DPat::class(
                b.clone(),
                vec![DPat::slice(
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
        let arm = DPat::slice(SliceShape::Fixed(0), vec![], arr.clone());
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
        let never = Ty::Never {
            attr: Default::default(),
        };
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
        let any = DPat::slice(
            SliceShape::Variable {
                prefix: 0,
                suffix: 0,
            },
            vec![],
            arr.clone(),
        );
        let one_true = DPat::slice(
            SliceShape::Fixed(1),
            vec![DPat::single(bool_lit(true), bool_ty())],
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
        let pre = DPat::slice(
            SliceShape::Variable {
                prefix: 1,
                suffix: 0,
            },
            vec![DPat::wildcard(bool_ty())],
            arr.clone(),
        );
        let suf = DPat::slice(
            SliceShape::Variable {
                prefix: 0,
                suffix: 1,
            },
            vec![DPat::wildcard(bool_ty())],
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
        let arm = DPat::slice(
            SliceShape::Variable {
                prefix: 1,
                suffix: 0,
            },
            vec![DPat::wildcard(bool_ty())],
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
        let arm = DPat::slice(
            SliceShape::Variable {
                prefix: 2,
                suffix: 0,
            },
            vec![DPat::wildcard(bool_ty()), DPat::wildcard(bool_ty())],
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
        let any = DPat::slice(
            SliceShape::Variable {
                prefix: 0,
                suffix: 0,
            },
            vec![],
            arr.clone(),
        );
        let pair = DPat::slice(
            SliceShape::Fixed(2),
            vec![DPat::wildcard(int_ty()), DPat::wildcard(int_ty())],
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
        let var31 = DPat::slice(
            SliceShape::Variable {
                prefix: 3,
                suffix: 1,
            },
            vec![
                DPat::wildcard(bool_ty()),
                DPat::wildcard(bool_ty()),
                DPat::wildcard(bool_ty()),
                DPat::wildcard(bool_ty()),
            ],
            arr.clone(),
        );
        let report = compute_match_usefulness(&cx, &[var31.clone()], arr.clone());
        assert!(
            !report.missing.is_empty(),
            "Var{{3,1}} alone should leave shorter lengths missing"
        );

        // Add a catch-all `[..r]` — exhaustive.
        let any = DPat::slice(
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
            |inner_pat: DPat| DPat::slice(SliceShape::Fixed(1), vec![inner_pat], outer_ty.clone());
        let inner_pat = |a: bool, b: Option<bool>| {
            DPat::slice(
                SliceShape::Fixed(2),
                vec![
                    DPat::single(bool_lit(a), bool_ty()),
                    match b {
                        Some(v) => DPat::single(bool_lit(v), bool_ty()),
                        None => DPat::wildcard(bool_ty()),
                    },
                ],
                inner_ty.clone(),
            )
        };
        let arm1 = outer_one_inner(inner_pat(true, None));
        let arm2 = outer_one_inner(inner_pat(false, Some(true)));
        let arm3 = DPat::slice(
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
            DPat::class(
                p.clone(),
                vec![
                    DPat::single(bool_lit(a), bool_ty()),
                    DPat::wildcard(bool_ty()),
                ],
                pty.clone(),
            )
        };
        let len0 = DPat::slice(SliceShape::Fixed(0), vec![], arr.clone());
        let len1_true = DPat::slice(SliceShape::Fixed(1), vec![pair(true)], arr.clone());
        let len1_false = DPat::slice(SliceShape::Fixed(1), vec![pair(false)], arr.clone());
        let len2plus = DPat::slice(
            SliceShape::Variable {
                prefix: 2,
                suffix: 0,
            },
            vec![DPat::wildcard(pty.clone()), DPat::wildcard(pty.clone())],
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

        let arm = DPat::class(
            h.clone(),
            vec![DPat::slice(
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

        let arm = DPat::class(e.clone(), vec![], ety.clone());
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
            DPat::class(
                inner.clone(),
                vec![DPat::single(bool_lit(v), bool_ty())],
                inner_ty.clone(),
            )
        };
        let arm = DPat::class(
            outer.clone(),
            vec![DPat::slice(
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

        let any = DPat::wildcard(pty.clone());
        let specific = DPat::class(
            p.clone(),
            vec![
                DPat::single(bool_lit(true), bool_ty()),
                DPat::single(bool_lit(false), bool_ty()),
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
            DPat::class(
                p.clone(),
                vec![
                    DPat::single(bool_lit(a), bool_ty()),
                    DPat::wildcard(bool_ty()),
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
            DPat::class(
                q.clone(),
                (0..8)
                    .map(|i| DPat::single(bool_lit((bits >> i) & 1 != 0), bool_ty()))
                    .collect(),
                qty.clone(),
            )
        };

        // All 256 → exhaustive.
        let arms: Vec<DPat> = (0u8..=255).map(combo).collect();
        let report = compute_match_usefulness(&cx, &arms, qty.clone());
        assert!(
            report.missing.is_empty(),
            "256 arms should be exhaustive: {} missing",
            report.missing.len()
        );
        assert!(report.unreachable_arms.is_empty());

        // Drop combo 0b10110100 (180) → exactly one missing.
        let arms: Vec<DPat> = (0u8..=255).filter(|b| *b != 180).map(combo).collect();
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

        let null_top = DPat::single(null_ty(), scrut.clone());
        // Some(node) with any val and any next.
        let some_any = DPat::class(
            n.clone(),
            vec![DPat::wildcard(bool_ty()), DPat::wildcard(opt_n.clone())],
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
            DPat::class(
                a.clone(),
                vec![DPat::class(
                    b.clone(),
                    vec![DPat::class(
                        c.clone(),
                        vec![DPat::class(
                            d.clone(),
                            vec![DPat::class(
                                e.clone(),
                                vec![DPat::single(bool_lit(v), bool_ty())],
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
        let inner_true = DPat::slice(
            SliceShape::Fixed(1),
            vec![DPat::single(bool_lit(true), bool_ty())],
            lvl1.clone(),
        );
        let mid = DPat::slice(SliceShape::Fixed(1), vec![inner_true], lvl2.clone());
        let outer = DPat::slice(SliceShape::Fixed(1), vec![mid], lvl3.clone());

        // Catch-all.
        let any = DPat::slice(
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
        let inner_true2 = DPat::slice(
            SliceShape::Fixed(1),
            vec![DPat::single(bool_lit(true), bool_ty())],
            lvl1.clone(),
        );
        let mid2 = DPat::slice(SliceShape::Fixed(1), vec![inner_true2], lvl2.clone());
        let outer2 = DPat::slice(SliceShape::Fixed(1), vec![mid2], lvl3.clone());
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
            DPat::class(
                p.clone(),
                vec![
                    a.map_or(DPat::wildcard(bool_ty()), |v| {
                        DPat::single(bool_lit(v), bool_ty())
                    }),
                    b.map_or(DPat::wildcard(bool_ty()), |v| {
                        DPat::single(bool_lit(v), bool_ty())
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
            .map(|q| DPat::class(q.clone(), vec![], class_ty(q)))
            .collect();
        let report = compute_match_usefulness(&cx, &arms_all, scrut.clone());
        assert!(report.missing.is_empty());

        // Drop "G" → exactly G missing.
        let arms: Vec<_> = qtns
            .iter()
            .filter(|q| q.name().as_str() != "G")
            .map(|q| DPat::class(q.clone(), vec![], class_ty(q)))
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
        let inner = DPat::class(
            i.clone(),
            vec![DPat::single(bool_lit(true), bool_ty())],
            i_ty.clone(),
        );
        let inner_arr_one = DPat::slice(SliceShape::Fixed(1), vec![inner], i_arr.clone());
        let mid = DPat::class(m.clone(), vec![inner_arr_one], m_ty.clone());
        let mid_arr_one = DPat::slice(SliceShape::Fixed(1), vec![mid], m_arr.clone());
        let outer = DPat::class(o.clone(), vec![mid_arr_one], o_ty.clone());

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
        let var53 = DPat::slice(
            SliceShape::Variable {
                prefix: 5,
                suffix: 3,
            },
            (0..8).map(|_| DPat::wildcard(bool_ty())).collect(),
            arr.clone(),
        );
        let any = DPat::slice(
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
            DPat::class(
                t.clone(),
                vec![
                    DPat::single(bool_lit(a), bool_ty()),
                    DPat::single(bool_lit(b), bool_ty()),
                    DPat::wildcard(bool_ty()),
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
            DPat::class(
                q.clone(),
                (0..6)
                    .map(|i| DPat::single(bool_lit((bits >> i) & 1 != 0), bool_ty()))
                    .collect(),
                qty.clone(),
            )
        };
        let arms: Vec<DPat> = (0u8..64).map(combo).collect();
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
            DPat::class(
                p.clone(),
                vec![
                    DPat::single(bool_lit(a), bool_ty()),
                    DPat::single(bool_lit(b), bool_ty()),
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
            DPat::class(
                b.clone(),
                vec![
                    DPat::single(bool_lit(p), bool_ty()),
                    DPat::single(bool_lit(q), bool_ty()),
                ],
                b_ty.clone(),
            )
        };
        // Cover all 64 = 4^3 combos.
        let mut arms = Vec::new();
        for px in 0..4 {
            for py in 0..4 {
                for pz in 0..4 {
                    let mk_b_n = |bits: usize| mk_b((bits & 1) != 0, (bits & 2) != 0);
                    arms.push(DPat::class(
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
        let ends_true = DPat::slice(
            SliceShape::Variable {
                prefix: 0,
                suffix: 1,
            },
            vec![DPat::single(bool_lit(true), bool_ty())],
            arr.clone(),
        );
        let true_false = DPat::slice(
            SliceShape::Fixed(2),
            vec![
                DPat::single(bool_lit(true), bool_ty()),
                DPat::single(bool_lit(false), bool_ty()),
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
    /// `DPat::or`; expansion happens inside the algorithm.
    #[test]
    fn testing_40_or_pattern_source_arm_ids() {
        let cx = TestingCtx::new();
        let scrut = union_of(vec![int_lit(1), int_lit(2)]);

        let arm0 = DPat::or(
            vec![
                DPat::single(int_lit(1), scrut.clone()),
                DPat::single(int_lit(2), scrut.clone()),
            ],
            scrut.clone(),
        );
        let arm1 = DPat::single(int_lit(2), scrut.clone());

        let report = compute_match_usefulness(&cx, &[arm0, arm1], scrut);
        assert_eq!(
            report.unreachable_arms,
            vec![ArmId(1)],
            "expected source arm 1 unreachable, got {:?}",
            report.unreachable_arms
        );
    }

    // ── 41. Depth limit must not silently mark a missing leaf exhaustive ─

    /// Build a chain of 258 nested classes, each with one field of the
    /// next class type, the innermost being `bool`. A single arm matches
    /// the deepest `true` leaf — `false` at the leaf is missing. The
    /// algorithm must report at least one missing case; the (now-removed)
    /// depth guard previously masked this by silently returning early.
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

        let mut pat = DPat::single(bool_lit(true), bool_ty());
        for q in qs.iter().rev() {
            pat = DPat::class(q.clone(), vec![pat], class_ty(q));
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
        let never = Ty::Never {
            attr: Default::default(),
        };
        let arr = list_of(never);
        let empty = DPat::slice(SliceShape::Fixed(0), vec![], arr.clone());

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
            DPat::single(bool_lit(true), outer.clone()),
            DPat::single(bool_lit(false), outer.clone()),
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
        cx.register(
            inner.clone(),
            vec![Ty::Never {
                attr: Default::default(),
            }],
        );
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
        cx.register(
            empty.clone(),
            vec![Ty::Never {
                attr: Default::default(),
            }],
        );
        let ty = class_ty(&empty);
        let report = compute_match_usefulness(&cx, &[DPat::wildcard(ty.clone())], ty);
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

    /// Helper: a `testingCtx`-style ctx that enumerates Union members as
    /// `UnionMember` ctors. Mirrors what the real builder does.
    struct UnionCtx {
        classes: std::collections::HashMap<QualifiedTypeName, Vec<Ty>>,
    }
    impl UnionCtx {
        fn new() -> Self {
            Self {
                classes: std::collections::HashMap::new(),
            }
        }
        fn register(&mut self, qtn: QualifiedTypeName, fields: Vec<Ty>) {
            self.classes.insert(qtn, fields);
        }
    }
    impl PatCtx for UnionCtx {
        fn enumerate_ctors(&self, ty: &Ty) -> Vec<Ctor> {
            match ty {
                Ty::Bool { .. } => {
                    vec![Ctor::Single(bool_lit(true)), Ctor::Single(bool_lit(false))]
                }
                Ty::Int { .. } | Ty::Float { .. } | Ty::String { .. } => vec![Ctor::NonExhaustive],
                Ty::Null { .. } => vec![Ctor::Single(ty.clone())],
                // Key change: each union member becomes a UnionMember ctor.
                Ty::Union(members, _) => members
                    .iter()
                    .map(|m| Ctor::UnionMember(m.clone()))
                    .collect(),
                Ty::Literal(_, _, _) | Ty::EnumVariant(_, _, _) => {
                    vec![Ctor::Single(ty.clone())]
                }
                Ty::Class(qtn, args, _) => vec![Ctor::Class(qtn.clone(), args.clone())],
                Ty::List(_, _) => vec![],
                Ty::Never { .. } => vec![],
                Ty::TypeVar(_, _) => vec![Ctor::NonExhaustive],
                _ => vec![Ctor::NonExhaustive],
            }
        }
        fn class_field_types(&self, qtn: &QualifiedTypeName, _ty: &Ty) -> Vec<Ty> {
            self.classes.get(qtn).cloned().unwrap_or_default()
        }
        fn list_element_type(&self, ty: &Ty) -> Ty {
            match ty {
                Ty::List(e, _) => (**e).clone(),
                _ => ty.clone(),
            }
        }
    }

    /// `match val: Class | int[] { Class{} | [..] => ... }` — the slice
    /// arm covers the entire list branch (Variable{0,0} covers everything).
    /// With the `UnionMember` ctor, specialising on `UnionMember(int[])`
    /// recurses into a column of type `int[]`, where slice-splitting fires
    /// and recognises `[..]` as exhaustive.
    #[test]
    fn testing_47_union_member_slice_wildcard_exhaustive() {
        let mut cx = UnionCtx::new();
        let cls = qtn("Cls");
        cx.register(cls.clone(), vec![]);
        let cls_ty = class_ty(&cls);
        let arr = list_of(int_ty());
        let scrut = union_of(vec![cls_ty.clone(), arr.clone()]);

        let class_arm = DPat::union_member(
            cls_ty.clone(),
            DPat::class(cls.clone(), vec![], cls_ty.clone()),
            scrut.clone(),
        );
        let slice_arm = DPat::union_member(
            arr.clone(),
            DPat::slice(
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
        let mut cx = UnionCtx::new();
        let cls = qtn("Cls");
        cx.register(cls.clone(), vec![]);
        let cls_ty = class_ty(&cls);
        let arr = list_of(int_ty());
        let scrut = union_of(vec![cls_ty.clone(), arr.clone()]);

        let class_arm = DPat::union_member(
            cls_ty.clone(),
            DPat::class(cls.clone(), vec![], cls_ty.clone()),
            scrut.clone(),
        );
        let slice_arm = DPat::union_member(
            arr.clone(),
            DPat::slice(
                SliceShape::Fixed(1),
                vec![DPat::wildcard(int_ty())],
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
        let mut cx = UnionCtx::new();
        let cls = qtn("Cls");
        cx.register(cls.clone(), vec![]);
        let cls_ty = class_ty(&cls);
        let arr = list_of(int_ty());
        let scrut = union_of(vec![cls_ty.clone(), arr.clone()]);

        let class_arm = DPat::union_member(
            cls_ty.clone(),
            DPat::class(cls.clone(), vec![], cls_ty.clone()),
            scrut.clone(),
        );
        let empty_arm = DPat::union_member(
            arr.clone(),
            DPat::slice(SliceShape::Fixed(0), vec![], arr.clone()),
            scrut.clone(),
        );
        let nonempty_arm = DPat::union_member(
            arr.clone(),
            DPat::slice(
                SliceShape::Variable {
                    prefix: 1,
                    suffix: 0,
                },
                vec![DPat::wildcard(int_ty())],
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
        let mut cx = UnionCtx::new();
        let cls = qtn("Cls");
        cx.register(cls.clone(), vec![]);
        let cls_ty = class_ty(&cls);
        let arr = list_of(int_ty());
        let scrut = union_of(vec![cls_ty.clone(), arr.clone()]);

        let class_arm = DPat::union_member(
            cls_ty.clone(),
            DPat::class(cls.clone(), vec![], cls_ty.clone()),
            scrut.clone(),
        );

        let report = compute_match_usefulness(&cx, &[class_arm], scrut);
        assert!(!report.missing.is_empty());
    }

    /// Wildcard arm covers any union member: exhaustive.
    #[test]
    fn testing_51_union_member_wildcard_covers_all() {
        let mut cx = UnionCtx::new();
        let cls = qtn("Cls");
        cx.register(cls.clone(), vec![]);
        let cls_ty = class_ty(&cls);
        let arr = list_of(int_ty());
        let scrut = union_of(vec![cls_ty, arr]);

        let report = compute_match_usefulness(&cx, &[DPat::wildcard(scrut.clone())], scrut);
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

        let alias_ty = Ty::TypeAlias(foo.clone(), Default::default());

        // Both branches → exhaustive.
        let arms = vec![
            DPat::single(bool_lit(true), alias_ty.clone()),
            DPat::single(bool_lit(false), alias_ty.clone()),
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
        let arms = vec![DPat::single(bool_lit(true), alias_ty.clone())];
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
        let tri_ty = Ty::TypeAlias(tri, Default::default());

        let arms = vec![
            DPat::single(int_lit(1), tri_ty.clone()),
            DPat::single(int_lit(2), tri_ty.clone()),
            DPat::single(int_lit(3), tri_ty.clone()),
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
        let alias_ty = Ty::TypeAlias(alias, Default::default());

        let null_top = DPat::single(null_ty(), alias_ty.clone());
        let some_any = DPat::class(
            n.clone(),
            vec![DPat::wildcard(bool_ty()), DPat::wildcard(opt_n.clone())],
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
        let witness = WitnessPat::new(
            Ctor::Slice(SliceShape::Variable {
                prefix: 2,
                suffix: 0,
            }),
            vec![
                WitnessPat::wildcard(bool_ty()),
                WitnessPat::wildcard(bool_ty()),
            ],
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
            DPat::slice(
                SliceShape::Fixed(2),
                vec![DPat::single(int_lit(v), int_ty()), DPat::wildcard(int_ty())],
                arr.clone(),
            )
        };
        // Two rows simulating `[1 | 2, _]`.
        let arms = vec![arm(1), arm(2), arm(3), DPat::wildcard(arr.clone())];
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
        let qtn = QualifiedTypeName::new(Name::new("user"), vec![], Name::new("Pair"));
        let pair_ty = Ty::Class(qtn.clone(), vec![], Default::default());

        let arm1 = DPat::class(
            qtn.clone(),
            vec![
                DPat::single(bool_lit(true), bool_ty()),
                DPat::wildcard(bool_ty()),
            ],
            pair_ty.clone(),
        );
        let arm2 = DPat::class(
            qtn.clone(),
            vec![
                DPat::single(bool_lit(false), bool_ty()),
                DPat::single(bool_lit(true), bool_ty()),
            ],
            pair_ty.clone(),
        );

        struct PairCtx(QualifiedTypeName);
        impl PatCtx for PairCtx {
            fn enumerate_ctors(&self, ty: &Ty) -> Vec<Ctor> {
                match ty {
                    Ty::Bool { .. } => vec![
                        Ctor::Single(Ty::Literal(
                            Literal::Bool(true),
                            Freshness::Regular,
                            Default::default(),
                        )),
                        Ctor::Single(Ty::Literal(
                            Literal::Bool(false),
                            Freshness::Regular,
                            Default::default(),
                        )),
                    ],
                    Ty::Class(_, args, _) => vec![Ctor::Class(self.0.clone(), args.clone())],
                    Ty::Literal(_, _, _) => vec![Ctor::Single(ty.clone())],
                    _ => vec![Ctor::NonExhaustive],
                }
            }
            fn class_field_types(&self, _q: &QualifiedTypeName, _t: &Ty) -> Vec<Ty> {
                vec![
                    Ty::Bool {
                        attr: Default::default(),
                    },
                    Ty::Bool {
                        attr: Default::default(),
                    },
                ]
            }
            fn list_element_type(&self, ty: &Ty) -> Ty {
                ty.clone()
            }
        }

        let cx = PairCtx(qtn);
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

    // ── Rigid (typevar/projection) rows: possible but never covering ─────────

    fn type_var_ty(name: &str) -> Ty {
        Ty::type_var(name)
    }

    fn projection_ty(base: Ty, iface: &str, member: &str) -> Ty {
        Ty::AssociatedTypeProjection {
            base: Box::new(base),
            interface: Box::new(baml_type::Interface::new(
                baml_type::TypeName::local(Name::new(iface)),
                vec![],
                vec![],
            )),
            member: Name::new(member),
            attr: Default::default(),
        }
    }

    #[test]
    fn ty_ctor_identity_is_stable_for_projections_and_typevars() {
        // Two independent constructions of the same projection agree; a
        // different member (or a different var) is a distinct identity. This
        // stability is what lets a rigid `Single` row round-trip through the
        // matrix's hash-based ctor sets.
        let a = projection_ty(type_var_ty("Self"), "Iterator", "Item");
        let b = projection_ty(type_var_ty("Self"), "Iterator", "Item");
        let other = projection_ty(type_var_ty("Self"), "Iterator", "Error");
        assert_eq!(ty_ctor_identity(&a), ty_ctor_identity(&b));
        assert_ne!(ty_ctor_identity(&a), ty_ctor_identity(&other));
        assert_eq!(
            ty_ctor_identity(&type_var_ty("T")),
            ty_ctor_identity(&type_var_ty("T"))
        );
        assert_ne!(
            ty_ctor_identity(&type_var_ty("T")),
            ty_ctor_identity(&type_var_ty("U"))
        );
    }

    #[test]
    fn rigid_single_row_in_finite_column_is_possible_not_covering() {
        // `match (x: bool) { let t: T if …, _ }`-shaped matrix: the rigid row
        // is outside the column's alphabet, so it covers nothing (the wildcard
        // is required) — but it must still get its own split branch so it is
        // specialized and not falsely flagged unreachable.
        let cx = TestingCtx::new();
        let rigid = DPat::single(type_var_ty("T"), bool_ty());
        let report =
            compute_match_usefulness(&cx, &[rigid.clone(), DPat::wildcard(bool_ty())], bool_ty());
        assert!(report.missing.is_empty(), "wildcard completes the match");
        assert!(
            report.unreachable_arms.is_empty(),
            "neither the rigid row nor the wildcard is unreachable: {:?}",
            report.unreachable_arms
        );

        // Without the wildcard the rigid row proves no coverage at all.
        let report = compute_match_usefulness(&cx, &[rigid], bool_ty());
        assert!(
            !report.missing.is_empty(),
            "a rigid row alone never covers a concrete column"
        );
        assert!(report.unreachable_arms.is_empty());
    }

    #[test]
    fn rigid_single_row_among_full_concrete_cover_is_not_unreachable() {
        // `let t: T` alongside a complete concrete cover: the concrete arms
        // decide exhaustiveness; the rigid row stays a live possible row.
        let cx = TestingCtx::new();
        let arms = vec![
            DPat::single(type_var_ty("T"), bool_ty()),
            DPat::single(bool_lit(true), bool_ty()),
            DPat::single(bool_lit(false), bool_ty()),
        ];
        let report = compute_match_usefulness(&cx, &arms, bool_ty());
        assert!(report.missing.is_empty());
        assert!(
            report.unreachable_arms.is_empty(),
            "unexpected unreachable arms: {:?}",
            report.unreachable_arms
        );
    }

    #[test]
    fn rigid_single_row_in_list_column_is_possible_not_covering() {
        // A `let t: T[]` type-pattern row in a `string[]` column: no length
        // class is covered (the wildcard is required), but the row gets its
        // own split branch and stays reachable.
        let cx = TestingCtx::new();
        let col = list_of(Ty::String {
            attr: Default::default(),
        });
        let rigid = DPat::single(list_of(type_var_ty("T")), col.clone());
        let report = compute_match_usefulness(
            &cx,
            &[rigid.clone(), DPat::wildcard(col.clone())],
            col.clone(),
        );
        assert!(report.missing.is_empty());
        assert!(
            report.unreachable_arms.is_empty(),
            "unexpected unreachable arms: {:?}",
            report.unreachable_arms
        );

        let report = compute_match_usefulness(&cx, &[rigid], col);
        assert!(!report.missing.is_empty());
        assert!(report.unreachable_arms.is_empty());
    }

    #[test]
    fn class_ctor_args_are_identity_relevant() {
        // Generics are invariant: `Box<T>` in a `Box<int>` column is a
        // possible-but-not-covering row (wildcard required, row reachable),
        // while `Box<T>` in a `Box<T>` column covers reflexively.
        let mut cx = TestingCtx::new();
        let box_q = qtn("Box");
        cx.register(box_q.clone(), vec![int_ty()]);
        let box_int = Ty::Class(box_q.clone(), vec![int_ty()], Default::default());
        let box_t = Ty::Class(box_q.clone(), vec![type_var_ty("T")], Default::default());

        let rigid_row = DPat::class_inst(
            box_q.clone(),
            vec![type_var_ty("T")],
            vec![DPat::wildcard(int_ty())],
            box_int.clone(),
        );
        let report = compute_match_usefulness(
            &cx,
            &[rigid_row.clone(), DPat::wildcard(box_int.clone())],
            box_int.clone(),
        );
        assert!(report.missing.is_empty());
        assert!(
            report.unreachable_arms.is_empty(),
            "unexpected unreachable arms: {:?}",
            report.unreachable_arms
        );

        let report = compute_match_usefulness(&cx, &[rigid_row], box_int);
        assert!(
            !report.missing.is_empty(),
            "an arg-mismatched class row never covers a concrete column"
        );
        assert!(report.unreachable_arms.is_empty());

        // Reflexive: same rigid args on both sides — a definite cover.
        let reflexive_row = DPat::class_inst(
            box_q.clone(),
            vec![type_var_ty("T")],
            vec![DPat::wildcard(int_ty())],
            box_t.clone(),
        );
        let report = compute_match_usefulness(&cx, &[reflexive_row], box_t);
        assert!(
            report.missing.is_empty(),
            "identical rigid args cover reflexively: {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );
        assert!(report.unreachable_arms.is_empty());
    }

    #[test]
    fn rigid_class_branch_does_not_manufacture_witnesses() {
        // `match (b: Box<int>) { Box<T> { v: 1 } =>, Box<int> { v: _ } => }`:
        // the concrete arm covers every value, so the match is exhaustive. The
        // rigid row's branch excludes the concrete row (`covers` is per-arg
        // identity), so a witness escaping it would be a false missing case —
        // the branch must stay reachability-only.
        let mut cx = TestingCtx::new();
        let box_q = qtn("Box");
        cx.register(box_q.clone(), vec![int_ty()]);
        let box_int = Ty::Class(box_q.clone(), vec![int_ty()], Default::default());

        let rigid_refutable = DPat::class_inst(
            box_q.clone(),
            vec![type_var_ty("T")],
            vec![DPat::single(int_lit(1), int_ty())],
            box_int.clone(),
        );
        let concrete_cover = DPat::class_inst(
            box_q.clone(),
            vec![int_ty()],
            vec![DPat::wildcard(int_ty())],
            box_int.clone(),
        );
        let report = compute_match_usefulness(
            &cx,
            &[rigid_refutable.clone(), concrete_cover],
            box_int.clone(),
        );
        assert!(
            report.missing.is_empty(),
            "the concrete arm covers everything; false witnesses: {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );
        assert!(
            report.unreachable_arms.is_empty(),
            "the rigid arm stays reachable through its own branch: {:?}",
            report.unreachable_arms
        );

        // Sole rigid arm: genuinely non-exhaustive, and the missing case is
        // reported exactly once — the reachability-only branch does not
        // duplicate the alphabet branch's witness.
        let report = compute_match_usefulness(&cx, &[rigid_refutable], box_int);
        let missing: Vec<String> = report.missing.iter().map(|w| w.to_string()).collect();
        assert_eq!(missing.len(), 1, "one witness, reported once: {missing:?}");
        assert!(report.unreachable_arms.is_empty());
    }

    #[test]
    fn rigid_single_head_in_list_column_does_not_manufacture_witnesses() {
        // `match (h: Holder) { Holder { xs: let t: T[], n: 1 } =>,
        // Holder { xs: [..], n: _ } => }` with `xs: string[]`: the slice arm
        // covers every length class, so the match is exhaustive. The rigid
        // `Single` head's branch excludes the slice row (a slice ctor never
        // covers a `Single`), and the refutable sibling column `n: 1` makes
        // that branch non-exhaustive internally — its witness must be dropped.
        let mut cx = TestingCtx::new();
        let holder_q = qtn("Holder");
        let strings = list_of(Ty::String {
            attr: Default::default(),
        });
        cx.register(holder_q.clone(), vec![strings.clone(), int_ty()]);
        let holder = Ty::Class(holder_q.clone(), vec![], Default::default());

        let rigid_row = DPat::class(
            holder_q.clone(),
            vec![
                DPat::single(list_of(type_var_ty("T")), strings.clone()),
                DPat::single(int_lit(1), int_ty()),
            ],
            holder.clone(),
        );
        let cover_row = DPat::class(
            holder_q.clone(),
            vec![
                DPat::slice(
                    SliceShape::Variable {
                        prefix: 0,
                        suffix: 0,
                    },
                    vec![],
                    strings.clone(),
                ),
                DPat::wildcard(int_ty()),
            ],
            holder.clone(),
        );
        let report = compute_match_usefulness(&cx, &[rigid_row, cover_row], holder);
        assert!(
            report.missing.is_empty(),
            "the slice arm covers everything; false witnesses: {:?}",
            report
                .missing
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
        );
        assert!(
            report.unreachable_arms.is_empty(),
            "the rigid arm stays reachable through its own branch: {:?}",
            report.unreachable_arms
        );
    }
}
