//! Body type inference: `infer_body` walks one body owner's expression tree
//! with an `InferenceContext` over an [`unify::InferenceTable`].
//!
//! S9 state: bidirectional checking. `infer_expr` synthesizes with an
//! `Expectation` flowing down (informing shape: container elements,
//! if-branch pass-through, lambda signature deduction); `check_expr`
//! additionally emits a `Sub` constraint, discharged eagerly per the settled
//! design - invariant heads decay to `Eq`, ground pairs ask the canonical
//! oracle, var-headed pairs deposit bounds, the irreducible residue defers
//! to finish. Control-flow merge points join through `union_of` - the
//! canonical union when members are var-free, syntactic until resolution
//! otherwise (never fabricated at variables - ruling 1);
//! `Diverges` tracks never-propagation. Value paths resolve through one
//! entry (`resolve_value_path`); lambdas deduce unwritten signature slots
//! from the expected function type and their bodies type in the owner's
//! table under the lambda's scope. Constructs the engine does not handle
//! yet still record the `Error` sentinel and upgrade slice by slice.

pub(crate) mod flow;
pub(crate) mod obligations;
pub(crate) mod pat;
pub mod unify;

use std::sync::Arc;

use baml_compiler2_ast::{
    Expr, ExprBody, ExprId, PatId, Pattern, Stmt, StmtId, traverse::BodyNode,
};
use baml_compiler2_hir::{
    body::BodyOwnerId,
    body_type_refs::BodyTypeRefs,
    scope::FileScopeId,
    semantic_index::{
        BindingId, BindingKind, ExprMetadataKey, ExprMetadataScope, FileSemanticIndex,
        PathResolution,
    },
};
use baml_type::{
    Freshness, Literal, TyAttr,
    interned::{InterfaceRef, Ty, TyKind},
    normalize::{canonical_union_interned, equivalent_interned, is_subtype_interned},
};
use rustc_hash::FxHashMap;

use crate::{
    facts::Facts,
    infer::unify::InferenceTable,
    lower::{
        LowerCtx, function_generic_frame, function_signature, lower_ctx_for_file, substitute_params,
    },
};

/// The unit-type identification (ruling: interim until tuples): `void`
/// and `null` both denote the single-value unit.
fn is_unit(ty: &Ty) -> bool {
    matches!(ty.kind(), TyKind::Void { .. } | TyKind::Null { .. })
}

/// The implicit `baml.spawn.SpawnParams<V, E>` a spawn's `with` chain
/// threads (BEP-034).
fn spawn_params_ty(value: Ty, error: Ty) -> Ty {
    Ty::intern(TyKind::Class(
        baml_type::TypeName::new(
            baml_type::Name::new("baml"),
            vec![baml_type::Name::new("spawn")],
            baml_type::Name::new("SpawnParams"),
        ),
        Box::new([value, error]),
        TyAttr::default(),
    ))
}

fn is_spawn_params_qtn(qtn: &baml_type::TypeName) -> bool {
    qtn.package().as_str() == "baml"
        && qtn.namespace().len() == 1
        && qtn.namespace()[0].as_str() == "spawn"
        && qtn.name().as_str() == "SpawnParams"
}

/// Negate a numeric literal into the negative literal TYPE (ruling 2:
/// `-1` is a type, TS parity). Freshness carries through. `None` skips
/// the fold: non-numeric literals, and an int result outside BAML's i63
/// value range (`-INT_MIN` = 2^62) - the unfolded dispatch result stands
/// and the VM raises the catchable overflow, identical to the
/// through-a-variable path (TIR's `fold_int` rule).
/// Where a type-qualified reference's OWN generic args come from -
/// r-a's `substs_from_path` split: the call site's turbofish channel,
/// or fresh variables for a value-position reference.
#[derive(Clone, Copy)]
enum OwnArgs {
    Call(ExprId),
    Fresh,
}

/// BAML's int value range: i63 (the VM tags the low bit). One
/// crate-level pair, mirroring TIR's layout and the
/// `baml_type::MAX_BIGINT_BITS` precedent; a folded result outside it
/// defers to the runtime overflow.
const INT_MIN: i64 = -(1 << 62);
const INT_MAX: i64 = (1 << 62) - 1;

fn negate_literal(lit: &Literal, freshness: Freshness) -> Option<Ty> {
    let negated = match lit {
        Literal::Int(n) => {
            let v = n.checked_neg()?;
            if !(INT_MIN..=INT_MAX).contains(&v) {
                return None;
            }
            Literal::Int(v)
        }
        Literal::Bigint(n) => Literal::Bigint(-n.clone()),
        // The float's WRITTEN digits are preserved exactly: negation is a
        // sign-prefix toggle, never a parse/format round trip.
        Literal::Float(text) => Literal::Float(match text.strip_prefix('-') {
            Some(rest) => rest.to_owned(),
            None => format!("-{text}"),
        }),
        Literal::String(_) | Literal::Bool(_) => return None,
    };
    Some(Ty::intern(TyKind::Literal(
        negated,
        freshness,
        TyAttr::default(),
    )))
}

/// Fold a binary operator over two literal TYPES into the literal
/// result - literal types are closed under the builtin operators
/// (RULING 2026-08-07; TIR's `try_fold_binary` lifted to the interned
/// layer; a deliberate divergence from TS, which never folds).
/// Freshness merges - fresh operands make a fresh result, so bindings
/// still widen (`let x = 1n + 2n` is `bigint`; only checked positions
/// see `3n`). `None` skips the fold: non-literal operands, mixed
/// bases, and any result the VM could not materialize (i63 overflow,
/// the bigint allocation cap, division by zero, non-finite floats) -
/// the unfolded dispatch result stands and the runtime raises the
/// same catchable error the through-a-variable path gets.
fn const_fold_binary(op: baml_compiler2_ast::BinaryOp, lhs: &Ty, rhs: &Ty) -> Option<Ty> {
    use baml_compiler2_ast::BinaryOp;
    let (TyKind::Literal(a, a_fresh, _), TyKind::Literal(b, b_fresh, _)) = (lhs.kind(), rhs.kind())
    else {
        return None;
    };
    let freshness = match (a_fresh, b_fresh) {
        (Freshness::Regular, Freshness::Regular) => Freshness::Regular,
        _ => Freshness::Fresh,
    };
    let lit = |value: Literal| Ty::intern(TyKind::Literal(value, freshness, TyAttr::default()));
    let boolean = |value: bool| Some(lit(Literal::Bool(value)));
    let int = |value: i64| {
        (INT_MIN..=INT_MAX)
            .contains(&value)
            .then(|| lit(Literal::Int(value)))
    };
    match (a, b) {
        (Literal::Int(a), Literal::Int(b)) => {
            let (a, b) = (*a, *b);
            match op {
                BinaryOp::Add => int(a.checked_add(b)?),
                BinaryOp::Sub => int(a.checked_sub(b)?),
                BinaryOp::Mul => int(a.checked_mul(b)?),
                BinaryOp::Div => int(a.checked_div(b)?),
                BinaryOp::Mod => int(a.checked_rem(b)?),
                BinaryOp::BitAnd => Some(lit(Literal::Int(a & b))),
                BinaryOp::BitOr => Some(lit(Literal::Int(a | b))),
                BinaryOp::BitXor => Some(lit(Literal::Int(a ^ b))),
                // Shifts range-check the RESULT too (`1 << 62` escapes
                // i63); bad counts defer to the runtime throw.
                BinaryOp::Shl => int(a.checked_shl(u32::try_from(b).ok()?)?),
                BinaryOp::Shr => int(a.checked_shr(u32::try_from(b).ok()?)?),
                BinaryOp::Eq => boolean(a == b),
                BinaryOp::Ne => boolean(a != b),
                BinaryOp::Lt => boolean(a < b),
                BinaryOp::Le => boolean(a <= b),
                BinaryOp::Gt => boolean(a > b),
                BinaryOp::Ge => boolean(a >= b),
                _ => None,
            }
        }
        (Literal::Bigint(a), Literal::Bigint(b)) => {
            use num_bigint::{BigInt, Sign};
            let capped = |value: BigInt| {
                (value.bits() <= baml_type::MAX_BIGINT_BITS).then(|| lit(Literal::Bigint(value)))
            };
            match op {
                BinaryOp::Add => capped(a + b),
                BinaryOp::Sub => capped(a - b),
                BinaryOp::Mul => {
                    // Pre-flight matching `Instruction::MulBigint`.
                    if a.bits().saturating_add(b.bits()) > baml_type::MAX_BIGINT_BITS {
                        return None;
                    }
                    capped(a * b)
                }
                BinaryOp::Div if b.sign() != Sign::NoSign => capped(a / b),
                BinaryOp::Mod if b.sign() != Sign::NoSign => capped(a % b),
                BinaryOp::BitAnd => capped(a & b),
                BinaryOp::BitOr => capped(a | b),
                BinaryOp::BitXor => capped(a ^ b),
                // Shl growth is bounded by the allocation cap; a count
                // past `usize` (or past the cap) defers to the runtime
                // AllocFailure. Shr cannot grow: huge counts saturate
                // to `0n` / `-1n`, mirroring `Instruction::ShrBigint`.
                BinaryOp::Shl if b.sign() != Sign::Minus => {
                    let shift = usize::try_from(b).ok()?;
                    if a.bits().saturating_add(u64::try_from(shift).ok()?)
                        > baml_type::MAX_BIGINT_BITS
                    {
                        return None;
                    }
                    capped(a << shift)
                }
                BinaryOp::Shr if b.sign() != Sign::Minus => match usize::try_from(b) {
                    Ok(shift) => capped(a >> shift),
                    Err(_) => capped(if a.sign() == Sign::Minus {
                        BigInt::from(-1)
                    } else {
                        BigInt::ZERO
                    }),
                },
                BinaryOp::Eq => boolean(a == b),
                BinaryOp::Ne => boolean(a != b),
                BinaryOp::Lt => boolean(a < b),
                BinaryOp::Le => boolean(a <= b),
                BinaryOp::Gt => boolean(a > b),
                BinaryOp::Ge => boolean(a >= b),
                _ => None,
            }
        }
        (Literal::Float(a_text), Literal::Float(b_text)) => {
            let a: f64 = a_text.parse().ok()?;
            let b: f64 = b_text.parse().ok()?;
            let float = |value: f64| Some(lit(Literal::Float(format_float(value)?)));
            match op {
                BinaryOp::Add => float(a + b),
                BinaryOp::Sub => float(a - b),
                BinaryOp::Mul => float(a * b),
                BinaryOp::Div if b != 0.0 => float(a / b),
                BinaryOp::Mod if b != 0.0 => float(a % b),
                #[allow(clippy::float_cmp)] // Intentional: literal float equality.
                BinaryOp::Eq => boolean(a == b),
                #[allow(clippy::float_cmp)] // Intentional: literal float inequality.
                BinaryOp::Ne => boolean(a != b),
                BinaryOp::Lt => boolean(a < b),
                BinaryOp::Le => boolean(a <= b),
                BinaryOp::Gt => boolean(a > b),
                BinaryOp::Ge => boolean(a >= b),
                _ => None,
            }
        }
        (Literal::Bool(a), Literal::Bool(b)) => match op {
            BinaryOp::And => boolean(*a && *b),
            BinaryOp::Or => boolean(*a || *b),
            BinaryOp::Eq => boolean(a == b),
            BinaryOp::Ne => boolean(a != b),
            _ => None,
        },
        (Literal::String(a), Literal::String(b)) => match op {
            BinaryOp::Add => Some(lit(Literal::String(format!("{a}{b}")))),
            BinaryOp::Eq => boolean(a == b),
            BinaryOp::Ne => boolean(a != b),
            BinaryOp::Lt => boolean(a < b),
            BinaryOp::Le => boolean(a <= b),
            BinaryOp::Gt => boolean(a > b),
            BinaryOp::Ge => boolean(a >= b),
            _ => None,
        },
        _ => None,
    }
}

/// A folded float formatted so it always reads back as a float (a
/// trailing `.0` when the value prints integral); non-finite results
/// refuse the fold.
fn format_float(value: f64) -> Option<String> {
    if !value.is_finite() {
        return None;
    }
    let text = format!("{value}");
    if text.contains('.') {
        Some(text)
    } else {
        Some(format!("{text}.0"))
    }
}

/// The STRUCTURAL third of union canonicalization, safe on
/// var-carrying members: flatten nested unions, drop `never`, dedup
/// identical members, collapse the empty union to `never` and the
/// singleton to its member. rustc keeps aggregate constructors
/// invariant-preserving the same way (a `dyn` existential-predicate
/// list is sorted and deduped BY CONSTRUCTION), and TS's
/// `getUnionType` flattens/dedups structurally before any semantic
/// work. The SEMANTIC pieces - absorption, literal-into-base - stay
/// with the oracle-consulting canonical path, which requires var-free
/// input. Without the collapse, `Union([?0])` leaks into unification,
/// whose union arm is positional - it could never unify with the
/// solved member.
fn syntactic_union(members: &[Ty]) -> Ty {
    fn push(flat: &mut Vec<Ty>, ty: &Ty) {
        match ty.kind() {
            TyKind::Union(inner, _) => {
                for member in inner {
                    push(flat, member);
                }
            }
            TyKind::Never { .. } => {}
            _ => {
                if !flat.contains(ty) {
                    flat.push(ty.clone());
                }
            }
        }
    }
    let mut flat: Vec<Ty> = Vec::new();
    for member in members {
        push(&mut flat, member);
    }
    match flat.len() {
        0 => Ty::never(),
        1 => flat.pop().expect("length checked"),
        _ => Ty::union(flat),
    }
}

/// TIR's `function_params_runtime_compatible`, verbatim: same arity,
/// same modes, and OPTIONAL parameters keep their names (named at the
/// call site and in the runtime's defaulted-slot filling); required
/// parameters may rename freely.
fn function_params_runtime_compatible(
    source: &[baml_type::interned::FunctionParam],
    target: &[baml_type::interned::FunctionParam],
) -> bool {
    source.len() == target.len()
        && source.iter().zip(target).all(|(source, target)| {
            source.mode == target.mode
                && (source.mode == baml_type::FunctionParamMode::Required
                    || source.name == target.name)
        })
}

/// Reduction budget for the finalize-time projection pass: bounds a
/// reduction CHAIN (`(A as I).X` -> `(B as J).Y` -> ...), the same
/// discipline as the canonical walk's fuel. Any real chain is far
/// shorter; a cyclic binding is a declaration-level error caught
/// elsewhere.
const PROJECTION_FINALIZE_FUEL: u32 = 32;

/// One member access's resolution: which declaration the member refers
/// to, through which dispatch mode - TIR's `MemberResolution` shape,
/// proven by MIR's consumption. The rust-analyzer equivalent is the
/// `method_resolutions`/`field_resolutions`/`variant_resolutions`/
/// `assoc_resolutions` table family; BAML expresses it as one enum
/// because virtual interface dispatch adds modes Rust encodes elsewhere,
/// and splitting would lose the mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberResolution<'db> {
    /// A class field access (`p.name`).
    Field {
        class: baml_compiler2_hir::loc::ClassLoc<'db>,
        field: baml_type::Name,
    },
    /// An enum variant access (`Status.Active`).
    Variant {
        enum_loc: baml_compiler2_hir::loc::EnumLoc<'db>,
        variant: baml_type::Name,
    },
    /// A free function named by a package/namespace path.
    Free {
        func: baml_compiler2_hir::loc::FunctionLoc<'db>,
    },
    CompiledFree {
        function: baml_package_interface::PackageItemId,
    },
    /// A method on a VALUE receiver (`p.get_name`): `self` is bound.
    BoundMethod {
        class: baml_compiler2_hir::loc::ClassLoc<'db>,
        func: baml_compiler2_hir::loc::FunctionLoc<'db>,
    },
    /// A method behind a TYPE qualifier (`Person.get_name`,
    /// `Array.filled`): no receiver, `self` stays a parameter.
    UnboundMethod {
        class: baml_compiler2_hir::loc::ClassLoc<'db>,
        func: baml_compiler2_hir::loc::FunctionLoc<'db>,
    },
    CompiledBoundMethod {
        class: baml_type::TypeName,
        method: baml_type::Name,
    },
    CompiledUnboundMethod {
        class: baml_type::TypeName,
        method: baml_type::Name,
    },
    /// A VIRTUAL interface-method call: only the slot (interface +
    /// member) is statically known; dispatch resolves to the receiver's
    /// runtime impl.
    InterfaceVirtualMethod {
        interface: baml_compiler2_hir::loc::InterfaceLoc<'db>,
        method: baml_type::Name,
    },
    /// A CONCRETE interface-method call through a statically-matched
    /// impl: `func` is the impl's override, or the interface's default
    /// body when inherited.
    InterfaceConcreteMethod {
        impl_block: baml_compiler2_hir::loc::ImplLoc<'db>,
        func: baml_compiler2_hir::loc::FunctionLoc<'db>,
    },
    /// A VIRTUAL interface-field access: read through the realized
    /// declaring-interface view (`view`, the runtime resolver's key)
    /// at `field_index` in that interface's own declared field list.
    InterfaceVirtualField {
        interface: baml_compiler2_hir::loc::InterfaceLoc<'db>,
        view: Ty,
        field_index: u32,
        field: baml_type::Name,
    },
    CompiledInterfaceVirtualField {
        interface: baml_type::TypeName,
        view: Ty,
        field_index: u32,
        field: baml_type::Name,
    },
}

/// A value-rooted path expression's resolved ladder (`a.b.c`: one
/// `Expr::Path` node, unnumbered segments). rustc keys per-segment info
/// by `HirId` because every segment has one; BAML's flat path AST does
/// not, so the ladder lives in one entry per path expression - one
/// structure, not TIR's three parallel maps. Package-rooted paths
/// (statics, variants, free items) resolve as a whole and record only a
/// `MemberResolution`; this table is for paths whose ROOT is a value.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPath<'db> {
    /// One entry per written segment: entry 0 is the root (a local,
    /// parameter, or template param - no member resolution), entry
    /// `i > 0` the member access the `i`-th segment performs.
    pub segments: Vec<ResolvedPathSegment<'db>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPathSegment<'db> {
    /// The value's type AFTER this segment.
    pub ty: Ty,
    pub resolution: Option<MemberResolution<'db>>,
}

/// A recorded coercion step at an expression, consumed structurally by
/// MIR lowering - rust-analyzer's `Adjustment { kind, target }` exactly
/// (their infer.rs Adjust family: NeverToAny/Deref/Borrow/Pointer; BAML
/// has ONE adjustment kind today). The source shape is `type_of_expr`;
/// the target shape is here - TIR's bespoke `FunctionCoercion` struct
/// carried both redundantly.
#[derive(Debug, Clone, PartialEq)]
pub struct Adjustment {
    pub kind: Adjust,
    /// The post-adjustment type (the expectation the value was adapted
    /// to).
    pub target: Ty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Adjust {
    /// The optional-parameter adapter: a function value satisfies its
    /// expectation by SUBTYPING but not by RUNTIME SHAPE (arity, mode,
    /// or optional-parameter names drift), so lowering synthesizes an
    /// adapter closure (TIR's `function_coercion_for` rule).
    FunctionAdapter,
}

/// One call's argument-to-parameter matching and solved instantiation -
/// TIR's `CallPlan` minus its dead fields (`instantiated_throws` was
/// TIR-internal; `call_type_instantiations` had no consumer). Rust needs
/// no analog (no named/default arguments); the prior art is Swift's
/// Sema-recorded argument matching consumed by `SILGen`. Keyed by the CALL
/// expression.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CallPlan {
    /// Parameter-ordered bindings over the callee's parameter list MINUS
    /// any bound receiver slot (the list written arguments match).
    /// Required parameters with no argument get no entry (the arity
    /// diagnostic is S17's).
    pub bindings: Vec<ParamBinding>,
    /// The callee's solved generic instantiation in declared De Bruijn
    /// order (owner frame prefix + own suffix). Recorded raw at the
    /// instantiation site; ground after writeback.
    pub type_args: Vec<Ty>,
    /// How many leading `type_args` are the OWNER frame's (receiver
    /// class args for bound methods, the interface frame for virtual
    /// calls). The runtime call convention threads only the suffix as
    /// call operands - the receiver/impl frame supplies the prefix - so
    /// consumers slice here (TIR records the suffix alone; recording the
    /// full instantiation plus the split keeps the whole solution
    /// available, r-a's `node_args` shape).
    pub own_offset: usize,
    /// Whether written turbofish filled the args. The runtime plan then
    /// carries none: MIR lowers the WRITTEN types itself (TIR gates its
    /// recording on `!explicit_args_used`), and a plan entry would
    /// double-emit the operands.
    pub explicit: bool,
    /// The trailing `$id = ...` side-channel argument (TIR's
    /// `CallSideChannels`, flattened until a second channel exists).
    pub runtime_id: Option<ExprId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamBinding {
    Provided {
        param_index: usize,
        arg: ExprId,
    },
    OmittedDefault {
        param_index: usize,
        param_name: baml_type::Name,
    },
}

/// Inference side tables for one body owner, keyed by arena ids, mirroring
/// rust-analyzer's `InferenceResult`. Types are the hash-consed
/// `baml_type::interned` representation (this crate's native vocabulary);
/// they are materialized to plain `baml_type::Ty` only at consumer
/// boundaries, after resolve-all guarantees no inference variables remain.
/// Which BEP-049 SS10 rule a tagged-template tag broke.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TaggedTagIssue {
    NotAFunction,
    NotMarked,
    BadBodyParam,
}

/// Where a written `_` hole sits: an expression's turbofish, or a
/// body-position type annotation's ref.
#[derive(Debug, Clone, Copy, PartialEq)]
enum HoleAnchor {
    TypeRef(baml_compiler2_hir::type_ref::TypeRefId),
}

/// S17 pending diagnostic (engine-internal): arena-anchored, payload
/// types interned (still var-carrying until finish); finalized into the
/// shared vocabulary with PLAIN types at writeback. Short-lived and
/// vec-collected, so variant size disparity is immaterial.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
enum PendingDiag<'db> {
    NonExhaustiveMatch {
        expr: ExprId,
        scrutinee: Ty,
        missing: Vec<String>,
    },
    UnreachableArm {
        expr: ExprId,
        /// Catch-road unreachability is advisory (a warning); a match
        /// matrix's unreachable arm is an error (canary's split).
        warning: bool,
    },
    UnresolvedName {
        expr: ExprId,
        name: baml_type::Name,
    },
    UnresolvedMember {
        expr: ExprId,
        base: Ty,
        member: baml_type::Name,
    },
    /// A body annotation failing the written-type well-formedness
    /// judgment (generic-argument bounds).
    AnnotWf {
        type_ref: baml_compiler2_hir::type_ref::TypeRefId,
        error: crate::diagnostics::TirTypeError,
    },
    /// A tagged-template tag failing BEP-049 SS10's contract.
    TaggedTagInvalid {
        at: ExprId,
        name: baml_type::Name,
        func: Option<baml_compiler2_hir::loc::FunctionLoc<'db>>,
        kind: TaggedTagIssue,
    },
    /// E0147: a written `_` in an expression-position type argument.
    ExprPositionHole {
        expr: ExprId,
    },
    /// E0150: an `int` literal outside the VM's i63 value range.
    IntLiteralOutOfRange {
        expr: ExprId,
        value: i64,
    },
    /// BEP-044 method disambiguation: `member` on `base` is declared by
    /// two or more realized-distinct interfaces (rustc's E0034 shape) -
    /// resolving would silently pick one, so the access must qualify
    /// with `as<I>`.
    AmbiguousMember {
        expr: ExprId,
        base: Ty,
        member: baml_type::Name,
        sources: Vec<baml_type::interned::InterfaceRef>,
        is_field: bool,
    },
    /// An interface FIELD reached on a concrete receiver: reachable only
    /// through an explicit `obj.as<I>.field` projection.
    FieldRequiresProjection {
        expr: ExprId,
        base: Ty,
        member: baml_type::Name,
        interface: baml_type::interned::InterfaceRef,
    },
    /// A method declared with `Self` outside the receiver position,
    /// called through an existential/union receiver (Rust's `dyn`
    /// object-safety split).
    SelfRestrictedMember {
        expr: ExprId,
        interface: baml_type::interned::InterfaceRef,
        member: baml_type::Name,
        position: crate::diagnostics::SelfCallPosition,
    },
    /// A member on a union receiver with no interface shared by every
    /// arm declaring it.
    UnionNoCommonInterface {
        expr: ExprId,
        base: Ty,
        member: baml_type::Name,
    },
    /// An abstract type (union or interface existential) instantiating an
    /// interface-bounded generic parameter - no single runtime type to
    /// dispatch on (E0001).
    BoundedArgNotConcrete {
        expr: ExprId,
        arg: Ty,
        bound: baml_type::interned::InterfaceRef,
    },
    /// A constructor entry naming an implemented interface's FIELD; the
    /// backing class field is the constructor's key.
    InterfaceFieldInConstruction {
        object: ExprId,
        name: baml_type::Name,
        class_field: baml_type::Name,
    },
    /// `.as<T>` with a non-interface target.
    UpcastTargetNotInterface {
        expr: ExprId,
        target: Ty,
    },
    /// `.as<I>` where the value's type does not implement `I`.
    UpcastNotImplemented {
        expr: ExprId,
        value: Ty,
        interface: Ty,
    },
    /// Explicit turbofish count disagrees with the callee's declared
    /// (writable) generic params.
    WrongTypeArgArity {
        expr: ExprId,
        callee: baml_type::Name,
        expected: usize,
        got: usize,
    },
    NotCallable {
        expr: ExprId,
        ty: Ty,
    },
    ArgCountMismatch {
        expr: ExprId,
        expected: usize,
        got: usize,
    },
    UnknownNamedArg {
        expr: ExprId,
        name: baml_type::Name,
    },
    RefutableLet {
        pat: PatId,
        context: crate::diagnostics::IrrefutableContextKind,
    },
    /// A NAMED call leaving a required parameter unfilled - reported by
    /// name, not count.
    MissingNamedArg {
        expr: ExprId,
        name: baml_type::Name,
    },
    /// A positional argument landing on a DEFAULTED (by-name-only)
    /// parameter.
    PositionalDefaultedArg {
        expr: ExprId,
        name: baml_type::Name,
    },
    /// A constructor head naming no class (with near-matches).
    UnresolvedCtor {
        expr: ExprId,
        name: baml_type::Name,
        suggestions: Box<[baml_type::Name]>,
    },
    /// A positional argument after a named one.
    PositionalAfterNamed {
        expr: ExprId,
    },
    /// The same argument name supplied twice.
    DuplicateNamedArg {
        expr: ExprId,
        name: baml_type::Name,
    },
    /// A generic construction whose parameter no field determined: the
    /// fresh instantiation slot survives to finalize unsolved (E-code:
    /// cannot infer type parameter).
    UninferredCtorParam {
        expr: ExprId,
        var: Ty,
        name: baml_type::Name,
    },
    /// A `spawn ... with` link that is not a middleware transformer
    /// (TIR's `SpawnWithNotATransformer`: names the contract and the link's
    /// concrete input).
    SpawnWithBad {
        at: ExprId,
        expected_input: Ty,
        got: Ty,
    },
    /// E0097: declared throws members the body can never throw (warning).
    ExtraneousThrows {
        at: ExprId,
        extra_types: Vec<String>,
    },
    /// Control flow that would escape a `defer` body (BEP-042): `return`
    /// always; `break`/`continue` unless a loop opened INSIDE the defer.
    DeferEscape {
        stmt: Option<StmtId>,
        expr: Option<ExprId>,
        keyword: &'static str,
    },
    /// An untyped-object property shorthand naming no in-scope value -
    /// the specialized spelling of unresolved-name, with near-matches.
    UnresolvedShorthand {
        expr: ExprId,
        name: baml_type::Name,
        suggestions: Vec<baml_type::Name>,
    },
    /// The comparison rules (TIR's family): a provably-disjoint `==`/`!=`
    /// pair warns (the comparison is a constant), and the ordering
    /// operators demand SAME-typed, `baml.ops.Compare`-implementing
    /// operands.
    ComparisonAlwaysDisjoint {
        at: ExprId,
        op: baml_compiler2_ast::BinaryOp,
        lhs: Ty,
        rhs: Ty,
    },
    OrderingDifferentTypes {
        at: ExprId,
        op: baml_compiler2_ast::BinaryOp,
        lhs: Ty,
        rhs: Ty,
    },
    OrderingRequiresCompare {
        at: ExprId,
        op: baml_compiler2_ast::BinaryOp,
        ty: Ty,
    },
    /// An object-literal entry naming no declared field of the constructed
    /// class - the plain spelling and the property-shorthand spelling get
    /// their own messages (TIR's pair). Shorthand is detected structurally
    /// (the value is a bare path spelling the field name), keeping
    /// inference span-free.
    UnknownObjectField {
        object: ExprId,
        value: ExprId,
        class_name: baml_type::QualifiedTypeName,
        declared: Vec<baml_type::Name>,
        name: baml_type::Name,
        shorthand: bool,
    },
    /// The call-site `$id` side channel's three rules: the value must be
    /// `boundary.LocalId`, at most one `$id` per call, and it must be the
    /// last argument (TIR's family, verbatim).
    RuntimeIdArgMismatch {
        at: ExprId,
        got: Ty,
    },
    DuplicateRuntimeIdArg {
        at: ExprId,
    },
    RuntimeIdArgNotLast {
        at: ExprId,
    },
    /// A throw site (or a callee's propagated effect) whose contribution
    /// escapes the CLOSED declared clause: TIR's throws-contract family,
    /// rendered as the dedicated E-code rather than a generic mismatch.
    ThrowsViolation {
        at: ExprId,
        declared: Ty,
        extra: Ty,
    },
    /// A `match`/`is` type pattern PROVABLY dead against its scrutinee - no
    /// realization of the in-scope rigid type variables gives them a common
    /// value (the overlap oracle's `No`). Reported like any concrete
    /// pattern/scrutinee mismatch (TIR's policy and message).
    PatternScrutMismatch {
        pat: PatId,
        expected: Ty,
        found: Ty,
    },
    OperatorNotApplicable {
        expr: ExprId,
        interface: &'static str,
        lhs: Ty,
        rhs: Option<Ty>,
    },
    BodyAnnot {
        type_ref: baml_compiler2_hir::type_ref::TypeRefId,
        kind: crate::lower::LoweringDiagKind,
    },
    InterpolatedMaybeNull {
        expr: ExprId,
        ty: Ty,
    },
    GenericDestructureNoArgs {
        pat: PatId,
        class_name: baml_type::Name,
    },
    RestNotBinding {
        pat: PatId,
    },
    UnresolvedPatternName {
        pat: PatId,
        name: baml_type::Name,
    },
    VoidResultUsed {
        expr: ExprId,
    },
    DeadCode {
        at: baml_compiler2_ast::StmtId,
        unreachable_count: usize,
    },
    RuntimeIdMember {
        expr: ExprId,
        member: baml_type::Name,
    },
    RuntimeIdCompoundAssignment {
        expr: ExprId,
    },
    UnknownPatternField {
        pat: PatId,
        class_name: baml_type::QualifiedTypeName,
        field_name: baml_type::Name,
        declared: Vec<baml_type::Name>,
    },
    UnnecessaryOptionalChain {
        expr: ExprId,
        expr_text: String,
        base_text: String,
    },
    OrBindingConflict {
        pat: PatId,
        name: baml_type::Name,
        first: Ty,
        other: Ty,
    },
}

/// Grows one map per slice; consumers must treat a missing entry as "not
/// inferred", never as an error.
#[derive(Debug, Clone, PartialEq)]
pub struct InferenceResult<'db> {
    pub type_of_expr: FxHashMap<ExprId, Ty>,
    pub type_of_pat: FxHashMap<PatId, Ty>,
    /// The owner's effect: the declared clause when written, else the
    /// canonical union of the body's throw sites and callee throws
    /// (`never` when nothing throws) - S12.
    pub throws: Ty,
    /// Definite check failures, keyed by the checked expression:
    /// `(expected, actual)`. Recorded always (rust-analyzer's discipline);
    /// rendered as diagnostics in S17.
    pub type_mismatches: FxHashMap<ExprId, (Ty, Ty)>,
    /// Match expressions whose unguarded arms do not cover the scrutinee.
    /// The expression types as Error; S17 renders E0062 with witnesses.
    pub non_exhaustive_matches: rustc_hash::FxHashSet<ExprId>,
    /// USER-FACING diagnostics (S17): the shared vocabulary, payloads
    /// finalized to plain types at finish. The check layer resolves the
    /// arena-ID anchors to spans and hands them to the one message stack.
    pub diagnostics: Vec<crate::diagnostics::TirDiagnostic<'db>>,
    /// Member accesses and callees resolved to their declarations, keyed
    /// by the accessing expression (a call's entry sits on the CALLEE
    /// expr, TIR's keying). S16: MIR consumes this instead of re-running
    /// resolution.
    pub member_resolutions: FxHashMap<ExprId, MemberResolution<'db>>,
    /// Value-rooted multi-segment paths' per-segment ladders. S16: MIR's
    /// field-chain-vs-method decisions read the ladder instead of
    /// re-resolving.
    pub path_resolutions: FxHashMap<ExprId, ResolvedPath<'db>>,
    /// Per-call argument matching and solved instantiations, keyed by the
    /// CALL expression. S16: MIR's argument emission and `LoadType`
    /// operands read this instead of re-planning.
    pub call_plans: FxHashMap<ExprId, CallPlan>,
    /// Coercion steps per expression (r-a's `expr_adjustments` shape).
    /// S16: MIR synthesizes the recorded adapters instead of re-deciding.
    pub expr_adjustments: FxHashMap<ExprId, Box<[Adjustment]>>,
    /// Callee expressions the walk resolved through a LANGUAGE-SUGAR
    /// tier (`to_string`/`to_json`/`from_json` lang-item desugars).
    /// Recorded as POSITIVE knowledge; TIR's convention leaves these
    /// callees untyped and MIR keys the desugar on that absence, so the
    /// provider omits their expr types (post-flip, MIR reads this table
    /// directly instead of an absence).
    pub desugared_callees: rustc_hash::FxHashSet<ExprId>,
}

impl Default for InferenceResult<'_> {
    fn default() -> Self {
        InferenceResult {
            type_of_expr: FxHashMap::default(),
            type_of_pat: FxHashMap::default(),
            throws: Ty::never(),
            type_mismatches: FxHashMap::default(),
            non_exhaustive_matches: rustc_hash::FxHashSet::default(),
            diagnostics: Vec::new(),
            member_resolutions: FxHashMap::default(),
            path_resolutions: FxHashMap::default(),
            call_plans: FxHashMap::default(),
            expr_adjustments: FxHashMap::default(),
            desugared_callees: rustc_hash::FxHashSet::default(),
        }
    }
}

// SAFETY: PartialEq-driven overwrite, the CallableThrows precedent. The
// equality comparison IS the S3 firewall: an edit that re-executes
// `infer_body` but reproduces the same result cuts off every downstream
// consumer.
#[allow(unsafe_code)]
unsafe impl salsa::Update for InferenceResult<'_> {
    #[allow(unsafe_code)]
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        #[allow(unsafe_code)]
        unsafe {
            let changed = *old_pointer != new_value;
            if changed {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            changed
        }
    }
}

fn infer_function_body_cycle_initial<'db>(
    _db: &'db dyn baml_compiler2_ppir::Db,
    _id: salsa::Id,
    _function: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> InferenceResult<'db> {
    // The fixpoint seed for the signature/throws cycle
    // (`infer_body -> function_signature -> callable_throws ->
    // infer_body`): an empty result whose effect is `never`, consistent
    // with `callable_throws`' own seed.
    InferenceResult::default()
}

/// TRACKED (S2/S3): the crate's central query, per function. Inputs are
/// span-free by construction - the ppir body, the item type refs, the
/// body type refs, and the semantic index's structural joins (the
/// lambda-scope map replaced the last span dependence) - and the
/// PartialEq-driven `Update` gives downstream consumers early cutoff on
/// unchanged results.
#[salsa::tracked(returns(ref), cycle_initial = infer_function_body_cycle_initial)]
fn infer_function_body<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    function: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> InferenceResult<'db> {
    infer_body_impl(db, BodyOwnerId::Function(function))
}

/// TRACKED (S2/S3): top-level `let` bodies (no signature/throws cycle -
/// lets declare no clause and no callers instantiate them).
#[salsa::tracked(returns(ref))]
fn infer_let_body<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    let_binding: baml_compiler2_hir::loc::LetLoc<'db>,
) -> InferenceResult<'db> {
    infer_body_impl(db, BodyOwnerId::Let(let_binding))
}

/// TRACKED: a function's parameter-default arena as its own inference
/// root (r-a's `DefWithBodyId::VariantId` shape - small expression
/// contexts are ordinary body owners). No cycle seed: nothing on the
/// signature road consults default inference.
#[salsa::tracked(returns(ref))]
fn infer_parameter_defaults<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    function: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> InferenceResult<'db> {
    infer_body_impl(db, BodyOwnerId::ParameterDefaults(function))
}

/// Infers types for one body owner (function or top-level let), keyed by
/// the S1 `BodyOwnerId` (rust-analyzer's `DefWithBodyId` shape). Lambdas
/// are typed inside their owner's run; parameter defaults get their own
/// inference root later. A plain dispatcher over the per-loc tracked
/// queries (ppir's `body`/`body_scope` shape - `BodyOwnerId` is an
/// ordinary enum, not a salsa struct).
pub fn infer_body<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    owner: BodyOwnerId<'db>,
) -> &'db InferenceResult<'db> {
    match owner {
        BodyOwnerId::Function(function) => infer_function_body(db, function),
        BodyOwnerId::Let(let_binding) => infer_let_body(db, let_binding),
        BodyOwnerId::ParameterDefaults(function) => infer_parameter_defaults(db, function),
    }
}

/// Count of body inferences run this process - the bytecode cache's
/// warm-run evidence counter (a warm compile with full seeds should keep
/// this near zero for clean files).
static BODY_INFERENCES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Number of body inferences executed (not served from salsa memos or
/// seeds) since process start.
pub fn body_inferences() -> usize {
    BODY_INFERENCES.load(std::sync::atomic::Ordering::Relaxed)
}

fn infer_body_impl<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    owner: BodyOwnerId<'db>,
) -> InferenceResult<'db> {
    BODY_INFERENCES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let body = baml_compiler2_ppir::body(db, owner);
    let index = baml_compiler2_ppir::file_semantic_index(db, owner.file(db));
    let owner_scope = baml_compiler2_ppir::body_scope(db, owner).map(|s| s.file_scope_id(db));
    // The owner's generic frame makes `T` in body annotations resolve; the
    // signature gives parameter references their types and the body its
    // return expectation.
    let (frame, param_tys, return_ty, declared_throws_ref) = match owner {
        BodyOwnerId::Function(function) => {
            let signature = function_signature(db, function);
            let data = baml_compiler2_ppir::item_data::elaborated_function_data(db, function);
            (
                function_generic_frame(db, function),
                signature
                    .params
                    .iter()
                    .map(|param| param.ty.clone())
                    .collect(),
                Some(signature.ret.clone()),
                // The owner checks its throw sites against the RAW written
                // clause (holes preserved - a partial clause opens the
                // contract), never the caller-facing surface, which for a
                // partial clause is derived FROM those sites.
                data.throws.map(|throws| (&data.type_refs, throws)),
            )
        }
        // Defaults type in the FUNCTION's environment (its frame makes
        // `T` in a default resolve; its parameter types are each
        // default's expectation) with no return expectation and no
        // declared throws clause of their own.
        BodyOwnerId::ParameterDefaults(function) => {
            let signature = function_signature(db, function);
            (
                function_generic_frame(db, function),
                signature
                    .params
                    .iter()
                    .map(|param| param.ty.clone())
                    .collect(),
                None,
                None,
            )
        }
        BodyOwnerId::Let(_) => (Vec::new(), Vec::new(), None, None),
    };
    let bounds = match owner {
        BodyOwnerId::Function(function) | BodyOwnerId::ParameterDefaults(function) => {
            crate::lower::function_generic_bounds(db, function)
        }
        BodyOwnerId::Let(_) => FxHashMap::default(),
    };
    let concrete_self = match owner {
        BodyOwnerId::Function(function) | BodyOwnerId::ParameterDefaults(function) => {
            // BODY-position `Self` is a PLAIN-class-method error (the
            // ratified rule: signatures resolve it, bodies do not);
            // implements-block bodies (Self substitutes to the subject),
            // interface default bodies (frame slot 0), and free-impl
            // bodies keep theirs.
            match baml_compiler2_ppir::item_data::method_owner(db, function) {
                Some(baml_compiler2_ppir::item_data::MethodOwner::Class(_))
                    if baml_compiler2_ppir::item_data::method_interface_target(db, function)
                        .is_none() =>
                {
                    None
                }
                _ => crate::lower::owner_self_ty(db, function, &frame),
            }
        }
        BodyOwnerId::Let(_) => None,
    };
    let impl_target = match owner {
        BodyOwnerId::Function(function) | BodyOwnerId::ParameterDefaults(function) => {
            crate::lower::owner_impl_target(db, function, &frame)
        }
        BodyOwnerId::Let(_) => None,
    };
    let lower = lower_ctx_for_file(db, owner.file(db))
        .with_frame(frame)
        .with_bounds(bounds.clone())
        .with_self_ty(concrete_self)
        .with_impl_target(impl_target)
        .with_diagnostics();
    let type_refs = baml_compiler2_ppir::body_type_refs(db, owner);
    let plain_bounds = bounds
        .into_iter()
        .map(|(param, bounds)| {
            (
                param,
                bounds
                    .into_iter()
                    .map(|bound| {
                        baml_type::Interface::new(
                            bound.name.clone(),
                            bound.generics.iter().map(Ty::to_plain).collect(),
                            bound
                                .associated_types
                                .iter()
                                .map(|(name, ty)| (name.clone(), ty.to_plain()))
                                .collect(),
                        )
                    })
                    .collect(),
            )
        })
        .collect();
    // Split the declared clause into its named part and openness (spec
    // rule 3: `throws T | _` names T and opens the remainder to
    // inference); nested holes in named members stay ruling-4 errors.
    let (declared_throws, declared_throws_open) =
        match declared_throws_ref.map(|(store, throws)| lower.lower_type_ref(store, throws)) {
            Some(raw) => {
                let (named, open) = crate::lower::throws_clause_parts(&raw);
                (Some(named), open)
            }
            None => (None, false),
        };
    let mut ctx = InferenceContext::new(
        db,
        index,
        owner_scope,
        lower,
        param_tys,
        return_ty,
        type_refs,
        plain_bounds,
    );
    ctx.declared_throws = declared_throws;
    ctx.declared_throws_open = declared_throws_open;
    ctx.body_owner = match owner {
        BodyOwnerId::Function(function) | BodyOwnerId::ParameterDefaults(function) => {
            Some(function)
        }
        BodyOwnerId::Let(_) => None,
    };
    ctx.body_owner_id = Some(owner);
    ctx.owner_file = Some(owner.file(db));
    ctx.defaults_owner = matches!(owner, BodyOwnerId::ParameterDefaults(_));
    if let BodyOwnerId::ParameterDefaults(function) = owner {
        // The defaults arena has no single root: each parameter's default
        // checks against that parameter's declared type (its expectation
        // at the call boundary, the rule MIR's callee-entry prologue
        // relies on).
        let defaults = baml_compiler2_ppir::function_parameter_defaults(db, function);
        if let Some(arena) = body.expr_body() {
            for (index, default) in defaults.params.iter().enumerate() {
                let Some(default) = default else { continue };
                match ctx.param_tys.get(index).cloned() {
                    Some(param_ty) if !param_ty.has_error() => {
                        ctx.check_expr(arena, default.expr.expr(), &param_ty);
                    }
                    _ => {
                        ctx.infer_expr(arena, default.expr.expr(), &Expectation::None);
                    }
                }
            }
            ctx.backfill_untyped_patterns(arena);
        }
    } else if let Some(expr_body) = body.expr_body() {
        ctx.infer_expr_body(expr_body);
    }
    ctx.finish()
}

/// Which bounded-var classes a finish-fixpoint round may commit: the
/// eager tier solves fully-ground classes only; the ground-subset tier
/// is the quiescence-only fallback (see `finish`). Demand points
/// (`structurally_resolve`) bypass the tiers - a structure demand
/// commits from whatever has accumulated, rustc's semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SolveTier {
    Ground,
    GroundSubset,
}

/// Whether execution can proceed past the current point. `Maybe & Maybe`
/// branch-combines to `Maybe`; a `return`/`throw` sets `Always`, and a block
/// whose statements always diverge types as `never`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Diverges {
    Maybe,
    Always,
}

impl Diverges {
    /// Sequential combination: diverged stays diverged.
    fn or(self, other: Diverges) -> Diverges {
        if self == Diverges::Always || other == Diverges::Always {
            Diverges::Always
        } else {
            Diverges::Maybe
        }
    }

    /// Branch combination: all branches must diverge.
    fn and(self, other: Diverges) -> Diverges {
        if self == Diverges::Always && other == Diverges::Always {
            Diverges::Always
        } else {
            Diverges::Maybe
        }
    }
}

/// Contextual type information flowing DOWN the walk - the "check" half of
/// bidirectional inference. Not a bare `Option<Ty>`: the methods encode when
/// context may CONSTRAIN (emit `Sub`) versus merely INFORM (shape container
/// literals, branch pass-through).
#[derive(Debug, Clone)]
enum Expectation {
    None,
    HasType(Ty),
}

impl Expectation {
    /// The `Error` sentinel is never propagated as context. Top-level only
    /// (rust-analyzer's `Expectation::has_type` discipline): a nested
    /// sentinel - e.g. the `throws Error` placeholder inside a function
    /// type until S12 - must not discard the useful structure around it.
    fn has_type(ty: Ty) -> Expectation {
        if matches!(ty.kind(), TyKind::Error { .. }) {
            Expectation::None
        } else {
            Expectation::HasType(ty)
        }
    }

    fn only_has_type(&self) -> Option<&Ty> {
        match self {
            Expectation::HasType(ty) => Some(ty),
            Expectation::None => None,
        }
    }

    /// For if/match arms: drop the expectation when it resolves to a bare
    /// unsolved variable, so the first arm cannot over-constrain the rest -
    /// the arms JOIN at the merge point instead.
    fn adjust_for_branches(&self, table: &mut InferenceTable) -> Expectation {
        match self {
            Expectation::HasType(ty) => {
                let resolved = table.shallow_resolve(ty);
                if matches!(resolved.kind(), TyKind::Infer { .. }) {
                    Expectation::None
                } else {
                    Expectation::HasType(resolved)
                }
            }
            Expectation::None => Expectation::None,
        }
    }
}

/// One inference run over one body owner: the table, the accumulating
/// result, and the bidirectional expression walk.
struct InferenceContext<'db> {
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: Facts<'db>,
    index: &'db FileSemanticIndex<'db>,
    /// The owner body's scope: the key half mapping this body's `ExprId`s
    /// into the semantic index's per-file tables, and the guard that keeps
    /// parameter lookups from crossing into lambda scopes.
    owner_scope: Option<FileScopeId>,
    /// The metadata scope expression lookups key under RIGHT NOW. Equal to
    /// `owner_scope` except while walking a lambda body: the semantic index
    /// keys a lambda body's expressions under the LAMBDA's scope even though
    /// they share the owner's arena (`builder.rs::walk_lambda_expr`), so the
    /// walk must swap this when it descends into one.
    current_scope: Option<FileScopeId>,
    /// Expression spans, for locating the `ScopeKind::Lambda` scope that a
    /// `Expr::Lambda` node opened (scopes are keyed by source range).
    /// Parameter types for each lambda scope this run has walked, deduced by
    /// `infer_lambda`; the lambda-scope analog of `param_tys`.
    lambda_params: FxHashMap<FileScopeId, Vec<Ty>>,
    /// The flow-narrowing overlay: refined types for bindings, consulted
    /// before `type_of_pat`. S10a populates it per match arm only;
    /// the S10b condition/branch machinery grows it into the full
    /// environment (design: eager-forward on the structured walk).
    flow: FxHashMap<BindingId, Ty>,
    /// Lowering for body-position type annotations, carrying the owner's
    /// generic frame.
    lower: LowerCtx<'db>,
    /// The owner's parameter types, from its lowered signature, indexed by
    /// declaration position.
    param_tys: Vec<Ty>,
    /// Every type annotation written in this body, pre-lowered to span-free
    /// `TypeRef`s (the rust-analyzer bodies-own-their-type-refs shape).
    type_refs: Arc<BodyTypeRefs>,
    /// The owner's declared return type, the body root's expectation.
    return_ty: Option<Ty>,
    /// The owner's DECLARED throws clause's NAMED part, when written: the
    /// contract every throw site and callee effect is checked against.
    /// `None` means the effect is inferred instead (from the channel
    /// below).
    declared_throws: Option<Ty>,
    /// Whether the declared clause carried an open slot (`throws T | _`,
    /// spec rule 3): the contract check is suspended and the final effect
    /// is the named part unioned with the inferred set.
    declared_throws_open: bool,
    /// The effect-channel stack: contributions from `throw` sites and
    /// callee throws accumulate into the top. The bottom entry is the
    /// owner's channel; lambdas and `catch` bases push their own.
    throws_channels: Vec<Vec<(ExprId, Ty)>>,
    /// S17 pending diagnostics: anchored on arena ids, interned payloads;
    /// finalized into `InferenceResult::diagnostics` (plain types) at
    /// finish - r-a's `InferenceDiagnostic` discipline.
    pending_diags: Vec<PendingDiag<'db>>,
    /// Every `_` hole instantiated as a fresh table variable, with the
    /// site it was written at. A hole whose class is still unsolved at
    /// finalize reports E0147 `CannotInferType` there (rustc's E0282
    /// discipline) instead of leaking a bare `Infer` into lowering.
    hole_vars: Vec<(baml_type::interned::InferVar, HoleAnchor)>,
    /// One lowering per written body annotation (rust-analyzer's
    /// discipline): the let rule, the pattern walk, and the backfill all
    /// read the SAME lowered type - and so the same instantiated hole
    /// vars - for one `TypeRefId`. Without this, a `_` hole instantiates
    /// once per consumer and only the demand-connected copy solves.
    annotation_cache: FxHashMap<baml_compiler2_hir::type_ref::TypeRefId, Ty>,
    /// Member-lookup PROBE depth (TIR's `suppress_member_lookup_errors`
    /// discipline): a failed lookup reports only when no fallback tier
    /// remains - probes increment, the committed frame reports.
    member_probe_depth: u32,
    /// Depth of pattern lowering where the dead-pattern overlap check
    /// probes SILENTLY: or-pattern alternatives (one alt that can't
    /// match is fine - rustc's rule - only the whole `|` chain failing
    /// to overlap reports, from the chain's own frame) and `is` tests
    /// (a disjoint runtime type test is legal and answers `false`).
    pub(crate) or_probe_depth: u32,
    /// Nonzero while lowering the subtree of an already-REJECTED rest
    /// sub-pattern. The subtree still lowers (bindings must record, no
    /// unresolved-name cascades) but further rest-shape reports and
    /// dead-pattern mismatches inside it are noise after the one
    /// rejection at the outermost structural link.
    pub(crate) rest_reject_depth: u32,
    /// Name-visible parameters of enclosing tagged-template bodies (the
    /// tag's `body` lambda params, e.g. `prompt`'s `role`/`ctx`). A stack
    /// frame per nested template; the semantic index cannot register these
    /// as bindings (the tag is a cross-file item, resolving it during the
    /// index walk would cycle), so path resolution consults this instead -
    /// TIR's `template_body_params`, name-keyed.
    template_params: Vec<FxHashMap<baml_type::Name, Ty>>,
    table: InferenceTable,
    /// The irreducible `Sub` residue: pairs that were neither ground nor
    /// var-headed nor decomposable when emitted; re-examined at finish once
    /// resolution has run.
    deferred_subs: Vec<(Ty, Ty, Option<ExprId>)>,
    /// The obligation worklist (I4): registered during the walk,
    /// discharged at finish interleaved with bound resolution.
    obligations: Vec<obligations::Obligation>,
    /// The expression whose CHECK is currently relating types - r-a's
    /// `ObligationCause`: obligations born inside a structural `sub`
    /// recursion anchor their eventual diagnostic here.
    obligation_anchor: Option<ExprId>,
    /// The function whose body this run infers, when the owner IS a
    /// function - the resolver for owner-scoped receivers (`default`
    /// inside an `implements` block, like `self`).
    body_owner: Option<baml_compiler2_hir::loc::FunctionLoc<'db>>,
    /// The body owner this run walks, for the rare queries that need the
    /// owner's `AstSourceMap` (property-shorthand identification). Set for
    /// every owner kind, unlike `body_owner`.
    body_owner_id: Option<BodyOwnerId<'db>>,
    /// The value expressions AST lowering marked as property shorthand
    /// (`{ key }`, never `{ "key": key }`). Materialized on demand - the
    /// source map is a clone, and only the shorthand DIAGNOSTIC path
    /// consults it.
    shorthand_exprs: std::cell::OnceCell<rustc_hash::FxHashSet<ExprId>>,
    /// Whether this run infers a parameter-default arena: the semantic
    /// index keys those expressions under
    /// `ExprMetadataScope::ParameterDefault` (the builder's
    /// `walk_parameter_defaults`), so metadata lookups at the OWNER scope
    /// switch variant. Lambda descents swap `current_scope` and key
    /// `Body` as everywhere else.
    defaults_owner: bool,
    /// One frame per enclosing `OptionalChain` (chains nest through
    /// arguments): a `?.` link whose base was nullable sets the top
    /// frame, and the chain BOUNDARY unions `null` back into its
    /// result - TS short-circuit semantics, where intermediate links
    /// see the non-null type.
    chain_nullable: Vec<bool>,
    /// Value exprs whose unresolved-name diagnostic was superseded by a
    /// more specific one (object property shorthand).
    suppressed_unresolved: rustc_hash::FxHashSet<ExprId>,
    /// BEP-042: loop depth at each active `defer` entry - a
    /// `break`/`continue` at the SAME depth (or any `return`) would
    /// escape the defer body and is rejected.
    defer_loop_floors: Vec<usize>,
    loop_depth: usize,
    /// The body's root expression, kept for finish-time anchors (the
    /// extraneous-throws warning has no arena node of its own).
    body_root: Option<ExprId>,
    /// Ground values that checked against then-open expectations by
    /// depositing bounds; re-judged once the vars solve.
    provisional_checks: Vec<(ExprId, Ty, Ty)>,
    diverges: Diverges,
    /// The body's file, for package-scoped lookups (the overlap oracle's
    /// alias map enumerates the owning package plus its dependency closure).
    owner_file: Option<baml_base::SourceFile>,
    /// The pattern-reachability oracle's pre-folded alias map, built once
    /// The enclosing scope's PLAIN bound env for the written-type
    /// well-formedness judgment on body annotations, built lazily like
    /// the overlap aliases.
    wf_scope_env:
        std::cell::OnceCell<rustc_hash::FxHashMap<baml_type::ParamTy, Vec<baml_type::Interface>>>,
    /// per inference on first use (TIR's `normalized_overlap_aliases`).
    overlap_aliases:
        std::cell::OnceCell<std::collections::HashMap<baml_type::QualifiedTypeName, baml_type::Ty>>,
    result: InferenceResult<'db>,
}

impl<'db> InferenceContext<'db> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        db: &'db dyn baml_compiler2_ppir::Db,
        index: &'db FileSemanticIndex<'db>,
        owner_scope: Option<FileScopeId>,
        lower: LowerCtx<'db>,
        param_tys: Vec<Ty>,
        return_ty: Option<Ty>,
        type_refs: Arc<BodyTypeRefs>,
        bounds: FxHashMap<baml_type::ParamTy, Vec<baml_type::Interface>>,
    ) -> InferenceContext<'db> {
        InferenceContext {
            db,
            facts: Facts::with_bounds(db, bounds),
            index,
            owner_scope,
            current_scope: owner_scope,
            lambda_params: FxHashMap::default(),
            flow: FxHashMap::default(),
            lower,
            param_tys,
            type_refs,
            return_ty,
            declared_throws: None,
            declared_throws_open: false,
            throws_channels: vec![Vec::new()],
            pending_diags: Vec::new(),
            hole_vars: Vec::new(),
            annotation_cache: FxHashMap::default(),
            member_probe_depth: 0,
            or_probe_depth: 0,
            rest_reject_depth: 0,
            template_params: Vec::new(),
            table: InferenceTable::new(),
            deferred_subs: Vec::new(),
            obligations: Vec::new(),
            obligation_anchor: None,
            body_owner: None,
            body_owner_id: None,
            shorthand_exprs: std::cell::OnceCell::new(),
            defaults_owner: false,
            chain_nullable: Vec::new(),
            suppressed_unresolved: rustc_hash::FxHashSet::default(),
            defer_loop_floors: Vec::new(),
            loop_depth: 0,
            body_root: None,
            provisional_checks: Vec::new(),
            diverges: Diverges::Maybe,
            owner_file: None,
            overlap_aliases: std::cell::OnceCell::new(),
            wf_scope_env: std::cell::OnceCell::new(),
            result: InferenceResult::default(),
        }
    }

    /// The semantic-index key for an expression under the CURRENT scope:
    /// `ParameterDefault` when this run infers a default arena and the
    /// walk sits at the owner scope, `Body` everywhere else (lambda
    /// bodies included - their descent swaps `current_scope`).
    fn metadata_key(&self, expr: ExprId) -> Option<ExprMetadataKey> {
        let scope = self.current_scope?;
        let metadata_scope = if self.defaults_owner && Some(scope) == self.owner_scope {
            ExprMetadataScope::ParameterDefault(scope)
        } else {
            ExprMetadataScope::Body(scope)
        };
        Some(ExprMetadataKey::new(metadata_scope, expr))
    }

    fn infer_expr_body(&mut self, body: &ExprBody) {
        self.body_root = body.root_expr;
        if let Some(root) = body.root_expr {
            match self.return_ty.clone() {
                // A void function DISCARDS its body's tail value (TIR's
                // statement semantics; `defer { .. }; log.push(..)` as
                // the last line of a `-> void` fn is fine) - the body
                // still walks fully, it just isn't checked against unit.
                Some(return_ty) if is_unit(&return_ty) => {
                    self.infer_expr(body, root, &Expectation::None);
                }
                Some(return_ty) if !return_ty.has_error() => {
                    self.check_expr(body, root, &return_ty);
                }
                _ => {
                    self.infer_expr(body, root, &Expectation::None);
                }
            }
        }
        self.backfill_untyped_patterns(body);
    }

    /// Backfills patterns the walk did not type. A TYPE-EXPRESSION node
    /// (an annotation or ascription, or an OR of them) records the type
    /// it DENOTES - typed coverage, TIR's convention (A2); everything
    /// else records the sentinel so gaps stay visible. Shared by the
    /// body drive and the parameter-defaults drive.
    fn backfill_untyped_patterns(&mut self, body: &ExprBody) {
        let unfilled: Vec<PatId> = body
            .patterns
            .iter()
            .map(|(pat_id, _)| pat_id)
            .filter(|pat_id| !self.result.type_of_pat.contains_key(pat_id))
            .collect();
        for pat_id in unfilled {
            let ty = self
                .pattern_ascription_ty(body, pat_id)
                .unwrap_or_else(Ty::error);
            self.result.type_of_pat.insert(pat_id, ty);
        }
    }

    /// Checking mode: infer with the expectation, then constrain -
    /// `Sub(actual, expected)`, discharged eagerly. Definite failures are
    /// recorded against the checked expression, never dropped.
    fn check_expr(&mut self, body: &ExprBody, expr: ExprId, expected: &Ty) -> Ty {
        let ty = self.infer_expr(body, expr, &Expectation::has_type(expected.clone()));
        let saved_anchor = self.obligation_anchor.replace(expr);
        let fits = self.sub(&ty, expected);
        self.obligation_anchor = saved_anchor;
        if !fits {
            self.result
                .type_mismatches
                .insert(expr, (expected.clone(), ty.clone()));
        } else {
            // A GROUND value checked against a still-open expectation
            // passed by DEPOSITING a bound; whether it actually fits is
            // only known once the var solves. Stash for the finalize
            // re-check (the establishment-order road: a later push's
            // incompatible element reports here).
            if (expected.has_infer() || ty.has_infer()) && !ty.has_error() {
                self.provisional_checks
                    .push((expr, expected.clone(), ty.clone()));
            }
            self.record_function_adapter(expr, &ty, expected);
        }
        ty
    }

    /// rustc/r-a record coercions as per-expression adjustments consumed
    /// structurally at MIR lowering; every checked position funnels
    /// through `check_expr`, so this one probe covers TIR's five
    /// recording sites. Fires only on an ACCEPTED check whose value and
    /// expectation are both function-shaped but runtime-incompatible
    /// (TIR's `function_coercion_for`): lowering must synthesize an
    /// adapter closure. Read-only on inference state (no var forcing -
    /// alias expansion only), so recording cannot perturb typing.
    fn record_function_adapter(&mut self, expr: ExprId, got: &Ty, expected: &Ty) {
        let got = self.table.resolve_completely(got);
        let got = self.expand_alias_ty(&got);
        let TyKind::Function { params: source, .. } = got.kind() else {
            return;
        };
        let target_fn = self.table.resolve_completely(expected);
        let target_fn = self.expand_alias_ty(&target_fn);
        let TyKind::Function { params: target, .. } = target_fn.kind() else {
            return;
        };
        if function_params_runtime_compatible(source, target) {
            return;
        }
        self.result.expr_adjustments.insert(
            expr,
            Box::new([Adjustment {
                kind: Adjust::FunctionAdapter,
                target: target_fn.clone(),
            }]),
        );
    }

    fn infer_expr(&mut self, body: &ExprBody, expr: ExprId, expected: &Expectation) -> Ty {
        let ty = match &body.exprs[expr] {
            Expr::Literal(lit) => {
                // An `int` literal whose value doesn't fit i63 (a valid i64
                // like 2^62, but out of `int` range) would otherwise reach
                // the VM and panic at engine load. Reject it here (E0150,
                // pointing at `bigint`) and substitute an in-range
                // placeholder so a failed compile can't carry the bad value
                // forward - TIR's rule verbatim.
                let lit = if let Literal::Int(v) = lit
                    && !(INT_MIN..=INT_MAX).contains(v)
                {
                    self.pending_diags
                        .push(PendingDiag::IntLiteralOutOfRange { expr, value: *v });
                    &Literal::Int(0)
                } else {
                    lit
                };
                Ty::intern(TyKind::Literal(
                    lit.clone(),
                    Freshness::Fresh,
                    TyAttr::default(),
                ))
            }
            Expr::Null => Ty::null(),
            // A byte-string literal (`b"..."`) IS a `uint8array` value -
            // its own expr kind, not a `Literal` (no literal TYPE per
            // byte-string; TIR agrees).
            Expr::ByteStringLiteral(_) => Ty::intern(TyKind::Uint8Array {
                attr: TyAttr::default(),
            }),
            Expr::Path(segments) => self.resolve_value_path(expr, segments),
            Expr::Index { base, index } => self.infer_index(body, expr, *base, *index, false),
            Expr::Spawn {
                name,
                with_exprs,
                body: spawn_body,
            } => {
                let (name, with_exprs, spawn_body) = (*name, with_exprs.clone(), *spawn_body);
                self.infer_spawn(body, name, &with_exprs, spawn_body)
            }
            Expr::Await { future } => self.infer_await(body, expr, *future),
            Expr::OptionalIndex { base, index } => {
                self.infer_index(body, expr, *base, *index, true)
            }
            Expr::Block { stmts, tail_expr } => {
                let entry_diverges = self.diverges;
                let mut first_unreachable: Option<usize> = None;
                for (index, stmt) in stmts.iter().enumerate() {
                    // Dead code counts only after a syntactic TERMINATOR
                    // statement (return/throw/break/continue) - a
                    // never-typed call is divergence the checker knows,
                    // not noise the user wrote past.
                    if first_unreachable.is_none()
                        && entry_diverges == Diverges::Maybe
                        && self.diverges == Diverges::Always
                        && index > 0
                        && (matches!(
                            body.stmts[stmts[index - 1]],
                            Stmt::Return { .. } | Stmt::Throw { .. } | Stmt::Break | Stmt::Continue
                        ) || matches!(
                            &body.stmts[stmts[index - 1]],
                            Stmt::Let {
                                initializer: Some(init),
                                ..
                            } if matches!(body.exprs[*init], Expr::Throw { .. })
                        ))
                    {
                        first_unreachable = Some(index);
                    }
                    self.infer_stmt(body, *stmt);
                }
                // The block TAIL counts too: `throw "x"` followed by a
                // tail `0` leaves the tail unreachable even with no
                // trailing statements.
                let tail_after_diverge = first_unreachable.is_none()
                    && tail_expr.is_some()
                    && !stmts.is_empty()
                    && matches!(
                        &body.stmts[*stmts.last().expect("nonempty")],
                        Stmt::Return { .. } | Stmt::Throw { .. } | Stmt::Break | Stmt::Continue
                    );
                if let Some(index) = first_unreachable {
                    self.pending_diags.push(PendingDiag::DeadCode {
                        at: stmts[index],
                        unreachable_count: stmts.len() - index + usize::from(tail_expr.is_some()),
                    });
                } else if tail_after_diverge {
                    self.pending_diags.push(PendingDiag::DeadCode {
                        at: *stmts.last().expect("nonempty"),
                        unreachable_count: 1,
                    });
                }
                match tail_expr {
                    Some(tail) => self.infer_expr(body, *tail, expected),
                    // A tail-less block that always diverged is never;
                    // otherwise it is void.
                    None if self.diverges == Diverges::Always
                        && entry_diverges == Diverges::Maybe =>
                    {
                        Ty::never()
                    }
                    None => Ty::void(),
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.check_expr(body, *condition, &Ty::bool());
                let facts = self.condition_facts(body, *condition);
                let condition_diverges = self.diverges;
                let branch_expectation = expected.adjust_for_branches(&mut self.table);
                let base_flow = self.flow.clone();

                self.apply_facts(&facts.when_true);
                self.diverges = Diverges::Maybe;
                let then_ty = self.infer_expr(body, *then_branch, &branch_expectation);
                let then_diverges = self.diverges;
                let then_flow = std::mem::replace(&mut self.flow, base_flow.clone());
                let then_flow = (then_diverges == Diverges::Maybe).then_some(then_flow);

                // The else path (written or implicit fall-through) carries
                // the condition's false facts; the divergence-aware merge
                // makes guard-with-early-return narrowing the ordinary
                // rule (B-688), no special case.
                self.apply_facts(&facts.when_false);
                match else_branch {
                    Some(else_expr) => {
                        self.diverges = Diverges::Maybe;
                        let else_ty = self.infer_expr(body, *else_expr, &branch_expectation);
                        let else_diverges = self.diverges;
                        let else_flow = std::mem::replace(&mut self.flow, base_flow.clone());
                        let else_flow = (else_diverges == Diverges::Maybe).then_some(else_flow);
                        self.diverges = condition_diverges.or(then_diverges.and(else_diverges));
                        self.merge_branch_flows(base_flow, then_flow, else_flow);
                        // The merge point: a canonical union, never a forced
                        // equality (joins happen at generation sites).
                        self.join(&[then_ty, else_ty])
                    }
                    None => {
                        let else_flow = std::mem::replace(&mut self.flow, base_flow.clone());
                        self.diverges = condition_diverges;
                        self.merge_branch_flows(base_flow, then_flow, Some(else_flow));
                        // No else: the if produces no value.
                        Ty::void()
                    }
                }
            }
            Expr::IfLet {
                pattern,
                scrutinee,
                then_branch,
                else_branch,
            } => {
                // `if let PAT = scrut`: the pattern types its bindings
                // (the S10 walk), the then-branch sees the scrutinee
                // narrowed to the matched type, the else-branch the
                // RESIDUAL (consumes-gated, B-1069's rule); branch join
                // and the divergence-aware merge follow the If arm.
                let scrut_ty = self.infer_expr(body, *scrutinee, &Expectation::None);
                let scrut = self.scrutinee_demand(&scrut_ty);
                let outcome = self.lower_pattern(body, *pattern, &scrut);
                let condition_diverges = self.diverges;
                let branch_expectation = expected.adjust_for_branches(&mut self.table);
                let base_flow = self.flow.clone();

                if let Some(binding) = self.narrowable_binding(body, *scrutinee) {
                    self.flow.insert(binding, outcome.matched_ty.clone());
                }
                self.diverges = Diverges::Maybe;
                let then_ty = self.infer_expr(body, *then_branch, &branch_expectation);
                let then_diverges = self.diverges;
                let then_flow = std::mem::replace(&mut self.flow, base_flow.clone());
                let then_flow = (then_diverges == Diverges::Maybe).then_some(then_flow);

                if outcome.consumes_matched
                    && let Some(binding) = self.narrowable_binding(body, *scrutinee)
                {
                    let residual = self.subtract_narrow(&scrut, &outcome.matched_ty);
                    self.flow.insert(binding, residual);
                }
                match else_branch {
                    Some(else_expr) => {
                        self.diverges = Diverges::Maybe;
                        let else_ty = self.infer_expr(body, *else_expr, &branch_expectation);
                        let else_diverges = self.diverges;
                        let else_flow = std::mem::replace(&mut self.flow, base_flow.clone());
                        let else_flow = (else_diverges == Diverges::Maybe).then_some(else_flow);
                        self.diverges = condition_diverges.or(then_diverges.and(else_diverges));
                        self.merge_branch_flows(base_flow, then_flow, else_flow);
                        self.join(&[then_ty, else_ty])
                    }
                    None => {
                        let else_flow = std::mem::replace(&mut self.flow, base_flow.clone());
                        self.diverges = condition_diverges;
                        self.merge_branch_flows(base_flow, then_flow, Some(else_flow));
                        Ty::void()
                    }
                }
            }
            Expr::Upcast { base, .. } => {
                // Rust's qualified trait path (`<X as Parent<i32>>`,
                // r-a's trait-object coercion): the value viewed through
                // a WRITTEN interface. The base proves the view - ground
                // pairs through the implements oracle, var-carrying
                // targets as an Implements obligation anchored HERE -
                // and the result IS the target, so members flow through
                // the existential road with exactly the requested
                // args/pins (what disambiguates same-name members across
                // instantiations). Interface views only (TIR's rule);
                // the non-interface diagnostic is S17's.
                let base_ty = self.infer_expr(body, *base, &Expectation::None);
                let target = self
                    .type_refs
                    .upcast_targets
                    .get(&expr)
                    .copied()
                    .map(|target_ref| {
                        let lowered = self.lower.lower_type_ref(&self.type_refs.store, target_ref);
                        self.reject_expr_position_holes(&lowered, expr)
                    })
                    .unwrap_or_else(Ty::error);
                // The interface-view gate is a STRUCTURE demand: an
                // alias naming an interface answers as the interface.
                let target = self.expand_alias_ty(&target);
                if target.has_error() {
                    Ty::error()
                } else if !matches!(target.kind(), TyKind::Interface(..)) {
                    self.pending_diags
                        .push(PendingDiag::UpcastTargetNotInterface {
                            expr,
                            target: target.clone(),
                        });
                    Ty::error()
                } else {
                    let saved_anchor = self.obligation_anchor.replace(expr);
                    let fits = self.sub(&base_ty, &target);
                    self.obligation_anchor = saved_anchor;
                    if !fits {
                        // BEP-044's dedicated form: the value's type does
                        // not implement the requested interface - clearer
                        // than the generic mismatch.
                        self.pending_diags.push(PendingDiag::UpcastNotImplemented {
                            expr,
                            value: base_ty,
                            interface: target.clone(),
                        });
                    }
                    target
                }
            }
            Expr::GenericApply { base, .. } => {
                // Value-position turbofish (rustc's `let f =
                // identity::<i32>`; r-a's `substs_from_path`): the SAME
                // resolution ladder a call's callee takes, reading the
                // written type args keyed at THIS expression (the
                // `instantiation_args` channel), minus the call. A
                // resolved METHOD binds its receiver like any
                // instance-accessed method value.
                let (fn_ty, bound) = self.infer_callee(body, expr, *base);
                if bound { bind_receiver(fn_ty) } else { fn_ty }
            }
            Expr::Template { tag, segments } => match tag {
                baml_compiler2_ast::TemplateTag::Default { elaborated } => {
                    // Untagged backtick (BEP-049 §11): the value IS the
                    // desugared `string.from`-wrapped `+` concat, which
                    // types every `${expr}` in place on its original span.
                    let segments = segments.clone();
                    let result = self.infer_expr(body, *elaborated, &Expectation::None);
                    // §11 strict stringify: a NULLABLE interpolated value
                    // errors at check time (the structured segments exist
                    // exactly for these per-`${...}` diagnostics on the
                    // original spans).
                    self.check_template_interps_strict(&segments);
                    result
                }
                baml_compiler2_ast::TemplateTag::Custom { tag, body: flatten } => {
                    // Tagged template (BEP-049 §10): the result is the TAG
                    // fn's return type (`prompt` yields the `(Context) ->
                    // PromptAst` render closure). The desugared flatten
                    // block types under the template's synthetic Lambda
                    // scope with the tag's body-lambda params name-visible;
                    // its effects and divergence stay the deferred
                    // closure's, not the enclosing function's - the
                    // `infer_lambda` discipline.
                    let tag_ty = self.infer_expr(body, *tag, &Expectation::None);
                    let resolved = self.table.resolve_completely(&tag_ty);
                    let tag_name = match &body.exprs[*tag] {
                        Expr::Path(segments) => segments.last().cloned(),
                        _ => None,
                    }
                    .unwrap_or_else(|| baml_type::Name::new("<tag>"));
                    // BEP-049 SS10 tag validation (TIR's rules): the tag must
                    // be a `//baml:tagged_string`-marked function whose first
                    // parameter is `body: (...) -> baml.TaggedString`. The
                    // body-lambda params scope into the interpolations only
                    // when the tag validated; an unresolved tag already
                    // reported UnresolvedName and stays quiet here.
                    let (result, frame) = match resolved.kind() {
                        TyKind::Function { params, ret, .. } => {
                            let func = match self.result.member_resolutions.get(tag) {
                                Some(MemberResolution::Free { func }) => Some(*func),
                                _ => None,
                            };
                            let is_tagged = func.is_some_and(|func| {
                                baml_compiler2_ppir::item_data::function_data(self.db, func)
                                    .is_tagged_template_tag
                            });
                            let body_param_ok = params.first().is_some_and(|param| {
                                param.name.as_ref().is_some_and(|n| n.as_str() == "body")
                                    && matches!(
                                        param.ty.kind(),
                                        TyKind::Function { ret: body_ret, .. }
                                            if matches!(
                                                body_ret.kind(),
                                                TyKind::Class(qtn, _, _)
                                                    if qtn.is_builtin_root_type("TaggedString")
                                            )
                                    )
                            });
                            if !is_tagged {
                                self.pending_diags.push(PendingDiag::TaggedTagInvalid {
                                    at: *tag,
                                    name: tag_name,
                                    func,
                                    kind: TaggedTagIssue::NotMarked,
                                });
                                (Ty::error(), FxHashMap::default())
                            } else if !body_param_ok {
                                self.pending_diags.push(PendingDiag::TaggedTagInvalid {
                                    at: *tag,
                                    name: tag_name,
                                    func,
                                    kind: TaggedTagIssue::BadBodyParam,
                                });
                                (Ty::error(), FxHashMap::default())
                            } else {
                                let mut frame = FxHashMap::default();
                                if let Some(first) = params.first()
                                    && let TyKind::Function {
                                        params: body_params,
                                        ..
                                    } = first.ty.kind()
                                {
                                    for param in body_params {
                                        if let Some(name) = &param.name {
                                            frame.insert(name.clone(), param.ty.clone());
                                        }
                                    }
                                }
                                (ret.clone(), frame)
                            }
                        }
                        // Unresolved: `UnresolvedName` already reported for a
                        // bare-path tag - no double report.
                        _ if resolved.has_error() => (Ty::error(), FxHashMap::default()),
                        _ => {
                            self.pending_diags.push(PendingDiag::TaggedTagInvalid {
                                at: *tag,
                                name: tag_name,
                                func: None,
                                kind: TaggedTagIssue::NotAFunction,
                            });
                            (Ty::error(), FxHashMap::default())
                        }
                    };
                    // NO scope switch: the builder marks the template's
                    // Lambda scope `is_template_body` - a capture boundary
                    // for MIR, but its expressions register in the
                    // ENCLOSING metadata namespace (TIR's
                    // `inference_owner_scope` climbs past it the same way).
                    // Entering it here would key every lookup into a
                    // namespace nothing was registered under.
                    self.template_params.push(frame);
                    self.throws_channels.push(Vec::new());
                    let saved_diverges = std::mem::replace(&mut self.diverges, Diverges::Maybe);
                    self.infer_expr(body, *flatten, &Expectation::None);
                    self.diverges = saved_diverges;
                    // The tag's `body` param declares the flatten block's
                    // effect contract (`throws never` for `prompt`); the
                    // contract diagnostic is S17's, so the channel drops.
                    self.throws_channels.pop();
                    self.template_params.pop();
                    result
                }
            },
            Expr::Array { elements } => {
                // With an expected element type, elements are CHECKED against
                // it; otherwise they synthesize and JOIN (fresh literals
                // widening at the join, per ruling 1's generation-site rule).
                let expected_element = self.expected_list_element(expected);
                match expected_element {
                    Some(element_ty) => {
                        for element in elements {
                            self.check_expr(body, *element, &element_ty);
                        }
                        Ty::list(element_ty)
                    }
                    None if elements.is_empty() => {
                        // `[]`: a list over a fresh element variable - the
                        // honest replacement for the EvolvingList sentinel.
                        Ty::list(self.table.new_establishment_var_ty())
                    }
                    None => {
                        let joined: Vec<Ty> = elements
                            .iter()
                            .map(|element| {
                                let ty = self.infer_expr(body, *element, &Expectation::None);
                                self.widen_fresh(&ty)
                            })
                            .collect();
                        // TS's best-common-type adoption (rustc likewise
                        // unifies vec elements; E0282 needs NO evidence):
                        // an element still carrying inference vars takes
                        // the join of its GROUND siblings as an upper
                        // bound, so `[[1], []]` is `int[][]` - the empty
                        // list's element solves from the siblings instead
                        // of erasing under the rustc-strict rule, which
                        // remains for genuinely unconstrained literals
                        // (`[[], []]` has no evidence and stays strict).
                        let (ground, open): (Vec<Ty>, Vec<Ty>) = joined
                            .iter()
                            .cloned()
                            .partition(|ty| !self.table.resolve_completely(ty).has_infer());
                        if !ground.is_empty() && !open.is_empty() {
                            let evidence = self.join(&ground);
                            for ty in &open {
                                self.sub(ty, &evidence);
                            }
                        }
                        Ty::list(self.union_of(&joined))
                    }
                }
            }
            Expr::Map { entries } => {
                // Property shorthand in an UNTYPED object (`{ options }`):
                // when the elided name resolves nowhere, the specialized
                // diagnostic (with in-scope near-matches) supersedes the
                // generic unresolved-name one.
                //
                // Two invariants this walk must not re-derive:
                //   * Shorthand-ness is the PARSER's fact, recorded in the
                //     source map. Key text equal to the value's name is a
                //     coincidence a written `{ "key": key }` shares, and
                //     that entry is not shorthand.
                //   * Scope is the semantic INDEX's fact. The value is an
                //     ordinary path expression, so it is in scope exactly
                //     when a plain use of it would be - through the same
                //     `resolve_value_path` tiers, which see every binding
                //     form (`if let`, match arms, `for`, `catch`,
                //     destructures) and every nesting depth.
                for (key, value) in entries {
                    let Expr::Literal(Literal::String(key_name)) = &body.exprs[*key] else {
                        continue;
                    };
                    let Expr::Path(segments) = &body.exprs[*value] else {
                        continue;
                    };
                    if segments.len() != 1 || segments[0].as_str() != key_name.as_str() {
                        continue;
                    }
                    if self.path_resolves_locally(*value)
                        || self.template_param_root(&segments[0])
                        || self.lower.resolve_value(segments).is_some()
                    {
                        continue;
                    }
                    if !self.is_property_shorthand(*value) {
                        // A written `{ "key": key }` with an unbound `key`:
                        // the generic unresolved-name diagnostic is the
                        // honest one - there is no shorthand to explain.
                        continue;
                    }
                    let locals = self.local_binding_names(*value);
                    let suggestions =
                        crate::diagnostics::similar_name_suggestions(&segments[0], locals.iter());
                    self.suppressed_unresolved.insert(*value);
                    self.pending_diags.push(PendingDiag::UnresolvedShorthand {
                        expr: *value,
                        name: segments[0].clone(),
                        suggestions,
                    });
                }
                // With an expected map type, entries are CHECKED against
                // its key/value (the Array arm's rule; r-a's
                // expectation-driven literal typing) - `{"input": s}` in
                // a `map<string, unknown>` position IS that map, not a
                // synthesized `map<string, string>` tripping invariance.
                let expected_entry = self.expected_map_entry(expected);
                if let Some((key_ty, value_ty)) = expected_entry {
                    for (key, value) in entries {
                        self.check_expr(body, *key, &key_ty);
                        self.check_expr(body, *value, &value_ty);
                    }
                    Ty::intern(TyKind::Map {
                        key: key_ty,
                        value: value_ty,
                        attr: TyAttr::default(),
                    })
                } else if entries.is_empty() {
                    // The key is `string` outright, not a fresh var: map
                    // keys are string-domain by language contract (see
                    // `checked_map_key`), so a key var has exactly one
                    // legal solution and leaving it open lets `m[0] = 1`
                    // silently solve `?K := int`.
                    Ty::intern(TyKind::Map {
                        key: Ty::string(),
                        value: self.table.new_establishment_var_ty(),
                        attr: TyAttr::default(),
                    })
                } else {
                    let (keys, values): (Vec<Ty>, Vec<Ty>) = entries
                        .iter()
                        .map(|(key, value)| {
                            let key_ty = self.infer_expr(body, *key, &Expectation::None);
                            let value_ty = self.infer_expr(body, *value, &Expectation::None);
                            (self.widen_fresh(&key_ty), self.widen_fresh(&value_ty))
                        })
                        .unzip();
                    Ty::intern(TyKind::Map {
                        key: self.union_of(&keys),
                        value: self.union_of(&values),
                        attr: TyAttr::default(),
                    })
                }
            }
            Expr::Return { value } => {
                if !self.defer_loop_floors.is_empty() {
                    self.pending_diags.push(PendingDiag::DeferEscape {
                        stmt: None,
                        expr: Some(expr),
                        keyword: "return",
                    });
                }
                if let Some(value) = value {
                    match self.return_ty.clone() {
                        Some(return_ty) if !return_ty.has_error() => {
                            self.check_expr(body, *value, &return_ty);
                        }
                        _ => {
                            self.infer_expr(body, *value, &Expectation::None);
                        }
                    }
                }
                self.diverges = Diverges::Always;
                Ty::never()
            }
            Expr::Throw { value } => {
                let thrown = self.infer_expr(body, *value, &Expectation::None);
                self.record_throw(*value, &thrown);
                self.diverges = Diverges::Always;
                Ty::never()
            }
            Expr::Binary { op, lhs, rhs } => self.infer_binary(body, expr, *op, *lhs, *rhs),
            Expr::Unary { op, expr: operand } => self.infer_unary(body, *op, *operand),
            Expr::Call { callee, args, .. } => self.infer_call(body, expr, *callee, args),
            Expr::Object {
                type_name,
                type_args,
                fields,
                spreads,
            } => {
                // `map { .. }` is a map literal in constructor clothing
                // (identifier keys are string keys), never a class named
                // `map` - same routing guard as the parser's object form.
                if type_args.is_empty()
                    && spreads.is_empty()
                    && matches!(type_name.0.as_slice(), [seg] if seg.as_str() == "map")
                {
                    if let Some((key_ty, value_ty)) = self.expected_map_entry(expected) {
                        for (_, value) in fields {
                            self.check_expr(body, *value, &value_ty);
                        }
                        Ty::intern(TyKind::Map {
                            key: key_ty,
                            value: value_ty,
                            attr: TyAttr::default(),
                        })
                    } else if fields.is_empty() {
                        Ty::intern(TyKind::Map {
                            key: Ty::string(),
                            value: self.table.new_establishment_var_ty(),
                            attr: TyAttr::default(),
                        })
                    } else {
                        let values: Vec<Ty> = fields
                            .iter()
                            .map(|(_, value)| {
                                let value_ty = self.infer_expr(body, *value, &Expectation::None);
                                self.widen_fresh(&value_ty)
                            })
                            .collect();
                        Ty::intern(TyKind::Map {
                            key: Ty::string(),
                            value: self.union_of(&values),
                            attr: TyAttr::default(),
                        })
                    }
                } else {
                    self.infer_object(body, expr, type_name, fields, spreads)
                }
            }
            Expr::MemberAccess { base, member } => {
                if self.check_runtime_id_member(body, expr, *base, member) {
                    return Ty::error();
                }
                let base_ty = self.infer_expr(body, *base, &Expectation::None);
                self.field_access(expr, &base_ty, member)
            }
            // TS short-circuit chains: the boundary owns the `| null`.
            Expr::OptionalChain { expr: inner } => {
                self.chain_nullable.push(false);
                let ty = self.infer_expr(body, *inner, &Expectation::None);
                let nullable = self.chain_nullable.pop().expect("pushed above");
                if nullable {
                    self.union_of(&[ty, Ty::null()])
                } else {
                    ty
                }
            }
            Expr::OptionalMemberAccess { base, member } => {
                if self.check_runtime_id_member(body, expr, *base, member) {
                    return Ty::error();
                }
                let base_ty = self.infer_expr(body, *base, &Expectation::None);
                self.check_needless_chain(body, expr, *base, &base_ty);
                let nonnull = self.peel_chain_null(&base_ty);
                self.field_access(expr, &nonnull, member)
            }
            Expr::OptionalCall { callee, args } => {
                let callee_ty = self.infer_expr(body, *callee, &Expectation::None);
                self.check_needless_chain(body, expr, *callee, &callee_ty);
                let nonnull = self.peel_chain_null(&callee_ty);
                let args = args.clone();
                self.check_call_args(body, expr, *callee, &nonnull, false, &args)
            }
            Expr::Lambda(def) => self.infer_lambda(body, expr, def, expected),
            Expr::Match {
                scrutinee, arms, ..
            } => {
                let arms = arms.clone();
                self.infer_match(body, expr, *scrutinee, &arms, expected)
            }
            Expr::Is { scrutinee, pattern } => self.infer_is(body, *scrutinee, *pattern),
            Expr::Catch { base, clauses } => {
                let clauses = clauses.clone();
                self.infer_catch(body, *base, &clauses, expected)
            }
            // The parse-recovery hole: no children in practice; the
            // generic visit stays as recovery if that ever changes.
            Expr::Missing => {
                let mut children = Vec::new();
                body.expr_children(expr, &mut children);
                for node in children {
                    match node {
                        BodyNode::Expr(child) => {
                            self.infer_expr(body, child, &Expectation::None);
                        }
                        BodyNode::Stmt(child) => self.infer_stmt(body, child),
                    }
                }
                Ty::error()
            }
        };
        self.result.type_of_expr.insert(expr, ty.clone());
        ty
    }

    fn infer_stmt(&mut self, body: &ExprBody, stmt: StmtId) {
        match &body.stmts[stmt] {
            Stmt::Expr(expr) => {
                self.infer_expr(body, *expr, &Expectation::None);
            }
            Stmt::Let {
                pattern,
                initializer,
                else_branch,
                ..
            } => {
                self.infer_let(body, *pattern, *initializer, *else_branch);
            }
            Stmt::Return(value) => {
                if !self.defer_loop_floors.is_empty() {
                    self.pending_diags.push(PendingDiag::DeferEscape {
                        stmt: Some(stmt),
                        expr: None,
                        keyword: "return",
                    });
                }
                if let Some(value) = value {
                    match self.return_ty.clone() {
                        Some(return_ty) if !return_ty.has_error() => {
                            self.check_expr(body, *value, &return_ty);
                        }
                        _ => {
                            self.infer_expr(body, *value, &Expectation::None);
                        }
                    }
                }
                self.diverges = Diverges::Always;
            }
            Stmt::Throw { value } => {
                let thrown = self.infer_expr(body, *value, &Expectation::None);
                self.record_throw(*value, &thrown);
                self.diverges = Diverges::Always;
            }
            // Loop-local terminators: the path past them is dead; the loop
            // discipline restores divergence and flow at the loop boundary.
            Stmt::Break | Stmt::Continue => {
                if self
                    .defer_loop_floors
                    .last()
                    .is_some_and(|&floor| self.loop_depth == floor)
                {
                    self.pending_diags.push(PendingDiag::DeferEscape {
                        stmt: Some(stmt),
                        expr: None,
                        keyword: if matches!(body.stmts[stmt], Stmt::Break) {
                            "break"
                        } else {
                            "continue"
                        },
                    });
                }
                self.diverges = Diverges::Always;
            }
            Stmt::Defer { body: defer_body } => {
                // BEP-042: the defer body runs at scope exit; escaping
                // control flow is rejected (loop-aware - a loop opened
                // inside the defer may break/continue freely).
                self.defer_loop_floors.push(self.loop_depth);
                self.infer_expr(body, *defer_body, &Expectation::None);
                self.defer_loop_floors.pop();
            }
            Stmt::Assign { target, value } => {
                self.infer_assign(body, *target, *value, None);
            }
            Stmt::AssignOp { target, op, value } => {
                self.infer_assign(body, *target, *value, Some(*op));
            }
            Stmt::While {
                condition,
                body: loop_body,
                after,
                ..
            } => {
                // Loop discipline (the no-fixpoint recipe): havoc the
                // bindings the body assigns, run the body once under the
                // condition's true facts, and the POST-loop environment is
                // loop entry plus the false facts - a zero-iteration loop
                // keeps no body narrowing (B-735).
                for binding in self.assigned_bindings(body, *loop_body) {
                    self.flow.remove(&binding);
                }
                self.check_expr(body, *condition, &Ty::bool());
                let facts = self.condition_facts(body, *condition);
                let entry_flow = self.flow.clone();
                self.apply_facts(&facts.when_true);
                let saved = self.diverges;
                self.loop_depth += 1;
                self.infer_expr(body, *loop_body, &Expectation::None);
                self.loop_depth -= 1;
                if let Some(after) = after {
                    self.infer_stmt(body, *after);
                }
                self.diverges = saved;
                self.flow = entry_flow;
                self.apply_facts(&facts.when_false);
            }
            Stmt::WhileLet {
                pattern,
                scrutinee,
                body: loop_body,
            } => {
                for binding in self.assigned_bindings(body, *loop_body) {
                    self.flow.remove(&binding);
                }
                let scrut_ty = self.infer_expr(body, *scrutinee, &Expectation::None);
                let scrut = self.scrutinee_demand(&scrut_ty);
                let outcome = self.lower_pattern(body, *pattern, &scrut);
                let entry_flow = self.flow.clone();
                if let Some(binding) = self.narrowable_binding(body, *scrutinee) {
                    self.flow.insert(binding, outcome.matched_ty);
                }
                let saved = self.diverges;
                self.loop_depth += 1;
                self.infer_expr(body, *loop_body, &Expectation::None);
                self.loop_depth -= 1;
                self.diverges = saved;
                self.flow = entry_flow;
            }
            Stmt::For {
                binding,
                collection,
                body: loop_body,
            } => {
                for havoced in self.assigned_bindings(body, *loop_body) {
                    self.flow.remove(&havoced);
                }
                let collection_ty = self.infer_expr(body, *collection, &Expectation::None);
                let resolved = self.table.resolve_completely(&collection_ty);
                let collection_ty = self.matrix_scrut(&resolved);
                // List elements directly; everything else through the
                // Iterator protocol (the `iter.Iterable` Item projection).
                let element = match collection_ty.kind() {
                    TyKind::List(element, _) => element.clone(),
                    _ => self.iteration_item(&collection_ty, *collection),
                };
                let outcome = self.lower_pattern(body, *binding, &element);
                if !outcome.covers_type
                    && !outcome.matched_ty.has_error()
                    && !element.has_error()
                    && !element.has_infer()
                {
                    self.pending_diags.push(PendingDiag::RefutableLet {
                        pat: *binding,
                        context: crate::diagnostics::IrrefutableContextKind::ForLet,
                    });
                }
                let entry_flow = self.flow.clone();
                let saved = self.diverges;
                self.loop_depth += 1;
                self.infer_expr(body, *loop_body, &Expectation::None);
                self.loop_depth -= 1;
                self.diverges = saved;
                self.flow = entry_flow;
            }
            _ => {
                let mut children = Vec::new();
                body.stmt_children(stmt, &mut children);
                for node in children {
                    match node {
                        BodyNode::Expr(child) => {
                            self.infer_expr(body, child, &Expectation::None);
                        }
                        BodyNode::Stmt(child) => self.infer_stmt(body, child),
                    }
                }
            }
        }
    }

    /// The `let` rule: with an annotation, the initializer is CHECKED
    /// against it (`_` holes as fresh vars, filled by the resulting bounds);
    /// without one, the initializer synthesizes and fresh literals widen at
    /// the binding site.
    fn infer_let(
        &mut self,
        body: &ExprBody,
        pattern: PatId,
        initializer: Option<ExprId>,
        else_branch: Option<ExprId>,
    ) {
        if else_branch.is_some() {
            // let-else is REFUTABLE (Rust's let-else): every pattern -
            // the ascribed-bind form (`let o: Ok = r else ..`) included
            // - is a pattern TEST against the synthesized initializer,
            // narrowing to the matched type; the else arm covers the
            // residual, so the initializer is never CHECKED against the
            // written type.
            self.infer_let_destructure(body, pattern, initializer, true);
            return self.finish_let_else(body, else_branch);
        }
        match &body.patterns[pattern] {
            Pattern::Bind { subpat, .. } => {
                // The `let` rule decides check-vs-synthesize from the
                // leading ascription alone; the pattern itself then
                // lowers through the ONE recursive walk match arms use
                // (r-a's `infer_top_pat` shape) - so chains
                // (`let x: let y`) and structural subpatterns
                // (`let xs: [..]: int[]`) record every binding.
                let subpat = *subpat;
                let annotation = subpat.and_then(|sub| self.pattern_ascription_ty(body, sub));
                let settled = match annotation {
                    Some(annotation_ty) => {
                        if let Some(init) = initializer {
                            self.check_expr(body, init, &annotation_ty);
                        }
                        annotation_ty
                    }
                    None => match initializer {
                        Some(init) => {
                            let init_ty = self.infer_expr(body, init, &Expectation::None);
                            // A VOID call result has no value to bind
                            // (void == unit interim, but the WRITTEN void
                            // contract says "no result").
                            let resolved = self.table.resolve_completely(&init_ty);
                            if matches!(resolved.kind(), TyKind::Void { .. }) {
                                self.pending_diags
                                    .push(PendingDiag::VoidResultUsed { expr: init });
                            }
                            self.widen_fresh(&init_ty)
                        }
                        None => Ty::error(),
                    },
                };
                self.lower_pattern(body, pattern, &settled);
                self.finish_let_else(body, else_branch);
            }
            // Destructures: the pattern walk records each binding's type
            // itself; the let-level entry keeps the initializer type for
            // the pattern node (refutability is S17's diagnostic).
            _ => {
                self.infer_let_destructure(body, pattern, initializer, else_branch.is_some());
                self.finish_let_else(body, else_branch);
            }
        }
    }

    /// Assignment typing. The value checks against the DECLARED binding
    /// type - never the narrowed overlay (B-618: a narrowed local may be
    /// re-assigned anything its declaration admits) - then the overlay
    /// re-narrows to the assigned value when it provably fits (the
    /// narrow-on-assign rule), else clears to declared.
    fn infer_assign(
        &mut self,
        body: &ExprBody,
        target: ExprId,
        value: ExprId,
        op: Option<baml_compiler2_ast::AssignOp>,
    ) {
        if Self::is_runtime_id_path(body, target) {
            self.result.type_of_expr.insert(target, Ty::string());
            if op.is_some() {
                self.pending_diags
                    .push(PendingDiag::RuntimeIdCompoundAssignment { expr: target });
                self.infer_expr(body, value, &Expectation::None);
                return;
            }

            let Some((param, throws)) = self.runtime_id_set_contract() else {
                self.infer_expr(body, value, &Expectation::None);
                return;
            };
            self.check_expr(body, value, &param);
            self.record_throw(target, &throws);
            return;
        }

        // An INDEX target (`xs[0] = v`, `xs[0] += v`): the element type
        // comes from the same `baml.ops.Index` dispatch as a read, the
        // value checks against it (expectation propagation - an empty
        // literal on the right adopts the element type), and a compound
        // op dispatches on (element, value) with the result checked
        // against the element. No binding narrows: the container's
        // declared element type is the contract.
        if let Expr::Index { base, index } = &body.exprs[target] {
            let (base, index) = (*base, *index);
            let element = self.infer_index(body, target, base, index, false);
            self.result.type_of_expr.insert(target, element.clone());
            match op {
                None => {
                    if !element.has_error() {
                        self.check_expr(body, value, &element);
                    } else {
                        self.infer_expr(body, value, &Expectation::None);
                    }
                }
                Some(op) => {
                    let rhs = self.infer_expr(body, value, &Expectation::None);
                    let result = self.compound_op_result(value, op, &element, &rhs);
                    if !element.has_error() && !self.sub(&result, &element) {
                        self.result.type_mismatches.insert(value, (element, result));
                    }
                }
            }
            return;
        }
        let binding = self.narrowable_binding(body, target);
        let declared = binding.map(|binding| self.binding_declared_ty(binding));
        // A MEMBER/PLACE target (`a.b[i].f = v`) types as an ordinary
        // place expression (r-a's `infer_assignee_expr` mirrors
        // expression inference for assignee position): the chain's types
        // and resolutions RECORD - MIR's field-slot road resolves the
        // store through them instead of falling to the dynamic
        // string-keyed store - and the place's type is the value's
        // expectation (TIR checks the assigned value against the field
        // type the same way).
        let place_ty = (binding.is_none() && op.is_none())
            .then(|| self.infer_expr(body, target, &Expectation::None));
        let assigned = match op {
            None => match (&declared, &place_ty) {
                (Some(declared), _) if !declared.has_error() => {
                    self.check_expr(body, value, declared)
                }
                (_, Some(place)) if !place.has_error() => self.check_expr(body, value, place),
                _ => self.infer_expr(body, value, &Expectation::None),
            },
            Some(op) => {
                // Compound assignment: `target op value` through the same
                // operator machinery, the result checked against declared.

                let lhs = binding
                    .map(|binding| self.binding_flow_ty(binding))
                    .unwrap_or_else(|| self.infer_expr(body, target, &Expectation::None));
                // An optional-chain place (`user?.id += 1`) SKIPS on
                // null - the op only runs when the chain produces a
                // value, so it dispatches on the peeled type (the
                // corpus pins the skip semantics).
                let lhs = if matches!(body.exprs[target], Expr::OptionalChain { .. }) {
                    self.peel_chain_null(&lhs)
                } else {
                    lhs
                };
                let rhs = self.infer_expr(body, value, &Expectation::None);
                let result = self.compound_op_result(value, op, &lhs, &rhs);
                if let Some(declared) = &declared
                    && !declared.has_error()
                    && !self.sub(&result, declared)
                {
                    self.result
                        .type_mismatches
                        .insert(value, (declared.clone(), result.clone()));
                }
                result
            }
        };
        if let Some(declared) = &declared {
            self.result.type_of_expr.insert(target, declared.clone());
        }
        if let Some(binding) = binding {
            // The overlay narrows to the assigned value's OWN type
            // (B-618's rule; TS narrows assignments to the literal the
            // same way): an assignment is a flow fact, not a binding
            // site, so ruling 1's widening does not apply - the fresh
            // literal survives into the overlay and still widens
            // wherever it later reaches a real binding or join.
            let narrowed = self.table.resolve_completely(&assigned);
            let fits = match &declared {
                Some(declared) if !declared.has_error() => {
                    crate::infer::pat::provable_subtype(&narrowed, declared, &self.facts)
                }
                _ => false,
            };
            if fits {
                self.flow.insert(binding, narrowed);
            } else {
                self.flow.remove(&binding);
            }
        }
    }

    /// The TYPE a binding's `:`-ascribed sub-pattern denotes, when it is
    /// a pure type ascription: a `Pattern::Type`, or an OR of type
    /// ascriptions - `let v: int | string` parses its annotation as an
    /// or-pattern, and the binding's recorded type is the WRITTEN union
    /// (ruling 3: bindings record what the user wrote; narrowing is for
    /// uses). Structural sub-patterns return None (the destructure walk
    /// owns those).
    fn pattern_ascription_ty(&mut self, body: &ExprBody, sub: PatId) -> Option<Ty> {
        match &body.patterns[sub] {
            Pattern::Type(_) => {
                let type_ref = self.type_refs.pattern_types.get(&sub).copied()?;
                Some(self.lower_body_annotation(type_ref))
            }
            Pattern::Or(alternatives) => {
                let alternatives = alternatives.clone();
                let mut members = Vec::new();
                for alt in alternatives {
                    if !matches!(body.patterns[alt], Pattern::Type(_)) {
                        return None;
                    }
                    let type_ref = self.type_refs.pattern_types.get(&alt).copied()?;
                    members.push(self.lower_body_annotation(type_ref));
                }
                Some(Ty::union(members))
            }
            _ => None,
        }
    }

    /// The `let ... else` tail, shared by the bind and destructure paths.
    /// Ruling: the else branch must diverge; the check itself is an S17
    /// diagnostic. Its divergence does not leak past the let.
    fn finish_let_else(&mut self, body: &ExprBody, else_branch: Option<ExprId>) {
        if let Some(else_expr) = else_branch {
            let saved = self.diverges;
            self.infer_expr(body, else_expr, &Expectation::None);
            self.diverges = saved;
        }
    }

    /// `Sub(actual, expected)` - eager discharge per the settled design:
    /// invariant same-heads decay to `Eq`; function types relate contra/co;
    /// var-headed pairs deposit bounds; ground pairs ask the canonical
    /// oracle; the irreducible residue defers to finish. Returns `false` on
    /// a DEFINITE mismatch (callers record it); undecided is `true`.
    fn sub(&mut self, actual: &Ty, expected: &Ty) -> bool {
        let mut actual = self.table.shallow_resolve(actual);
        let mut expected = self.table.shallow_resolve(expected);
        // Normalize-then-relate (rustc's FnCtxt normalize-before-unify;
        // r-a's `normalize_projection_ty` during unification): a GROUND
        // projection the oracle can already determine reduces before the
        // pair is related, so `(Iterator<Item = int> as Iterable).Item[]`
        // meets `int[]`. VAR-CARRYING projections must not enter the
        // reduction (the oracle speaks the plain algebra, whose
        // conversion erases inference vars); they relate as lazy
        // predicates through the deferred residue (`eq_piece`).
        if actual.has_projection() && !actual.has_infer() {
            actual = self.reduce_projections(&actual, PROJECTION_FINALIZE_FUEL);
        }
        if expected.has_projection() && !expected.has_infer() {
            expected = self.reduce_projections(&expected, PROJECTION_FINALIZE_FUEL);
        }
        if actual == expected || actual.has_error() || expected.has_error() {
            return true;
        }
        // A GROUND top expectation is trivially satisfied - but it IS a
        // consuming use, so an empty container literal flowing into
        // `unknown` commits its establishment slots to the top type
        // (`{}` in a `map<string, unknown>` value slot is
        // `map<unknown, unknown>` - TIR's frozen-Evolving behavior at
        // exactly the demanded case). ONLY establishment vars commit:
        // solving an ordinary inference var to `unknown` here would
        // poison its real solution. A literal with NO demand at all
        // keeps the strict uninferrable-container error (ruling 2's
        // fixture `unconstrained_empty_list_strict`).
        // A bare inference variable still records its upper bound through
        // the Infer arm below (an `unknown` upper participates in bounds
        // resolution); only STRUCTURED actuals take the fast path.
        if matches!(expected.kind(), TyKind::Unknown { .. })
            && !matches!(actual.kind(), TyKind::Infer { .. })
        {
            self.commit_establishments_to_unknown(&actual);
            return true;
        }
        // RULING (interim until tuple types): `void` and `null` are the
        // same UNIT type - a void-returning body evaluates to null, and
        // both spellings are mutually assignable (TIR's behavior).
        // Identified leaf-side so renders keep the written spelling and
        // the shared algebra is untouched; when tuples land, `void`
        // becomes the empty tuple and this arm is deleted.
        if is_unit(&actual) && is_unit(&expected) {
            return true;
        }
        match (actual.kind(), expected.kind()) {
            // A variable flowing into a context: upper bound. A value
            // flowing into a variable: lower bound. (Var-var records on
            // both sides; resolution sees through whichever solves first.)
            (TyKind::Infer { var: Some(var), .. }, _) => {
                self.table.add_upper_bound(*var, expected.clone());
                if let TyKind::Infer {
                    var: Some(other), ..
                } = expected.kind()
                {
                    self.table.add_lower_bound(*other, actual.clone());
                }
                true
            }
            (_, TyKind::Infer { var: Some(var), .. }) => {
                self.table.add_lower_bound(*var, actual.clone());
                true
            }
            // A union flowing into a context decomposes universally:
            // `A | B <: C` iff `A <: C` and `B <: C` (set semantics -
            // sound and complete regardless of variables). Decomposing
            // lets a VAR-CARRYING member meet the expectation instead of
            // deferring the whole pair to the residue, where the member's
            // variable would never receive a bound and erase (B-616: a
            // catch arm's `?E[]` member gets `?E := int` from the return
            // check here). Ground unions skip this arm and keep the
            // single oracle verdict below.
            (TyKind::Union(members, _), _) if actual.has_infer() => {
                let members: Vec<Ty> = members.to_vec();
                let mut ok = true;
                for member in members {
                    ok &= self.sub(&member, &expected);
                }
                ok
            }
            // A var-carrying union TARGET: TypeScript's
            // `inferToMultipleTypes`, the union-position inference rule
            // (`int | null <: ?R | null` must conclude `?R := int`;
            // BAML's `R?` is a bare union, so no constructor exists for
            // plain unification to match on). Constituents IDENTICAL on
            // both sides match and drop; then:
            // - exactly ONE naked variable left in the target takes the
            //   whole remaining source as a lower bound (the ordinary
            //   var arm);
            // - no naked variable, a single-member remainder, and a
            //   UNIQUE structurally-matching target constituent recurse
            //   (`int[] <: ?R[] | null` solves through the list pair);
            // - anything else - several naked variables, no unique home
            //   - defers. Forced answers only, the unique-candidate
            //   discipline; TS likewise refuses to partition a source
            //   across several naked variables.
            (_, TyKind::Union(members, _)) if expected.has_infer() => {
                let members: Vec<Ty> = members.to_vec();
                let actual_members: Vec<Ty> = match actual.kind() {
                    TyKind::Union(actual_members, _) => actual_members.to_vec(),
                    _ => vec![actual.clone()],
                };
                let (naked, targets): (Vec<Ty>, Vec<Ty>) =
                    members.into_iter().partition(|member| {
                        matches!(member.kind(), TyKind::Infer { var: Some(_), .. })
                    });
                let remainder: Vec<Ty> = actual_members
                    .into_iter()
                    .filter(|member| !targets.contains(member))
                    .collect();
                if remainder.is_empty() {
                    // Every source constituent found an identical target
                    // member: the pair holds with no inference.
                    return true;
                }
                if let [naked_var] = naked.as_slice() {
                    let naked_var = naked_var.clone();
                    // `join`, not `union_of`: the remainder flows on as a
                    // LOWER BOUND, and the canonical algebra would erase
                    // literal freshness - `fmu_pick(7, 0)` must leave
                    // `?R`'s lowers fresh so they widen and agree.
                    let source = self.join(&remainder);
                    return self.sub(&source, &naked_var);
                }
                if naked.is_empty()
                    && let [source] = remainder.as_slice()
                {
                    let matching: Vec<Ty> = targets
                        .iter()
                        .filter(|target| {
                            target.has_infer() && same_head_constructor(source, target)
                        })
                        .cloned()
                        .collect();
                    if let [target] = matching.as_slice() {
                        let source = source.clone();
                        let target = target.clone();
                        return self.sub(&source, &target);
                    }
                }
                self.deferred_subs
                    .push((actual, expected, self.obligation_anchor));
                true
            }
            // Invariant constructors: Sub decays to Eq of the pieces.
            (TyKind::Class(a_name, a_args, _), TyKind::Class(b_name, b_args, _))
                if a_name == b_name && a_args.len() == b_args.len() =>
            {
                let pairs: Vec<(Ty, Ty)> =
                    a_args.iter().cloned().zip(b_args.iter().cloned()).collect();
                let mut ok = true;
                for (a, b) in pairs {
                    ok &= self.eq_piece(&a, &b);
                }
                ok
            }
            (TyKind::List(a, _), TyKind::List(b, _)) => {
                let (a, b) = (a.clone(), b.clone());
                self.eq_piece(&a, &b)
            }
            (
                TyKind::Map {
                    key: ak, value: av, ..
                },
                TyKind::Map {
                    key: bk, value: bv, ..
                },
            ) => {
                let (ak, av, bk, bv) = (ak.clone(), av.clone(), bk.clone(), bv.clone());
                let key_ok = self.eq_piece(&ak, &bk);
                let value_ok = self.eq_piece(&av, &bv);
                key_ok && value_ok
            }
            (TyKind::Future(av, ae, _), TyKind::Future(bv, be, _)) => {
                let (av, ae, bv, be) = (av.clone(), ae.clone(), bv.clone(), be.clone());
                let value_ok = self.eq_piece(&av, &bv);
                let error_ok = self.eq_piece(&ae, &be);
                value_ok && error_ok
            }
            // Function types: contravariant params, covariant ret/throws.
            (
                TyKind::Function {
                    params: a_params,
                    ret: a_ret,
                    throws: a_throws,
                    ..
                },
                TyKind::Function {
                    params: b_params,
                    ret: b_ret,
                    throws: b_throws,
                    ..
                },
            ) if a_params.len() == b_params.len() => {
                let param_pairs: Vec<(Ty, Ty)> = a_params
                    .iter()
                    .zip(b_params.iter())
                    .map(|(a, b)| (b.ty.clone(), a.ty.clone()))
                    .collect();
                let rets = (a_ret.clone(), b_ret.clone());
                let throws = (a_throws.clone(), b_throws.clone());
                let mut ok = true;
                for (b, a) in param_pairs {
                    ok &= self.sub(&b, &a);
                }
                ok &= self.sub(&rets.0, &rets.1);
                ok &= self.sub(&throws.0, &throws.1);
                ok
            }
            _ => {
                // Ground on both sides: one oracle verdict. Otherwise the
                // pair is the deferred residue.
                let actual = self.table.resolve_completely(&actual);
                let expected = self.table.resolve_completely(&expected);
                if !actual.has_infer() && !expected.has_infer() {
                    is_subtype_interned(&actual, &expected, &self.facts)
                } else {
                    // A SAME-INTERFACE pair with variables: identity is
                    // INVARIANT (args positional, pins by name) - rustc
                    // unifies `dyn Trait<X>` against `Trait<?R>`
                    // directly. Requires-closure upcasts between
                    // DIFFERENT interfaces keep the oracle/obligation
                    // road; a requested pin the actual side lacks falls
                    // through (the fill-at-reference default is the
                    // oracle's business).
                    if let (
                        TyKind::Interface(a_name, a_args, a_pins, _),
                        TyKind::Interface(b_name, b_args, b_pins, _),
                    ) = (actual.kind(), expected.kind())
                        && a_name == b_name
                        && a_args.len() == b_args.len()
                        && (a_name.is_builtin_root_type("AnyFunction")
                            || b_pins
                                .iter()
                                .all(|(pin, _)| a_pins.iter().any(|(a_pin, _)| a_pin == pin)))
                    {
                        let arg_pairs: Vec<(Ty, Ty)> =
                            a_args.iter().cloned().zip(b_args.iter().cloned()).collect();
                        // An `AnyFunction` pin the actual side leaves unwritten
                        // reads as its declared `unknown` default (BEP-062), so
                        // a bare value binds the expectation's pin variables to
                        // `unknown` instead of stranding them.
                        let pin_pairs: Vec<(Ty, Ty)> = b_pins
                            .iter()
                            .map(|(pin, b_ty)| {
                                match a_pins.iter().find(|(a_pin, _)| a_pin == pin) {
                                    Some((_, a_ty)) => (a_ty.clone(), b_ty.clone()),
                                    None => (
                                        Ty::intern(TyKind::Unknown {
                                            attr: TyAttr::default(),
                                        }),
                                        b_ty.clone(),
                                    ),
                                }
                            })
                            .collect();
                        let mut ok = true;
                        for (a, b) in arg_pairs.into_iter().chain(pin_pairs) {
                            ok &= self.eq_piece(&a, &b);
                        }
                        return ok;
                    }
                    // An interface EXPECTATION in a var-carrying pair is
                    // an Implements GOAL, not an inert pair: rustc
                    // registers the trait obligation and confirmation
                    // unifies, which both proves the goal and BINDS the
                    // expectation's variables (`int[] <: Iterable<Item =
                    // ?R>` confirms through the `T[]` impl, pinning `?R
                    // := int`). Fulfillment handles ground goals, var
                    // goals via selection, and ambiguity by stalling.
                    if let TyKind::Interface(name, args, pins, _) = expected.kind()
                        && !matches!(actual.kind(), TyKind::Infer { .. })
                        && let Some(anchor) = self.obligation_anchor
                    {
                        let interface = baml_type::interned::InterfaceRef::new(
                            name.clone(),
                            args.to_vec().into_boxed_slice(),
                            pins.to_vec(),
                        );
                        // A union is a subtype of an existential iff ALL
                        // members are (spec: Variance rule 2.1) - and
                        // only concrete types implement, so the goal
                        // decomposes per member before registration.
                        let goals: Vec<Ty> = match actual.kind() {
                            TyKind::Union(members, _) => members.to_vec(),
                            _ => vec![actual],
                        };
                        for goal in goals {
                            self.register_obligation(obligations::Obligation::Implements {
                                ty: goal,
                                interface: interface.clone(),
                                at: anchor,
                                not_concrete_rejects: false,
                            });
                        }
                        return true;
                    }
                    self.deferred_subs
                        .push((actual, expected, self.obligation_anchor));
                    true
                }
            }
        }
    }

    /// Solves every ESTABLISHMENT variable inside `ty` to the top type -
    /// the commitment a ground `unknown` demand makes on an empty
    /// container literal's slots. Ordinary inference vars are untouched.
    fn commit_establishments_to_unknown(&mut self, ty: &Ty) {
        if !ty.has_infer() {
            return;
        }
        let resolved = self.table.shallow_resolve(ty);
        if let TyKind::Infer { var: Some(var), .. } = resolved.kind() {
            if self.table.is_establishment_var(*var) {
                self.table.solve(
                    *var,
                    Ty::intern(TyKind::Unknown {
                        attr: TyAttr::default(),
                    }),
                );
            }
            return;
        }
        let mut children = Vec::new();
        baml_type::interned::for_each_child(resolved.kind(), |child| children.push(child.clone()));
        for child in children {
            self.commit_establishments_to_unknown(&child);
        }
    }

    /// One invariant piece of a decayed Sub: SEMANTIC equality. A ground
    /// pair asks the canonical oracle (`equivalent` reduces projections,
    /// expands aliases, normalizes unions - `(C as I).Item` IS `int` when
    /// the impl binds it); a variable-carrying pair unifies through the
    /// table. Structural table unification on ground pairs was a real
    /// bug: it judged `(IntStore as Store).Item[] </: int[]` and recorded
    /// a mismatch whose two sides FINALIZE to the same type - the first
    /// catch of the error-channel contract.
    fn eq_piece(&mut self, a: &Ty, b: &Ty) -> bool {
        let a = self.table.resolve_completely(a);
        let b = self.table.resolve_completely(b);
        if is_unit(&a) && is_unit(&b) {
            return true;
        }
        if !a.has_infer() && !b.has_infer() {
            return equivalent_interned(&a, &b, &self.facts);
        }
        // A projection whose base still carries variables cannot relate
        // structurally - rustc keeps the pair as a lazy `Projection`
        // predicate and discharges it once inference progresses. The
        // deferred `Sub` residue is that ledger here: both directions
        // (this is Eq), re-examined at finish after resolution.
        if a.has_projection() || b.has_projection() {
            self.deferred_subs
                .push((a.clone(), b.clone(), self.obligation_anchor));
            self.deferred_subs.push((b, a, self.obligation_anchor));
            return true;
        }
        self.table.unify(&a, &b).is_ok()
    }

    /// A union of members that may still contain inference variables. The
    /// canonical algebra consults the semantic oracle and REQUIRES
    /// var-free input (the normalizer's invariant), so a var-containing
    /// join stays syntactic until resolution - the S13 finalize pass
    /// re-canonicalizes once every variable is solved or ruled an error.
    fn union_of(&mut self, members: &[Ty]) -> Ty {
        if members.iter().any(Ty::has_infer) {
            return syntactic_union(members);
        }
        canonical_union_interned(members, &self.facts)
    }

    /// The control-flow join: a canonical union that PRESERVES literal
    /// freshness across the round-trip (the canonical algebra erases
    /// freshness as identity-irrelevant, but widening at the eventual
    /// binding site still needs it: `if c { 1 } else { 2 }` is the fresh
    /// `1 | 2`, widening to `int` at a binding - while `true | false`
    /// collapses to `bool` here, where freshness no longer matters).
    fn join(&mut self, members: &[Ty]) -> Ty {
        // Collect literal witnesses by freshness (top-level members and
        // one union layer - the shapes arm joins produce). A value with
        // a RIGID witness stays rigid: TS's union-widening rule (a
        // union widens only when ALL constituents are widening
        // literals), and the same non-widening-witness preference
        // `try_solve_bounded_var` applies - one merge policy, two
        // sites.
        let mut fresh: Vec<Literal> = Vec::new();
        let mut regular: Vec<Literal> = Vec::new();
        let mut collect = |ty: &Ty| {
            if let TyKind::Literal(lit, freshness, _) = ty.kind() {
                match freshness {
                    Freshness::Fresh => fresh.push(lit.clone()),
                    Freshness::Regular => regular.push(lit.clone()),
                }
            }
        };
        for member in members {
            match member.kind() {
                TyKind::Union(inner, _) => inner.iter().for_each(&mut collect),
                _ => collect(member),
            }
        }
        let joined = self.union_of(members);
        if fresh.is_empty() {
            return joined;
        }
        let remark = |ty: &Ty| -> Ty {
            match ty.kind() {
                TyKind::Literal(lit, Freshness::Regular, attr)
                    if fresh.contains(lit) && !regular.contains(lit) =>
                {
                    Ty::intern(TyKind::Literal(lit.clone(), Freshness::Fresh, attr.clone()))
                }
                _ => ty.clone(),
            }
        };
        match joined.kind() {
            TyKind::Union(joined_members, attr) => Ty::intern(TyKind::Union(
                joined_members.iter().map(remark).collect(),
                attr.clone(),
            )),
            _ => remark(&joined),
        }
    }

    /// Fresh literals widen to their base primitive at binding sites and
    /// joins; a union of fresh literals widens member-wise and
    /// re-canonicalizes (`1 | 2` at a binding is `int`).
    fn widen_fresh(&mut self, ty: &Ty) -> Ty {
        match ty.kind() {
            TyKind::Literal(_, Freshness::Fresh, _) => widen_fresh_literal(ty),
            TyKind::Union(members, _)
                if members.iter().any(|member| {
                    matches!(member.kind(), TyKind::Literal(_, Freshness::Fresh, _))
                }) =>
            {
                let widened: Vec<Ty> = members.iter().map(widen_fresh_literal).collect();
                self.union_of(&widened)
            }
            _ => ty.clone(),
        }
    }

    /// Binary operator typing. Dispatching operators (arithmetic, ordered
    /// comparison) go through the interfaces - decision 3B, matching TIR's
    /// arithmetic arm; the structural ones (`&&`/`||` short-circuit
    /// control flow, `==`/`!=` structural equality over `Concrete`, `??`
    /// null-algebra) are type algebra, not dispatch. Operand-validity
    /// diagnostics are S17's; the Compare obligation on ordered
    /// comparisons lands with I4.
    fn infer_binary(
        &mut self,
        body: &ExprBody,
        expr: ExprId,
        op: baml_compiler2_ast::BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
    ) -> Ty {
        use baml_compiler2_ast::BinaryOp;
        match op {
            BinaryOp::And | BinaryOp::Or => {
                let lhs_ty = self.check_expr(body, lhs, &Ty::bool());
                let rhs_ty = self.check_expr(body, rhs, &Ty::bool());
                let lhs_ty = self.table.resolve_completely(&lhs_ty);
                let rhs_ty = self.table.resolve_completely(&rhs_ty);
                const_fold_binary(op, &lhs_ty, &rhs_ty).unwrap_or_else(Ty::bool)
            }
            BinaryOp::Eq | BinaryOp::Ne => {
                let lhs_ty = self.infer_expr(body, lhs, &Expectation::None);
                let rhs_ty = self.infer_expr(body, rhs, &Expectation::None);
                let lhs_ty = self.table.resolve_completely(&lhs_ty);
                let rhs_ty = self.table.resolve_completely(&rhs_ty);
                // Equality folds whenever compile time can DECIDE it
                // (ruling 2026-08-07), in TIR's layering: the literal
                // VALUE table first (same-base pairs, floats included),
                // then the shared algebra's `constant_equality` - false
                // for provably-DISJOINT operands (under the language's
                // equality semantics: int/float overlap numerically,
                // bigint does not; a non-nullable type never equals
                // `null`), true for equal unoverridable singletons.
                // Everything else is `bool`.
                if let Some(folded) = const_fold_binary(op, &lhs_ty, &rhs_ty) {
                    return folded;
                }
                if !lhs_ty.has_infer()
                    && !rhs_ty.has_infer()
                    && !lhs_ty.has_error()
                    && !rhs_ty.has_error()
                    && let Some(equal) = baml_type::normalize::TypeContext::constant_equality(
                        &self.facts,
                        &lhs_ty.to_plain(),
                        &rhs_ty.to_plain(),
                    )
                {
                    if !equal {
                        // Disjoint operands: the comparison is pointless.
                        self.pending_diags
                            .push(PendingDiag::ComparisonAlwaysDisjoint {
                                at: expr,
                                op,
                                lhs: lhs_ty,
                                rhs: rhs_ty,
                            });
                    }
                    let value = if matches!(op, BinaryOp::Eq) {
                        equal
                    } else {
                        !equal
                    };
                    return Ty::intern(TyKind::Literal(
                        Literal::Bool(value),
                        Freshness::Fresh,
                        TyAttr::default(),
                    ));
                }
                Ty::bool()
            }
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                let lhs_ty = self.infer_expr(body, lhs, &Expectation::None);
                let rhs_ty = self.infer_expr(body, rhs, &Expectation::None);
                let lhs_ty = self.table.resolve_completely(&lhs_ty);
                let rhs_ty = self.table.resolve_completely(&rhs_ty);
                // Exact-type ordering (TIR's rule): both operands widen
                // (literal -> base, variant -> enum) and must be the SAME
                // type - subtyping is not enough - and that type must
                // implement `baml.ops.Compare`. Open/error operands skip
                // (cascades; a var may still solve either way).
                if !lhs_ty.has_infer()
                    && !rhs_ty.has_infer()
                    && !lhs_ty.has_error()
                    && !rhs_ty.has_error()
                    && !matches!(lhs_ty.kind(), TyKind::Unknown { .. })
                    && !matches!(rhs_ty.kind(), TyKind::Unknown { .. })
                {
                    let widen = |this: &mut Self, ty: &Ty| -> baml_type::Ty {
                        use baml_base::Literal as Lit;
                        let plain = this.expand_alias_ty(ty).to_plain();
                        match plain {
                            baml_type::Ty::Literal(Lit::Int(_), _, attr) => {
                                baml_type::Ty::Int { attr }
                            }
                            baml_type::Ty::Literal(Lit::Bigint(_), _, attr) => {
                                baml_type::Ty::Bigint { attr }
                            }
                            baml_type::Ty::Literal(Lit::Float(_), _, attr) => {
                                baml_type::Ty::Float { attr }
                            }
                            baml_type::Ty::Literal(Lit::String(_), _, attr) => {
                                baml_type::Ty::String { attr }
                            }
                            baml_type::Ty::Literal(Lit::Bool(_), _, attr) => {
                                baml_type::Ty::Bool { attr }
                            }
                            baml_type::Ty::EnumVariant(name, _, attr) => {
                                baml_type::Ty::Enum(name, attr)
                            }
                            other => other,
                        }
                    };
                    let lhs_base = widen(self, &lhs_ty);
                    let rhs_base = widen(self, &rhs_ty);
                    if !baml_type::normalize::equivalent(&lhs_base, &rhs_base, &self.facts) {
                        self.pending_diags
                            .push(PendingDiag::OrderingDifferentTypes {
                                at: expr,
                                op,
                                lhs: lhs_ty.clone(),
                                rhs: rhs_ty.clone(),
                            });
                    } else {
                        // Ordering needs a SINGLE concrete type (or a
                        // bounded rigid realizing to one) implementing
                        // `baml.ops.Compare`; unions and existentials are
                        // not orderable even member-wise (TIR's rule).
                        let compare_existential = baml_type::Ty::Interface(
                            baml_type::QualifiedTypeName::new(
                                baml_base::Name::new("baml"),
                                vec![baml_base::Name::new("ops")],
                                baml_base::Name::new("Compare"),
                            ),
                            Vec::new(),
                            Vec::new(),
                            baml_type::TyAttr::default(),
                        );
                        let comparable = !matches!(
                            lhs_base,
                            baml_type::Ty::Union(..)
                                | baml_type::Ty::Interface(..)
                                | baml_type::Ty::Unknown { .. }
                        ) && baml_type::normalize::is_subtype(
                            &lhs_base,
                            &compare_existential,
                            &self.facts,
                        );
                        if !comparable {
                            self.pending_diags
                                .push(PendingDiag::OrderingRequiresCompare {
                                    at: expr,
                                    op,
                                    ty: Ty::from_plain(&lhs_base),
                                });
                        }
                    }
                }
                const_fold_binary(op, &lhs_ty, &rhs_ty).unwrap_or_else(Ty::bool)
            }
            BinaryOp::NullCoalesce => {
                let lhs_ty = self.infer_expr(body, lhs, &Expectation::None);
                // B-1135: the unwrapped lhs INFORMS the rhs, so `xs ?? []`
                // adopts the element type instead of leaving a hole. It
                // does not CONSTRAIN it - `v ?? "fallback"` is a join, not
                // a mismatch - which is exactly Expectation's inform/
                // constrain split (same as if-branches).
                let inner = self.remove_null(&lhs_ty);
                let rhs_ty = self.infer_expr(body, rhs, &Expectation::has_type(inner.clone()));
                self.null_coalesce(inner, &rhs_ty)
            }
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                let lhs_ty = self.infer_expr(body, lhs, &Expectation::None);
                let rhs_ty = self.infer_expr(body, rhs, &Expectation::None);
                let lhs_ty = self.table.resolve_completely(&lhs_ty);
                let rhs_ty = self.table.resolve_completely(&rhs_ty);
                if let Some(folded) = const_fold_binary(op, &lhs_ty, &rhs_ty) {
                    return folded;
                }
                let interface = match op {
                    BinaryOp::Add => "Add",
                    BinaryOp::Sub => "Subtract",
                    BinaryOp::Mul => "Multiply",
                    BinaryOp::Div => "Divide",
                    BinaryOp::Mod => "Remainder",
                    _ => unreachable!("outer match arm"),
                };
                self.operator_or_obligation(expr, interface, &lhs_ty, Some(&rhs_ty))
            }
            // Bitwise dispatches through the `baml.ops` interfaces like
            // every other operator (decision 3B); the stdlib grew them
            // with B-1075 and the hack table is gone.
            BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor
            | BinaryOp::Shl
            | BinaryOp::Shr => {
                let lhs_ty = self.infer_expr(body, lhs, &Expectation::None);
                let rhs_ty = self.infer_expr(body, rhs, &Expectation::None);
                let lhs_ty = self.table.resolve_completely(&lhs_ty);
                let rhs_ty = self.table.resolve_completely(&rhs_ty);
                if let Some(folded) = const_fold_binary(op, &lhs_ty, &rhs_ty) {
                    return folded;
                }
                let interface = match op {
                    BinaryOp::BitAnd => "BitAnd",
                    BinaryOp::BitOr => "BitOr",
                    BinaryOp::BitXor => "BitXor",
                    BinaryOp::Shl => "ShiftLeft",
                    BinaryOp::Shr => "ShiftRight",
                    _ => unreachable!("outer match arm"),
                };
                self.operator_or_obligation(expr, interface, &lhs_ty, Some(&rhs_ty))
            }
        }
    }

    /// `spawn name? with...? { body } : Future<T, E>` (BEP-034; rustc's
    /// async-block shape). The body arrives as a synthetic 0-arg lambda
    /// and types through the ordinary lambda path - its OWN effect
    /// channel (the S12 discipline) is the future's error side, read
    /// straight off the lambda's fn type. Fresh literals widen out of
    /// both slots. `with` transformers fold left-to-right over
    /// `SpawnParams<T, E>`: each checks against
    /// `(SpawnParams<cur>) -> SpawnParams<unknown, unknown>`, the
    /// concrete input binding a generic transformer's params through
    /// ordinary unification (TIR needs a value-ref workaround here;
    /// inference variables make it unnecessary), and the transformer's
    /// OUTPUT args seed the next link.
    fn infer_spawn(
        &mut self,
        body: &ExprBody,
        name: Option<ExprId>,
        with_exprs: &[ExprId],
        spawn_body: ExprId,
    ) -> Ty {
        if let Some(name_id) = name {
            self.infer_expr(body, name_id, &Expectation::None);
        }
        let lambda_ty = self.infer_expr(body, spawn_body, &Expectation::None);
        let resolved = self.structurally_resolve(&lambda_ty);
        let (value, error) = match resolved.kind() {
            TyKind::Function { ret, throws, .. } => (ret.clone(), throws.clone()),
            _ => (resolved.clone(), Ty::never()),
        };
        let mut cur_value = self.widen_fresh(&value);
        let mut cur_error = self.widen_fresh(&error);
        for &with_id in with_exprs {
            let unknown = || {
                Ty::intern(TyKind::Unknown {
                    attr: TyAttr::default(),
                })
            };
            // A with-modifier is DEMANDED structurally (the infer_await
            // discipline for language constructs): the expectation below
            // flows into lambdas and generic calls (`options(group = g)`
            // solves its `T`/`E` from the chain via the param
            // unification), but the verdict is the shape check - a full
            // subtype check against open `unknown` slots would trip the
            // class-invariance rule on perfectly good modifiers.
            let expected = Ty::intern(TyKind::Function {
                params: Box::new([baml_type::interned::FunctionParam {
                    name: None,
                    ty: spawn_params_ty(cur_value.clone(), cur_error.clone()),
                    mode: baml_type::FunctionParamMode::Required,
                }]),
                ret: spawn_params_ty(unknown(), unknown()),
                throws: unknown(),
                attr: TyAttr::default(),
            });
            let got = self.infer_expr(body, with_id, &Expectation::has_type(expected.clone()));
            let got = self.structurally_resolve(&got);
            let link = match got.kind() {
                TyKind::Function { params, ret, .. } => {
                    let ret = ret.clone();
                    let ret = self.structurally_resolve(&ret);
                    match ret.kind() {
                        TyKind::Class(qn, args, _)
                            if is_spawn_params_qtn(qn) && args.len() == 2 =>
                        {
                            // The modifier must accept the chain's
                            // current link (solving its generics when
                            // still open).
                            if let Some(param) = params.first() {
                                let chain = spawn_params_ty(cur_value.clone(), cur_error.clone());
                                let param_ty = param.ty.clone();
                                if !self.sub(&chain, &param_ty) {
                                    // Full transformer types on both sides:
                                    // the render then shows the chain's
                                    // concrete input against the
                                    // transformer's (TIR's shape).
                                    self.result
                                        .type_mismatches
                                        .insert(with_id, (expected.clone(), got.clone()));
                                }
                            }
                            let ret = self.table.resolve_completely(&ret);
                            match ret.kind() {
                                TyKind::Class(_, args, _) => {
                                    Some((args[0].clone(), args[1].clone()))
                                }
                                _ => None,
                            }
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
            match link {
                Some((value, error)) => {
                    cur_value = self.table.resolve_completely(&value);
                    cur_error = self.table.resolve_completely(&error);
                }
                None => {
                    // Not a transformer at all. A fn-shaped or value-ref
                    // link gets the middleware-contract wording (TIR's
                    // SpawnWithNotATransformer); a direct non-fn value
                    // keeps the readable shape mismatch.
                    let is_value_ref = matches!(
                        &body.exprs[with_id],
                        Expr::Path(_) | Expr::MemberAccess { .. }
                    );
                    let got_resolved = self.table.resolve_completely(&got);
                    if (is_value_ref || matches!(got_resolved.kind(), TyKind::Function { .. }))
                        && !got_resolved.has_error()
                        && !matches!(got_resolved.kind(), TyKind::Unknown { .. })
                    {
                        self.pending_diags.push(PendingDiag::SpawnWithBad {
                            at: with_id,
                            expected_input: spawn_params_ty(cur_value.clone(), cur_error.clone()),
                            got: got_resolved,
                        });
                    } else {
                        self.result.type_mismatches.insert(with_id, (expected, got));
                    }
                }
            }
        }
        Ty::intern(TyKind::Future(cur_value, cur_error, TyAttr::default()))
    }

    /// `await e : T` for `e : Future<T, E>`; `E` joins the effect
    /// channel like any throw site. DISTRIBUTES over a union of futures
    /// (BEP-034: `Future` is invariant, so mixed spawns join as a union
    /// of futures, not a future of unions) - values union, each error
    /// side contributes. `never` passes through (an unreachable await);
    /// a still-unsolved operand is DEMANDED structurally (unified with
    /// `Future<?V, ?E>`); a non-future records the mismatch against
    /// `Future<unknown, unknown>` (TIR's expected render).
    fn infer_await(&mut self, body: &ExprBody, expr: ExprId, future: ExprId) -> Ty {
        let fut = self.infer_expr(body, future, &Expectation::None);
        let resolved = self.structurally_resolve(&fut);
        match resolved.kind() {
            TyKind::Future(value, error, _) => {
                let (value, error) = (value.clone(), error.clone());
                self.record_throw(expr, &error);
                value
            }
            TyKind::Union(members, _)
                if !members.is_empty()
                    && members
                        .iter()
                        .all(|member| matches!(member.kind(), TyKind::Future(..))) =>
            {
                let mut values = Vec::new();
                for member in members {
                    if let TyKind::Future(value, error, _) = member.kind() {
                        values.push(value.clone());
                        let error = error.clone();
                        self.record_throw(expr, &error);
                    }
                }
                self.union_of(&values)
            }
            TyKind::Never { .. } => resolved,
            TyKind::Infer { .. } => {
                let value = self.table.new_var_ty();
                let error = self.table.new_effect_var_ty();
                let demanded = Ty::intern(TyKind::Future(
                    value.clone(),
                    error.clone(),
                    TyAttr::default(),
                ));
                let _ = self.table.unify(&resolved, &demanded);
                self.record_throw(expr, &error);
                value
            }
            _ if resolved.has_error() => resolved,
            _ => {
                let unknown = || {
                    Ty::intern(TyKind::Unknown {
                        attr: TyAttr::default(),
                    })
                };
                let expected = Ty::intern(TyKind::Future(unknown(), unknown(), TyAttr::default()));
                self.result
                    .type_mismatches
                    .insert(expr, (expected, resolved));
                Ty::error()
            }
        }
    }

    /// One compound-assignment step: `lhs op rhs` through the operator
    /// machinery, shared by binding and index targets.
    fn compound_op_result(
        &mut self,
        at: ExprId,
        op: baml_compiler2_ast::AssignOp,
        lhs: &Ty,
        rhs: &Ty,
    ) -> Ty {
        use baml_compiler2_ast::AssignOp;
        let interface = match op {
            AssignOp::Add => "Add",
            AssignOp::Sub => "Subtract",
            AssignOp::Mul => "Multiply",
            AssignOp::Div => "Divide",
            AssignOp::Mod => "Remainder",
            AssignOp::BitAnd => "BitAnd",
            AssignOp::BitOr => "BitOr",
            AssignOp::BitXor => "BitXor",
            AssignOp::Shl => "ShiftLeft",
            AssignOp::Shr => "ShiftRight",
        };
        // The reporting road (an inapplicable compound operator is the
        // same E0004 the binary spelling gets), anchored at the value.
        self.operator_or_obligation(at, interface, lhs, Some(rhs))
    }

    /// `base[idx]` dispatches through `baml.ops.Index` (the ruling:
    /// Rust's `ops::Index` shape - stdlib blankets cover lists, maps,
    /// and uint8array; MIR rewrites statically-typed cases to direct
    /// instructions). The OPTIONAL form (`base?[idx]`) unwraps a
    /// nullable base, dispatches on the payload, and rewraps the result
    /// with `| null`; a nullable base in the PLAIN form reaches dispatch
    /// as the union, whose `null` member has no impl - the mismatch
    /// records (TIR's `NullableMemberAccess`, rendered at S17).
    fn infer_index(
        &mut self,
        body: &ExprBody,
        expr: ExprId,
        base: ExprId,
        index: ExprId,
        optional: bool,
    ) -> Ty {
        let base_ty = self.infer_expr(body, base, &Expectation::None);
        let subject = if optional {
            // Chain semantics: the base's null peels here and the
            // enclosing OptionalChain boundary re-unions it.
            self.peel_chain_null(&base_ty)
        } else {
            base_ty
        };
        // Builtin indexing first (rustc's `try_index_step` order: the
        // structural forms index directly - the bytecode special case
        // TIR shares), the `baml.ops.Index` interface otherwise (generic
        // `T extends Index<..>` bounds). The structural tier is what lets
        // a rigid-element `T[]` index without `root.Concrete` evidence.
        let resolved_subject = self.structurally_resolve(&subject);
        // `?.[]` short-circuits on a null INDEX too (`arr?.[i]` with
        // `i: int?` is null), so the optional form's key check admits
        // null; the plain form stays strict.
        let key_expectation = |this: &mut Self, key: Ty| {
            if optional {
                this.union_of(&[key, Ty::null()])
            } else {
                key
            }
        };
        let element = match resolved_subject.kind() {
            TyKind::List(element, _) => {
                let element = element.clone();
                let expectation = key_expectation(self, Ty::int());
                self.check_expr(body, index, &expectation);
                element
            }
            TyKind::Map { key, value, .. } => {
                let key = key.clone();
                let value = value.clone();
                let expectation = key_expectation(self, key);
                self.check_expr(body, index, &expectation);
                value
            }
            TyKind::Uint8Array { .. } => {
                let expectation = key_expectation(self, Ty::int());
                self.check_expr(body, index, &expectation);
                Ty::int()
            }
            _ => {
                let index_ty = self.infer_expr(body, index, &Expectation::None);
                self.operator_or_obligation(expr, "Index", &resolved_subject, Some(&index_ty))
            }
        };
        if optional {
            // A nullable INDEX short-circuits too (`arr?.[i]` with
            // `i: int?` is null at runtime when `i` is): mark the chain
            // frame; the boundary owns the `| null`.
            let index_ty = self
                .result
                .type_of_expr
                .get(&index)
                .cloned()
                .map(|ty| self.table.resolve_completely(&ty));
            let index_nullable = index_ty
                .map(|ty| self.remove_null(&ty) != ty)
                .unwrap_or(false);
            if index_nullable && let Some(top) = self.chain_nullable.last_mut() {
                *top = true;
            }
        }
        element
    }

    fn infer_unary(
        &mut self,
        body: &ExprBody,
        op: baml_compiler2_ast::UnaryOp,
        operand: ExprId,
    ) -> Ty {
        match op {
            baml_compiler2_ast::UnaryOp::Not => {
                let ty = self.check_expr(body, operand, &Ty::bool());
                // `!` on a literal bool constant-FOLDS (TIR's
                // `try_fold_unary`), freshness preserved.
                let resolved = self.table.resolve_completely(&ty);
                if let TyKind::Literal(Literal::Bool(value), freshness, _) = resolved.kind() {
                    return Ty::intern(TyKind::Literal(
                        Literal::Bool(!value),
                        *freshness,
                        TyAttr::default(),
                    ));
                }
                Ty::bool()
            }
            baml_compiler2_ast::UnaryOp::Neg => {
                let ty = self.infer_expr(body, operand, &Expectation::None);
                let dispatched = self.operator_or_obligation(operand, "Negate", &ty, None);
                // Negative LITERAL types (ruling 2, TS parity, TIR's
                // discipline): dispatch through `Negate` is the semantic
                // gate above; a literal operand then constant-FOLDS to
                // the negated literal, preserving freshness, so `-1` has
                // type `-1`. An i63-range overflow (`-INT_MIN`) skips the
                // fold and keeps the dispatch result - the VM throws the
                // same catchable IntegerOverflow either way.
                let resolved = self.table.resolve_completely(&ty);
                if let TyKind::Literal(lit, freshness, _) = resolved.kind()
                    && !dispatched.has_error()
                    && let Some(folded) = negate_literal(lit, *freshness)
                {
                    return folded;
                }
                dispatched
            }
        }
    }

    /// Operator typing at a use site: ground operands dispatch now;
    /// operands still carrying inference variables REGISTER an operator
    /// obligation (I4 - rust-analyzer's register-and-fulfill, never
    /// guess-or-fail early) whose fresh output variable stands for the
    /// result until discharge.
    fn operator_or_obligation(
        &mut self,
        at: ExprId,
        interface: &'static str,
        lhs: &Ty,
        rhs: Option<&Ty>,
    ) -> Ty {
        let lhs_resolved = self.table.resolve_completely(lhs);
        let rhs_resolved = rhs.map(|ty| self.table.resolve_completely(ty));
        if lhs_resolved.has_infer() || rhs_resolved.as_ref().is_some_and(Ty::has_infer) {
            let out = self.table.new_var_ty();
            self.register_obligation(obligations::Obligation::Operator {
                interface,
                lhs: lhs_resolved,
                rhs: rhs_resolved,
                out: out.clone(),
                at,
            });
            return out;
        }
        let result = self.dispatch_operator(interface, &lhs_resolved, rhs_resolved.as_ref());
        if result.has_error() {
            self.report_operator_failure(at, interface, &lhs_resolved, rhs_resolved.as_ref());
        }
        result
    }

    /// A ground operator dispatch found NO impl for clean operands - the
    /// committed E0004 (an error/unknown/var operand is a cascade and
    /// stays silent, the same `tainted_by_errors` rule everywhere).
    pub(super) fn report_operator_failure(
        &mut self,
        at: ExprId,
        interface: &'static str,
        lhs: &Ty,
        rhs: Option<&Ty>,
    ) {
        // SHALLOW error/unknown screening (TIR's gate): a nested error slot
        // (an incomplete existential's recovered pin) does not silence the
        // operator report - the operand as written still has no impl, and
        // that is this diagnostic's claim.
        let dirty = |ty: &Ty| {
            matches!(ty.kind(), TyKind::Error { .. } | TyKind::Unknown { .. }) || ty.has_infer()
        };
        if dirty(lhs) || rhs.is_some_and(dirty) {
            return;
        }
        self.pending_diags.push(PendingDiag::OperatorNotApplicable {
            expr: at,
            interface,
            lhs: lhs.clone(),
            rhs: rhs.cloned(),
        });
    }

    /// The GROUND dispatch: every (lhs alternative, rhs alternative) pair
    /// of the operands' union members must have an impl of
    /// `baml.ops.<interface>`; the result is the join of the Outputs.
    /// Literals widen to their bases for lookup (folding literal
    /// arithmetic is a later refinement). `never` propagates
    /// (unreachable-operand rule); Error/unknown operands suppress to the
    /// sentinel. Also the discharge rule for operator obligations.
    pub(super) fn dispatch_operator(&mut self, interface: &str, lhs: &Ty, rhs: Option<&Ty>) -> Ty {
        // Normalize-then-dispatch (rustc normalizes obligations before
        // selection): operands resolve through the ONE structure demand
        // - ground projections reduce (`(T as Source).Item + ":"`
        // dispatches Add on `string`) and weak aliases expand (a union
        // alias splits pairwise like the union it names).
        let lhs_resolved = self.structurally_resolve(lhs);
        let lhs = &lhs_resolved;
        let rhs_resolved = rhs.map(|rhs_ty| self.structurally_resolve(rhs_ty));
        let rhs = rhs_resolved.as_ref();
        let lhs = self.table.resolve_completely(lhs);
        let rhs = rhs.map(|ty| self.table.resolve_completely(ty));
        if matches!(lhs.kind(), TyKind::Never { .. })
            || rhs
                .as_ref()
                .is_some_and(|ty| matches!(ty.kind(), TyKind::Never { .. }))
        {
            return Ty::never();
        }
        let undispatchable = |ty: &Ty| {
            ty.has_error() || ty.has_infer() || matches!(ty.kind(), TyKind::Unknown { .. })
        };
        if undispatchable(&lhs) || rhs.as_ref().is_some_and(undispatchable) {
            return Ty::error();
        }
        let mut outputs = Vec::new();
        for lhs_member in operand_members(&lhs) {
            match &rhs {
                Some(rhs) => {
                    for rhs_member in operand_members(rhs) {
                        match self.member_operator_output(interface, &lhs_member, Some(&rhs_member))
                        {
                            Some(output) => outputs.push(output),
                            None => return Ty::error(),
                        }
                    }
                }
                None => match self.member_operator_output(interface, &lhs_member, None) {
                    Some(output) => outputs.push(output),
                    None => return Ty::error(),
                },
            }
        }
        self.union_of(&outputs)
    }

    /// One operand pair's operator result: a rigid operand dispatches
    /// through its CARRIED bound (I2 - the spec's `T extends
    /// baml.ops.Add<O>` example), yielding the bound's `Output` pin or
    /// the symbolic projection; everything else asks the impl registry.
    fn member_operator_output(
        &mut self,
        interface: &str,
        lhs: &Ty,
        rhs: Option<&Ty>,
    ) -> Option<Ty> {
        if let TyKind::TypeVar(param, _) = lhs.kind() {
            let bounds = baml_type::normalize::TypeContext::type_var_bound(&self.facts, param);
            let bound = bounds.iter().find(|bound| {
                !bound.name.is_local()
                    && bound.name.package().as_str() == "baml"
                    && bound.name.namespace().len() == 1
                    && bound.name.namespace()[0].as_str() == "ops"
                    && bound.name.name().as_str() == interface
                    && match rhs {
                        Some(rhs) => {
                            bound.generics.len() == 1 && Ty::from_plain(&bound.generics[0]) == *rhs
                        }
                        None => bound.generics.is_empty(),
                    }
            })?;
            if let Some((_, pinned)) = bound
                .associated_types
                .iter()
                .find(|(name, _)| name.as_str() == "Output")
            {
                return Some(Ty::from_plain(pinned));
            }
            // No `Output` pin: a bound whose args are still SYMBOLIC keeps
            // the canonical projection (the spec's `T extends Add<O>`
            // example - `T.Output` is the signature's own name for it). A
            // fully GROUND bound (`T extends Add<int>`) leaves the result
            // genuinely underdetermined - each implementor picks its own
            // `Output` - so the operand must pin it (the fixture rule).
            if !bound
                .generics
                .iter()
                .any(baml_type_runtime::contains_typevar)
            {
                return None;
            }
            return Some(Ty::intern(TyKind::AssociatedTypeProjection {
                base: lhs.clone(),
                interface: baml_type::interned::InterfaceRef::new(
                    bound.name.clone(),
                    bound.generics.iter().map(Ty::from_plain).collect(),
                    bound
                        .associated_types
                        .iter()
                        .map(|(name, ty)| (name.clone(), Ty::from_plain(ty)))
                        .collect(),
                ),
                member: baml_type::Name::new("Output"),
                attr: TyAttr::default(),
            }));
        }
        // An interface-EXISTENTIAL operand dispatches through its own
        // interface (or one it `requires`): `x: baml.ops.Add<int, Output =
        // int>` adds like the virtual `x.add(rhs)` call it desugars to.
        // The pin is the result; a recovered error pin surfaces as the
        // operator failure (the completeness check already named it).
        if let TyKind::Interface(qtn, args, pins, _) = lhs.kind() {
            let root = baml_type::interned::InterfaceRef::new(
                qtn.clone(),
                (args.to_vec()).into(),
                pins.to_vec(),
            );
            let mut heads = vec![root.clone()];
            heads.extend(crate::impls::direct_requires_closure(
                self.db, &root, lhs, 8,
            ));
            let head = heads.into_iter().find(|head| {
                !head.name.is_local()
                    && head.name.package().as_str() == "baml"
                    && head.name.namespace().len() == 1
                    && head.name.namespace()[0].as_str() == "ops"
                    && head.name.name().as_str() == interface
                    && match rhs {
                        Some(rhs) => head.generics.len() == 1 && head.generics[0] == *rhs,
                        None => head.generics.is_empty(),
                    }
            })?;
            let (_, pinned) = head
                .associated_types
                .iter()
                .find(|(name, _)| name.as_str() == "Output")?;
            return Some(pinned.clone());
        }
        crate::ops::operator_output(self.db, interface, lhs, rhs)
    }

    /// The non-null part of a type: `Null` drops from unions (an all-null
    /// type leaves `never`).
    /// A `?.` link's base: null peels off (marking the enclosing chain
    /// frame so the BOUNDARY re-unions it) and the link proceeds on the
    /// non-null part - TS short-circuit semantics.
    fn peel_chain_null(&mut self, ty: &Ty) -> Ty {
        let resolved = self.table.resolve_completely(ty);
        let nonnull = self.remove_null(&resolved);
        if nonnull != resolved
            && let Some(top) = self.chain_nullable.last_mut()
        {
            *top = true;
        }
        nonnull
    }

    fn remove_null(&mut self, ty: &Ty) -> Ty {
        // Null-peeling is a STRUCTURE demand (r-a resolves before every
        // such match): an aliased nullable answers as its union.
        let resolved = self.structurally_resolve(ty);
        match resolved.kind() {
            TyKind::Null { .. } => Ty::never(),
            TyKind::Union(members, _) => {
                let non_null: Vec<Ty> = members
                    .iter()
                    .filter(|member| !matches!(member.kind(), TyKind::Null { .. }))
                    .cloned()
                    .collect();
                // Filtering never REWRITES the survivors (the
                // `subtract_narrow` rule): the structural constructor
                // keeps each member's identity - freshness included -
                // so a fresh literal crossing a null test still widens
                // at its eventual binding.
                syntactic_union(&non_null)
            }
            _ => resolved,
        }
    }

    /// `a ?? b`: given the already-unwrapped lhs, the canonical-unwrap fast
    /// paths (TIR's rule) - `rhs <: inner` keeps the unwrapped lhs, `inner
    /// <: rhs` keeps rhs - else the freshness-preserving join.
    fn null_coalesce(&mut self, inner: Ty, rhs: &Ty) -> Ty {
        let rhs = self.table.resolve_completely(rhs);
        let ground = |ty: &Ty| {
            !ty.has_infer() && !ty.has_error() && !matches!(ty.kind(), TyKind::Never { .. })
        };
        if ground(&inner) && ground(&rhs) {
            if is_subtype_interned(&rhs, &inner, &self.facts) {
                return inner;
            }
            if is_subtype_interned(&inner, &rhs, &self.facts) {
                return rhs;
            }
        }
        self.join(&[inner, rhs])
    }

    /// Call typing: direct calls to resolved functions instantiate the
    /// signature (explicit turbofish args, else fresh variables per generic
    /// param - the equality regime resolves them from argument bounds);
    /// calls through function-typed VALUES check against the value's type.
    /// Two argument passes: non-lambda args first, so lambda signatures can
    /// be deduced from already-resolved param types (S9).
    fn infer_call(
        &mut self,
        body: &ExprBody,
        call: ExprId,
        callee: ExprId,
        args: &[baml_compiler2_ast::CallArg],
    ) -> Ty {
        let (callee_fn_ty, bound_receiver) = self.infer_callee(body, call, callee);
        self.check_call_args(body, call, callee, &callee_fn_ty, bound_receiver, args)
    }

    /// The argument/return half of a call, shared by ordinary calls and
    /// `?.()` (which strips the callee's null first): labeled args match
    /// by name, lambdas check in the second pass, the callee's throws
    /// join the effect channel.
    fn check_call_args(
        &mut self,
        body: &ExprBody,
        call: ExprId,
        callee: ExprId,
        callee_fn_ty: &Ty,
        bound_receiver: bool,
        args: &[baml_compiler2_ast::CallArg],
    ) -> Ty {
        // The call is a STRUCTURE demand on the callee (rustc's
        // `check_call` structurally resolves before matching
        // callability): `inc2` holding a still-unsolved `?T` bounded
        // by a function type forces here, exactly as a method
        // receiver would.
        let callee_fn_ty = &self.structurally_resolve(callee_fn_ty);
        let TyKind::Function {
            params,
            ret,
            throws,
            ..
        } = callee_fn_ty.kind()
        else {
            // Not callable (or not yet typed): visit the args, sentinel out.
            // A callee already carrying error/vars is a cascade, not a
            // second finding - EXCEPT a union whose error members are mere
            // recovery arms: the concrete remainder (`int` of `int |
            // <unresolved>`) is still provably not callable, and that
            // finding stands on its own (E0006).
            let concrete_uncallable_union = || match callee_fn_ty.kind() {
                TyKind::Union(members, _) => {
                    let clean: Vec<Ty> = members
                        .iter()
                        .filter(|member| !member.has_error())
                        .cloned()
                        .collect();
                    (!clean.is_empty()
                        && clean.iter().all(|member| {
                            !member.has_infer()
                                && !matches!(
                                    member.kind(),
                                    TyKind::Function { .. } | TyKind::Unknown { .. }
                                )
                        }))
                    .then(|| syntactic_union(&clean))
                }
                _ => None,
            };
            if !callee_fn_ty.has_error()
                && !callee_fn_ty.has_infer()
                && !matches!(callee_fn_ty.kind(), TyKind::Unknown { .. })
            {
                self.pending_diags.push(PendingDiag::NotCallable {
                    expr: callee,
                    ty: callee_fn_ty.clone(),
                });
            } else if let Some(clean) = concrete_uncallable_union() {
                self.pending_diags.push(PendingDiag::NotCallable {
                    expr: callee,
                    ty: clean,
                });
            }
            for arg in args {
                self.infer_expr(body, arg.expr, &Expectation::None);
            }
            return Ty::error();
        };
        let callee_throws = throws.clone();
        self.record_throw(callee, &callee_throws);
        // A bound method call: the receiver already fills the `self` slot,
        // so written arguments match against the remaining parameters.
        let params: Vec<baml_type::interned::FunctionParam> = params
            .iter()
            .skip(usize::from(bound_receiver))
            .cloned()
            .collect();
        let ret = ret.clone();
        // The matching is decided ONCE, checked below and recorded as the
        // call plan's bindings: a LABELED argument selects its parameter
        // by NAME (`options(cancel = tok)` checks against `cancel`, not
        // whatever sits at its position); unlabeled arguments fill
        // positionally; an unmatched label falls back to position. The
        // label/arity diagnostics are S17's. `$id = ...` is the runtime-id
        // side channel (TIR's rule: not a parameter binding) - it never
        // matches a parameter; its own LocalId check lives at the capture.
        let matched: Vec<Option<usize>> = args
            .iter()
            .enumerate()
            .map(|(index, arg)| match &arg.label {
                Some(label) if label.as_str() == "$id" => None,
                Some(label) => params
                    .iter()
                    .position(|param| param.name.as_ref() == Some(label))
                    .or_else(|| params.get(index).map(|_| index)),
                None => params.get(index).map(|_| index),
            })
            .collect();
        // An UNKNOWN label's positional fallback types the value but
        // never FILLS the parameter: `search(q = "cats")` still owes
        // `query`, so both the unknown-label and missing-required
        // diagnostics report.
        let label_fallback: Vec<bool> = args
            .iter()
            .map(|arg| {
                arg.label.as_ref().is_some_and(|label| {
                    label.as_str() != "$id"
                        && !params
                            .iter()
                            .any(|param| param.name.as_ref() == Some(label))
                })
            })
            .collect();
        let is_lambda_arg =
            |arg: &baml_compiler2_ast::CallArg| matches!(body.exprs[arg.expr], Expr::Lambda(_));
        for pass in 0..2 {
            for (index, arg) in args.iter().enumerate() {
                if (pass == 0) == is_lambda_arg(arg) {
                    continue;
                }
                match matched[index].map(|param_index| params[param_index].ty.clone()) {
                    Some(param_ty) => {
                        self.check_expr(body, arg.expr, &param_ty);
                    }
                    None => {
                        // Extra argument: the arity diagnostic is S17's.
                        self.infer_expr(body, arg.expr, &Expectation::None);
                    }
                }
            }
        }
        // Parameter-ordered bindings: provided slots from the matching,
        // omitted OPTIONAL slots as defaults (required gaps get no entry;
        // arity is S17's).
        let mut slots: Vec<Option<ExprId>> = vec![None; params.len()];
        let mut runtime_id = None;
        for (index, arg) in args.iter().enumerate() {
            if arg
                .label
                .as_ref()
                .is_some_and(|label| label.as_str() == "$id")
            {
                if runtime_id.is_some() {
                    self.pending_diags
                        .push(PendingDiag::DuplicateRuntimeIdArg { at: arg.expr });
                } else {
                    // The side channel accepts exactly `boundary.LocalId`.
                    let got = self
                        .result
                        .type_of_expr
                        .get(&arg.expr)
                        .cloned()
                        .unwrap_or_else(|| self.infer_expr(body, arg.expr, &Expectation::None));
                    let local_id = Ty::intern(TyKind::Class(
                        baml_type::QualifiedTypeName::new(
                            baml_type::Name::new(baml_builtins2::PACKAGE_BOUNDARY),
                            vec![],
                            baml_type::Name::new("LocalId"),
                        ),
                        Vec::new().into(),
                        TyAttr::default(),
                    ));
                    if !self.sub(&got, &local_id) {
                        self.pending_diags
                            .push(PendingDiag::RuntimeIdArgMismatch { at: arg.expr, got });
                    }
                }
                runtime_id = Some(arg.expr);
                continue;
            }
            if runtime_id.is_some() {
                self.pending_diags
                    .push(PendingDiag::RuntimeIdArgNotLast { at: arg.expr });
            }
            if let Some(param_index) = matched[index]
                && slots[param_index].is_none()
                && !label_fallback[index]
            {
                slots[param_index] = Some(arg.expr);
            }
        }
        // Arity + label diagnostics (S17): counts exclude the `$id`
        // side channel; an over-supplied call reports against the full
        // parameter count, an unfilled REQUIRED slot against the
        // required count.
        {
            let provided = args
                .iter()
                .filter(|arg| arg.label.as_ref().map(smol_str::SmolStr::as_str) != Some("$id"))
                .count();
            let required = params
                .iter()
                .filter(|param| param.mode == baml_type::FunctionParamMode::Required)
                .count();
            if provided > params.len() {
                self.pending_diags.push(PendingDiag::ArgCountMismatch {
                    expr: call,
                    expected: params.len(),
                    got: provided,
                });
            } else {
                let saw_named = args
                    .iter()
                    .any(|arg| arg.label.as_ref().is_some_and(|l| l.as_str() != "$id"));
                let unfilled: Vec<usize> = (0..params.len())
                    .filter(|&param_index| {
                        params[param_index].mode == baml_type::FunctionParamMode::Required
                            && slots[param_index].is_none()
                    })
                    .collect();
                if !unfilled.is_empty() {
                    if saw_named {
                        // A named call reports each missing parameter BY
                        // NAME (the count would misread as positional).
                        for param_index in unfilled {
                            if let Some(name) = params[param_index].name.clone() {
                                self.pending_diags
                                    .push(PendingDiag::MissingNamedArg { expr: call, name });
                            } else {
                                self.pending_diags.push(PendingDiag::ArgCountMismatch {
                                    expr: call,
                                    expected: required,
                                    got: provided,
                                });
                                break;
                            }
                        }
                    } else {
                        self.pending_diags.push(PendingDiag::ArgCountMismatch {
                            expr: call,
                            expected: required,
                            got: provided,
                        });
                    }
                }
            }
            // Argument-form rules (TIR's family): a positional
            // argument after a named one; a DEFAULTED parameter
            // supplied positionally (by-name only); a repeated name.
            let mut seen_named = false;
            let mut seen_labels: Vec<&baml_type::Name> = Vec::new();
            for (index, arg) in args.iter().enumerate() {
                match &arg.label {
                    Some(label) if label.as_str() == "$id" => {}
                    Some(label) => {
                        if seen_labels.contains(&label) {
                            self.pending_diags.push(PendingDiag::DuplicateNamedArg {
                                expr: arg.expr,
                                name: label.clone(),
                            });
                        }
                        seen_labels.push(label);
                        seen_named = true;
                    }
                    None if seen_named => {
                        self.pending_diags
                            .push(PendingDiag::PositionalAfterNamed { expr: arg.expr });
                    }
                    None => {
                        if let Some(param_index) = matched[index]
                            && params[param_index].mode == baml_type::FunctionParamMode::Optional
                            && let Some(name) = params[param_index].name.clone()
                        {
                            self.pending_diags
                                .push(PendingDiag::PositionalDefaultedArg {
                                    expr: arg.expr,
                                    name,
                                });
                        }
                    }
                }
            }
            for arg in args {
                if let Some(label) = &arg.label
                    && label.as_str() != "$id"
                    && !params
                        .iter()
                        .any(|param| param.name.as_ref() == Some(label))
                {
                    self.pending_diags.push(PendingDiag::UnknownNamedArg {
                        expr: call,
                        name: label.clone(),
                    });
                }
            }
        }
        let bindings: Vec<ParamBinding> = slots
            .iter()
            .enumerate()
            .filter_map(|(param_index, slot)| match slot {
                Some(arg) => Some(ParamBinding::Provided {
                    param_index,
                    arg: *arg,
                }),
                None => match &params[param_index] {
                    param if param.mode == baml_type::FunctionParamMode::Optional => param
                        .name
                        .clone()
                        .map(|param_name| ParamBinding::OmittedDefault {
                            param_index,
                            param_name,
                        }),
                    _ => None,
                },
            })
            .collect();
        let plan = self.result.call_plans.entry(call).or_default();
        plan.bindings = bindings;
        plan.runtime_id = runtime_id;
        ret
    }

    /// The callee's (instantiated) function type, plus whether its first
    /// parameter is already bound to a receiver (`xs.push(1)` checks `1`
    /// against `item`, not `self`). Direct function references and
    /// type-qualified method paths instantiate here because their
    /// instantiation reads the CALL site's turbofish, which a bare value
    /// does not have; bound method callees resolve through
    /// `method_resolution`; everything else is whatever the expression
    /// infers to.
    fn infer_callee(&mut self, body: &ExprBody, call: ExprId, callee: ExprId) -> (Ty, bool) {
        // An INTERFACE-qualified direct call
        // (`IqDescribable.describe(t)`): turbofish-aware instantiation
        // with bounds registered - `Self`'s implements-bound becomes an
        // obligation the argument discharges.
        if let Expr::Path(segments) = &body.exprs[callee]
            && segments.len() >= 2
            && !self.path_resolves_locally(callee)
        {
            let segments = segments.clone();
            let (prefix, member) = segments.split_at(segments.len() - 1);
            if let Some(fn_ty) = self.interface_static_value(
                prefix,
                &member[0],
                OwnArgs::Call(call),
                call,
                Some(callee),
            ) {
                self.result.type_of_expr.insert(callee, fn_ty.clone());
                return (fn_ty, false);
            }
        }
        // `default.m(..)` inside an `implements` block: delegation to
        // the interface's DEFAULT implementation - a scoped receiver
        // like `self`, resolved from the body owner, never the value
        // namespace. A local named `default` shadows it.
        if let Expr::Path(segments) = &body.exprs[callee]
            && let [root, member] = segments.as_slice()
            && root.as_str() == "default"
            && !self.path_resolves_locally(callee)
        {
            let member = member.clone();
            if let Some(interface_member) = self.default_member(&member) {
                let (ty, bound) = self.interface_member_callee(interface_member, call);
                self.result.type_of_expr.insert(callee, ty.clone());
                return (ty, bound);
            }
        }
        if let Expr::Path(segments) = &body.exprs[callee]
            && !self.path_resolves_locally(callee)
            && !segments
                .first()
                .is_some_and(|root| self.template_param_root(root))
        {
            // A path that names a function is a direct call.
            if let Some(baml_compiler2_hir::contributions::Definition::Function(function)) =
                self.lower.resolve_value(segments)
            {
                let signature = function_signature(self.db, function);
                let callee_name = baml_compiler2_ppir::item_data::function_data(self.db, function)
                    .name
                    .clone();
                let instantiation =
                    self.instantiation_args(call, &signature.generic_params, Some(&callee_name));
                self.register_call_bounds(function, &instantiation, call);
                self.write_call_type_args(call, &instantiation, 0);
                let fn_ty = function_value_ty(signature, &instantiation);
                self.result.type_of_expr.insert(callee, fn_ty.clone());
                self.write_member_resolution(callee, MemberResolution::Free { func: function });
                return (fn_ty, false);
            }
            if let Some((function_id, function)) = self.lower.resolve_compiled_function(segments) {
                let instantiation =
                    self.instantiation_args(call, &function.generic_params, Some(&function.name));
                self.register_compiled_bounds(&function.generic_bounds, &instantiation, 0, call);
                self.write_call_type_args(call, &instantiation, 0);
                let fn_ty = compiled_function_value_ty(&function, &instantiation);
                self.result.type_of_expr.insert(callee, fn_ty.clone());
                self.write_member_resolution(
                    callee,
                    MemberResolution::CompiledFree {
                        function: function_id,
                    },
                );
                return (fn_ty, false);
            }
            // A type-qualified method path (`Array.filled(3, 0)`,
            // `baml.Array.generate(...)`): statics call directly, and the
            // UFCS spelling of an instance method takes the receiver as
            // its written first argument - either way the declared
            // parameter list matches the written arguments (no bound
            // receiver). Class generics have no receiver to pin them, so
            // they instantiate fresh alongside the method's own.
            if segments.len() >= 2 {
                let (prefix, member) = segments.split_at(segments.len() - 1);
                if let Some(fn_ty) = self.class_static_value(
                    prefix,
                    &member[0],
                    OwnArgs::Call(call),
                    call,
                    Some(callee),
                ) {
                    self.result.type_of_expr.insert(callee, fn_ty.clone());
                    return (fn_ty, false);
                }
            }
        }
        // A bound method callee, in either AST spelling: `expr.name(..)`
        // parses as MemberAccess, `local.name(..)` as a multi-segment Path
        // (the AST cannot split paths before name resolution).
        match &body.exprs[callee] {
            Expr::MemberAccess { base, member } => {
                let member = member.clone();
                // TYPE-qualified member spelling: when the receiver
                // carries written type args (`Box<int>.from_json(j)`),
                // lowering keeps the callee a MemberAccess and hoists
                // the `<int>` onto the CALL's type-arg channel
                // (BEP-039) - so this is the path ladder's static road
                // in member clothing. One ladder, two spellings.
                if let Expr::Path(segments) = &body.exprs[*base]
                    && !self.path_resolves_locally(*base)
                {
                    let base_expr = *base;
                    let segments = segments.clone();
                    if let Some(fn_ty) = self
                        .type_qualified_member_callee(call, base_expr, &segments, &member, callee)
                    {
                        self.result.type_of_expr.insert(callee, fn_ty.clone());
                        return (fn_ty, false);
                    }
                }
                if self.check_runtime_id_member(body, callee, *base, &member) {
                    self.result.type_of_expr.insert(callee, Ty::error());
                    return (Ty::error(), false);
                }
                let receiver = self.infer_expr(body, *base, &Expectation::None);
                let (ty, bound, resolution, desugar) =
                    self.member_callee(call, callee, &receiver, &member);
                self.result.type_of_expr.insert(callee, ty.clone());
                if desugar {
                    self.result.desugared_callees.insert(callee);
                }
                if let Some(resolution) = resolution {
                    self.write_member_resolution(callee, resolution);
                }
                return (ty, bound);
            }
            // `x?.method(..)`: the link peels the receiver's null (the
            // chain boundary re-unions it) and dispatches on the rest.
            Expr::OptionalMemberAccess { base, member } => {
                let member = member.clone();
                if self.check_runtime_id_member(body, callee, *base, &member) {
                    self.result.type_of_expr.insert(callee, Ty::error());
                    return (Ty::error(), false);
                }
                let receiver = self.infer_expr(body, *base, &Expectation::None);
                let nonnull = self.peel_chain_null(&receiver);
                let (ty, bound, resolution, desugar) =
                    self.member_callee(call, callee, &nonnull, &member);
                self.result.type_of_expr.insert(callee, ty.clone());
                if desugar {
                    self.result.desugared_callees.insert(callee);
                }
                if let Some(resolution) = resolution {
                    self.write_member_resolution(callee, resolution);
                }
                return (ty, bound);
            }
            Expr::Path(segments)
                if segments.len() >= 2
                    && (self.path_resolves_locally(callee)
                        || segments
                            .first()
                            .is_some_and(|root| self.template_param_root(root))) =>
            {
                let segments = segments.clone();
                let root = if self.path_resolves_locally(callee) {
                    self.infer_path(callee)
                } else {
                    // Guard admitted a tagged-template body param root.
                    segments
                        .first()
                        .and_then(|root| self.template_param_ty(root))
                        .unwrap_or_else(Ty::error)
                };
                // Intermediate segments walk the ladder; the FINAL member
                // resolves as the callee, and its step completes the
                // recorded path.
                let (receiver, mut steps) =
                    self.walk_path_members(callee, root, &segments[1..segments.len() - 1]);
                let member = segments.last().expect("checked len");
                let (ty, bound, resolution, desugar) =
                    self.member_callee(call, callee, &receiver, member);
                self.result.type_of_expr.insert(callee, ty.clone());
                if desugar {
                    self.result.desugared_callees.insert(callee);
                }
                steps.push(ResolvedPathSegment {
                    ty: ty.clone(),
                    resolution,
                });
                self.write_resolved_path(callee, steps);
                return (ty, bound);
            }
            _ => {}
        }
        (self.infer_expr(body, callee, &Expectation::None), false)
    }

    /// `receiver.member` in callee position: a method (instantiated - the
    /// receiver pins the class generics, the call site's turbofish or
    /// fresh variables fill the method's own; bound iff it takes `self`),
    /// or a field holding a function value.
    fn member_callee(
        &mut self,
        call: ExprId,
        // The member-access expression itself - the E0121/E0122 anchor
        // (rustc anchors ambiguity at the method segment, not the whole
        // call with its arguments).
        member_expr: ExprId,
        receiver: &Ty,
        member: &baml_type::Name,
    ) -> (Ty, bool, Option<MemberResolution<'db>>, bool) {
        let resolved = self.structurally_resolve(receiver);
        // Callee position on a UNION: the member resolves through the
        // single interface every arm shares (TIR's rule - the union as
        // the intersection existential), `Self` bound to the union. No
        // shared declarer FALLS THROUGH - the operator-style sugars at
        // the bottom of the ladder are TOTAL and apply to the WHOLE
        // union (`(int | null).to_string()` is `string.from<int | null>`);
        // a full miss then reports "no common interface" instead of the
        // bare "no member".
        if let TyKind::Union(union_members, _) = resolved.kind() {
            let union_members = union_members.to_vec();
            match crate::method_resolution::lookup_union_member(
                self.db,
                &self.facts,
                &resolved,
                &union_members,
                member,
            ) {
                crate::method_resolution::UnionMemberLookup::Found(interface_member) => {
                    let resolution = self.declarer_resolution(&interface_member.declarer, member);
                    let (ty, bound) = self.interface_member_callee(interface_member, call);
                    return (ty, bound, resolution, false);
                }
                crate::method_resolution::UnionMemberLookup::Ambiguous { sources, is_field } => {
                    if self.member_probe_depth == 0 {
                        self.pending_diags.push(PendingDiag::AmbiguousMember {
                            expr: member_expr,
                            base: resolved.clone(),
                            member: member.clone(),
                            sources,
                            is_field,
                        });
                    }
                    return (Ty::error(), false, None, false);
                }
                crate::method_resolution::UnionMemberLookup::SelfRestricted {
                    interface,
                    position,
                } => {
                    if self.member_probe_depth == 0 {
                        self.pending_diags.push(PendingDiag::SelfRestrictedMember {
                            expr: member_expr,
                            interface,
                            member: member.clone(),
                            position,
                        });
                    }
                    return (Ty::error(), false, None, false);
                }
                crate::method_resolution::UnionMemberLookup::ClassFieldJoin(field_ty) => {
                    // The agreed class field types the callee; boundness
                    // is a plain value read (no receiver binding).
                    return (field_ty, false, None, false);
                }
                crate::method_resolution::UnionMemberLookup::NoCommonInterface => {}
            }
        }
        let candidate =
            crate::method_resolution::lookup_method(self.db, &self.facts, &resolved, member);
        // An own method does not arbitrate an interface clash: two
        // implemented interfaces declaring `member` still make the
        // unqualified access ambiguous (E0121), own method or not.
        if candidate.is_some()
            && let Some((sources, is_field)) = crate::method_resolution::concrete_member_ambiguity(
                self.db,
                &self.facts,
                &resolved,
                member,
            )
        {
            if self.member_probe_depth == 0 {
                self.pending_diags.push(PendingDiag::AmbiguousMember {
                    expr: member_expr,
                    base: resolved.clone(),
                    member: member.clone(),
                    sources,
                    is_field,
                });
            }
            return (Ty::error(), false, None, false);
        }
        let Some(candidate) = candidate else {
            // Interface members (I3): existential and bounded-var
            // receivers dispatch virtually; methods bind their receiver.
            // A receiver still CARRYING inference variables probes the
            // impls by unification instead (r-a's snapshot probe; the
            // ground registry fails safe on such types).
            match crate::method_resolution::lookup_interface_member(
                self.db,
                &self.facts,
                &resolved,
                member,
            ) {
                crate::method_resolution::InterfaceMemberLookup::Found(interface_member) => {
                    let resolution = self.declarer_resolution(&interface_member.declarer, member);
                    let (ty, bound) = self.interface_member_callee(interface_member, call);
                    return (ty, bound, resolution, false);
                }
                crate::method_resolution::InterfaceMemberLookup::Ambiguous {
                    sources,
                    is_field,
                } => {
                    if self.member_probe_depth == 0 {
                        self.pending_diags.push(PendingDiag::AmbiguousMember {
                            expr: member_expr,
                            base: resolved.clone(),
                            member: member.clone(),
                            sources,
                            is_field,
                        });
                    }
                    return (Ty::error(), false, None, false);
                }
                crate::method_resolution::InterfaceMemberLookup::FieldRequiresProjection {
                    interface,
                } => {
                    if self.member_probe_depth == 0 {
                        self.pending_diags
                            .push(PendingDiag::FieldRequiresProjection {
                                expr: member_expr,
                                base: resolved.clone(),
                                member: member.clone(),
                                interface,
                            });
                    }
                    return (
                        Ty::intern(TyKind::Unknown {
                            attr: TyAttr::default(),
                        }),
                        false,
                        None,
                        false,
                    );
                }
                crate::method_resolution::InterfaceMemberLookup::SelfRestricted {
                    interface,
                    position,
                } => {
                    if self.member_probe_depth == 0 {
                        self.pending_diags.push(PendingDiag::SelfRestrictedMember {
                            expr: member_expr,
                            interface,
                            member: member.clone(),
                            position,
                        });
                    }
                    return (Ty::error(), false, None, false);
                }
                crate::method_resolution::InterfaceMemberLookup::NotFound => {}
            }
            // The var-carrying probe resolves through an impl candidate
            // whose block loc `ImplFacts` does not carry yet, so its
            // declarer would mislabel concrete dispatch as virtual -
            // no recorded entry rather than a wrong one (S16 follow-up:
            // thread the block loc through `ImplFacts`).
            if resolved.has_infer()
                && let Some(interface_member) = self.probe_impl_member(&resolved, member, call)
            {
                let (ty, bound) = self.interface_member_callee(interface_member, call);
                return (ty, bound, None, false);
            }
            self.member_probe_depth += 1;
            let (field, field_resolution) = self.field_access_resolved(call, &resolved, member);
            self.member_probe_depth -= 1;
            // `recv.to_json()` is language sugar for `baml.json.from(recv)`
            // (universal serialization; TIR's builder lowers the call the
            // same way). It is a FALLBACK tier: an `implements baml.ToJson`
            // method or a field named `to_json` resolves above. `bound`
            // makes the receiver fill the `value: T` slot.
            if field.has_error()
                && member.as_str() == "to_json"
                && let Some(fn_ty) = self.json_desugar_callee("from", resolved.clone())
            {
                // Desugar tiers record nothing: MIR keys the sugar on the
                // ABSENCE of a resolution (TIR's convention); the callee
                // gets a desugared_callees mark instead.
                return (fn_ty, true, None, true);
            }
            // `recv.to_string()` with no real `implements baml.ToString`
            // member is likewise sugar for `string.from(recv)` (TIR's
            // operator-style fallback; total, `throws never`, honoring
            // overrides via the runtime shim) - the same lang-item tier.
            if field.has_error()
                && member.as_str() == "to_string"
                && let Some(fn_ty) = self.string_from_callee(resolved.clone())
            {
                return (fn_ty, true, None, true);
            }
            // Every tier exhausted: the COMMITTED member failure reports
            // here (probes above stayed silent). A union base says "no
            // common interface" - each arm may well declare `member` via
            // different interfaces, so the bare "no member" would mislead.
            if field.has_error()
                && self.member_probe_depth == 0
                && !resolved.has_error()
                && !resolved.has_infer()
                && !matches!(resolved.kind(), TyKind::Unknown { .. })
            {
                if matches!(resolved.kind(), TyKind::Union(..)) {
                    self.pending_diags
                        .push(PendingDiag::UnionNoCommonInterface {
                            expr: call,
                            base: resolved.clone(),
                            member: member.clone(),
                        });
                } else {
                    self.pending_diags.push(PendingDiag::UnresolvedMember {
                        expr: call,
                        base: resolved.clone(),
                        member: member.clone(),
                    });
                }
            }
            return (field, false, field_resolution, false);
        };
        match candidate.target {
            crate::method_resolution::MethodCandidateTarget::Source { method, class } => {
                let signature = function_signature(self.db, method);
                let class_count = candidate.class_args.len();
                let own_params = signature.generic_params[class_count..].to_vec();
                let method_name = baml_compiler2_ppir::item_data::function_data(self.db, method)
                    .name
                    .clone();
                let mut instantiation = candidate.class_args;
                instantiation.extend(self.instantiation_args(
                    call,
                    &own_params,
                    Some(&method_name),
                ));
                self.register_call_bounds(method, &instantiation, call);
                self.write_call_type_args(call, &instantiation, class_count);
                let fn_ty = function_value_ty(signature, &instantiation);
                let bound = signature
                    .params
                    .first()
                    .is_some_and(|param| param.name.as_str() == "self");
                let resolution = if bound {
                    MemberResolution::BoundMethod {
                        class,
                        func: method,
                    }
                } else {
                    MemberResolution::UnboundMethod {
                        class,
                        func: method,
                    }
                };
                (fn_ty, bound, Some(resolution), false)
            }
            crate::method_resolution::MethodCandidateTarget::Compiled {
                class,
                class_generic_params,
                class_generic_bounds,
                method,
            } => {
                let class_count = class_generic_params.len();
                let mut instantiation = candidate.class_args;
                instantiation.extend(self.instantiation_args(
                    call,
                    &method.generic_params,
                    Some(&method.name),
                ));
                self.register_compiled_bounds(
                    &class_generic_bounds,
                    &instantiation,
                    usize::MAX,
                    call,
                );
                self.register_compiled_bounds(
                    &method.generic_bounds,
                    &instantiation,
                    class_count,
                    call,
                );
                self.write_call_type_args(call, &instantiation, class_count);
                let fn_ty = compiled_function_value_ty(&method, &instantiation);
                let bound = method
                    .params
                    .first()
                    .and_then(|param| param.name.as_ref())
                    .is_some_and(|name| name.as_str() == "self");
                let resolution = if bound {
                    MemberResolution::CompiledBoundMethod {
                        class,
                        method: method.name,
                    }
                } else {
                    MemberResolution::CompiledUnboundMethod {
                        class,
                        method: method.name,
                    }
                };
                (fn_ty, bound, Some(resolution), false)
            }
        }
    }

    /// The `default` receiver's meaning inside an `implements` block:
    /// the block's target interface (its written args and associated
    /// bindings lowered in the owner's frame) plus the IMPLEMENTOR as
    /// `Self` - the class's self type, or a free impl's for-target.
    /// `None` anywhere else; the caller falls back to ordinary
    /// resolution.
    fn default_receiver_target(&mut self) -> Option<(InterfaceRef, Ty)> {
        let function = self.body_owner?;
        let target =
            baml_compiler2_ppir::item_data::method_interface_target(self.db, function).as_ref()?;
        let target_ty = self.lower.lower_type_ref_at(
            &target.type_refs,
            target.target,
            crate::lower::TypePosition::ConstraintHead,
        );
        let TyKind::Interface(name, args, pins, _) = target_ty.kind() else {
            return None;
        };
        let self_ty = match baml_compiler2_ppir::item_data::method_owner(self.db, function) {
            Some(baml_compiler2_ppir::item_data::MethodOwner::Class(class)) => {
                crate::lower::class_self_ty(self.db, class)
            }
            Some(baml_compiler2_ppir::item_data::MethodOwner::FreeImpl(impl_loc)) => {
                let data = baml_compiler2_ppir::item_data::impl_block_data(self.db, impl_loc);
                match &data.subject {
                    baml_compiler2_ppir::item_data::ImplSubjectData::Free {
                        for_target, ..
                    } => self.lower.lower_type_ref(&data.type_refs, *for_target),
                    baml_compiler2_ppir::item_data::ImplSubjectData::InClass { class, .. } => {
                        crate::lower::class_self_ty(self.db, *class)
                    }
                }
            }
            _ => return None,
        };
        Some((
            InterfaceRef::new(name.clone(), (args.to_vec()).into(), pins.to_vec()),
            self_ty,
        ))
    }

    /// `default.member` - delegation to the interface's DEFAULT
    /// implementation (Java's `I.super.m()`): resolved on the enclosing
    /// implements block's interface with `Self` = the implementor,
    /// restricted to DEFAULT-BODIED members (a required signature has
    /// no body to delegate to).
    fn default_member(
        &mut self,
        member: &baml_type::Name,
    ) -> Option<crate::method_resolution::InterfaceMember<'db>> {
        let (target, self_ty) = self.default_receiver_target()?;
        let Some(baml_compiler2_hir::contributions::Definition::Interface(interface)) =
            self.facts.definition_of(&target.name)
        else {
            return None;
        };
        // The interface side must give the member MEANING: a declared
        // field (contract state, read through the interface view) or a
        // default-bodied method (delegation). Bodyless required methods
        // have nothing to delegate to.
        let data = baml_compiler2_ppir::item_data::interface_data(self.db, interface);
        let provided = data.fields.iter().any(|field| field.name == *member)
            || data.methods.iter().any(|&method| {
                baml_compiler2_ppir::item_data::function_has_body(self.db, method)
                    && baml_compiler2_ppir::item_data::function_data(self.db, method).name
                        == *member
            });
        if !provided {
            return None;
        }
        crate::method_resolution::member_on_interface(
            self.db,
            &self.facts,
            &target,
            &self_ty,
            member,
            false,
        )
    }

    /// An interface member in CALLEE position: a default method's OWN
    /// generics fill from the call site (turbofish or fresh vars) on
    /// top of the receiver-pinned interface prefix - the same
    /// owner-prefix + own-suffix instantiation the class-method path
    /// performs. Shared by the ground interface road and the probe.
    fn interface_member_callee(
        &mut self,
        interface_member: crate::method_resolution::InterfaceMember<'db>,
        call: ExprId,
    ) -> (Ty, bool) {
        if let Some(pending) = interface_member.pending_own {
            match pending {
                crate::method_resolution::PendingOwnGenerics::Source { method, prefix } => {
                    let signature = function_signature(self.db, method);
                    let own_offset = prefix.len();
                    let own_params = signature.generic_params[own_offset..].to_vec();
                    let method_name =
                        baml_compiler2_ppir::item_data::function_data(self.db, method)
                            .name
                            .clone();
                    let mut instantiation = prefix;
                    instantiation.extend(self.instantiation_args(
                        call,
                        &own_params,
                        Some(&method_name),
                    ));
                    self.register_call_bounds(method, &instantiation, call);
                    self.write_call_type_args(call, &instantiation, own_offset);
                    let fn_ty = function_value_ty(signature, &instantiation);
                    return (fn_ty, interface_member.is_method);
                }
                crate::method_resolution::PendingOwnGenerics::Compiled {
                    method,
                    mut bindings,
                } => {
                    let own_args =
                        self.instantiation_args(call, &method.generic_params, Some(&method.name));
                    bindings.extend(method.generic_params.iter().cloned().zip(own_args.clone()));
                    self.register_compiled_interface_method_bounds(
                        &method.generic_bounds,
                        &bindings,
                        call,
                    );
                    self.write_call_type_args(call, &own_args, 0);
                    let fn_ty = crate::impls::substitute_bindings(
                        &Ty::from_plain(&method.function_ty),
                        &bindings,
                    );
                    return (fn_ty, interface_member.is_method);
                }
            }
        }
        (interface_member.ty, interface_member.is_method)
    }

    /// The instantiated stdlib callee a json desugar lowers to
    /// (`baml.json.from` for `recv.to_json()`, `baml.json.to` for
    /// `Type.from_json(j)`), its one generic pinned to `target`. The
    /// r-a/rustc lang-item discipline (`infer_expr_await` resolving
    /// `LangItem::Future`): the sugar types as a call to the known
    /// library function, so parameter, result, and throws charge exactly
    /// as the runtime-lowered call's real signature says.
    fn json_desugar_callee(&mut self, name: &str, target: Ty) -> Option<Ty> {
        let segments = [
            baml_type::Name::new("baml"),
            baml_type::Name::new("json"),
            baml_type::Name::new(name),
        ];
        let Some(baml_compiler2_hir::contributions::Definition::Function(function)) =
            self.lower.resolve_value(&segments)
        else {
            return None;
        };
        let signature = function_signature(self.db, function);
        // The desugar targets are single-`<T>`-generic by contract;
        // anything else is stdlib drift this tier must not paper over.
        if signature.generic_params.len() != 1 {
            return None;
        }
        Some(function_value_ty(signature, &[target]))
    }

    /// The source-level `$id = value` form is exactly a call to
    /// `baml.id.set(value)`. Resolve its parameter and effect from the
    /// builtin declaration so the type checker and MIR lowering cannot drift.
    fn runtime_id_set_contract(&mut self) -> Option<(Ty, Ty)> {
        let segments = [
            baml_type::Name::new("baml"),
            baml_type::Name::new("id"),
            baml_type::Name::new("set"),
        ];
        let Some(baml_compiler2_hir::contributions::Definition::Function(function)) =
            self.lower.resolve_value(&segments)
        else {
            return None;
        };
        let signature = function_signature(self.db, function);
        let [param] = signature.params.as_slice() else {
            return None;
        };
        Some((param.ty.clone(), signature.throws.clone()))
    }

    /// The instantiated `string.from` callee backing the `to_string`
    /// operator-style fallback - a class STATIC (`baml.String.from<T>`),
    /// resolved through the same static-class correspondence written
    /// `string.from(..)` calls use, its `T` pinned to the receiver.
    fn string_from_callee(&mut self, target: Ty) -> Option<Ty> {
        let (class, _) =
            self.static_class_for(std::slice::from_ref(&baml_type::Name::new("string")))?;
        let method = baml_compiler2_ppir::item_data::class_data(self.db, class)
            .methods
            .iter()
            .copied()
            .find(|&method| {
                baml_compiler2_ppir::item_data::function_data(self.db, method)
                    .name
                    .as_str()
                    == "from"
            })?;
        let signature = function_signature(self.db, method);
        if signature.generic_params.len() != 1 {
            return None;
        }
        Some(function_value_ty(signature, &[target]))
    }

    /// The one home for value-position path typing (rust-analyzer's
    /// `infer/path.rs` shape): a local/parameter root followed by field
    /// accesses, or a package-level FUNCTION as a first-class value (`let c:
    /// (x: int) -> int throws never = inc;`), instantiated with fresh
    /// variables per generic param - only a call site's turbofish can spell
    /// arguments explicitly, and the expectation's bounds resolve them here.
    /// Constants and enum variants join as later slices land.
    fn resolve_value_path(&mut self, expr: ExprId, segments: &[baml_type::Name]) -> Ty {
        if segments.len() == 1 && segments[0].as_str() == "$id" {
            return Ty::string();
        }
        // `$id` reads as a string value, but is not a binding: any dotted
        // use reports the bind-it-first rewrite.
        if segments.len() > 1 && segments[0].as_str() == "$id" {
            self.pending_diags.push(PendingDiag::RuntimeIdMember {
                expr,
                member: segments[1].clone(),
            });
            return Ty::error();
        }
        if self.path_resolves_locally(expr) {
            // The root resolves through the semantic index; the remaining
            // segments are member accesses (the AST cannot split `b.v` into
            // base+member before name resolution).
            let root_ty = self.infer_path(expr);
            let (ty, steps) = self.walk_path_members(expr, root_ty, &segments[1..]);
            self.write_resolved_path(expr, steps);
            return ty;
        }
        // Tagged-template body params (`prompt`'s `role`/`ctx`): the
        // semantic index cannot register them (the tag is a cross-file
        // item), so they resolve here, shadowing package items exactly as
        // locals do, with the same root + member-access fold.
        if let Some(root_ty) = segments
            .first()
            .and_then(|root| self.template_param_ty(root))
        {
            let (ty, steps) = self.walk_path_members(expr, root_ty, &segments[1..]);
            self.write_resolved_path(expr, steps);
            return ty;
        }
        if let Some(baml_compiler2_hir::contributions::Definition::Function(function)) =
            self.lower.resolve_value(segments)
        {
            let signature = function_signature(self.db, function);
            let instantiation: Vec<Ty> = signature
                .generic_params
                .iter()
                .map(|param| self.fresh_generic_arg(param))
                .collect();
            self.write_member_resolution(expr, MemberResolution::Free { func: function });
            return function_value_ty(signature, &instantiation);
        }
        if let Some((function_id, function)) = self.lower.resolve_compiled_function(segments) {
            let instantiation: Vec<Ty> = function
                .generic_params
                .iter()
                .map(|param| self.fresh_generic_arg(param))
                .collect();
            self.register_compiled_bounds(&function.generic_bounds, &instantiation, 0, expr);
            self.write_member_resolution(
                expr,
                MemberResolution::CompiledFree {
                    function: function_id,
                },
            );
            return compiled_function_value_ty(&function, &instantiation);
        }
        // A type-qualified static as a VALUE (`let f = float.nan;`,
        // `Array.filled`): the same tier the call spellings use, with
        // the own suffix fresh (only a call site can spell turbofish).
        if segments.len() >= 2 {
            let (prefix, member) = segments.split_at(segments.len() - 1);
            if let Some(fn_ty) =
                self.class_static_value(prefix, &member[0], OwnArgs::Fresh, expr, Some(expr))
            {
                return fn_ty;
            }
        }
        // `default.m` as a VALUE: the delegation target as a bound
        // method value (same scoped-receiver rule as the callee road).
        if let [root, member] = segments
            && root.as_str() == "default"
            && let Some(interface_member) = self.default_member(&member.clone())
        {
            return self.interface_member_value(interface_member);
        }
        // An INTERFACE-qualified method as a VALUE (`Trait::method`):
        // the uniform item road with every frame slot fresh - the call
        // that consumes the value solves `Self` from its argument.
        if segments.len() >= 2 {
            let (prefix, member) = segments.split_at(segments.len() - 1);
            if let Some(fn_ty) =
                self.interface_static_value(prefix, &member[0], OwnArgs::Fresh, expr, Some(expr))
            {
                return fn_ty;
            }
        }
        // Enum VARIANT values (`Shape.Rectangle`): the variant's singleton
        // literal type - the same product the type-position path gives
        // (`lower_path` fallback 2; r-a resolves the value namespace to
        // the variant the same way).
        if let Some(ty) = self.enum_variant_value(segments, Some(expr)) {
            return ty;
        }
        // `Type.from_json(j)` is language sugar for `baml.json.to<Type>(j)`
        // - the decode counterpart of the `to_json` desugar, same lang-item
        // discipline. The prefix is any written type (class, enum, alias,
        // or an in-scope generic param) through the same resolution
        // annotations use; a real `from_json` static (an `implements
        // baml.FromJson` impl) already resolved above.
        if segments.len() >= 2
            && segments
                .last()
                .is_some_and(|segment| segment.as_str() == "from_json")
            && let Some(fn_ty) =
                self.from_json_desugar_value(&segments[..segments.len() - 1], OwnArgs::Fresh, None)
        {
            self.result.desugared_callees.insert(expr);
            return fn_ty;
        }
        // A name that RESOLVES to a definition kind this road doesn't
        // type (clients, top-level lets outside their tier) is not
        // unresolved - it stays the silent sentinel it always was.
        if self.lower.resolve_value(segments).is_none()
            && self.lower.resolve_compiled_function(segments).is_none()
            && !self.suppressed_unresolved.contains(&expr)
        {
            // When a proper prefix resolves (`baml.media.Image.missing`
            // has the valid type `baml.media.Image`), the segment AFTER
            // the longest valid prefix is what failed - report it alone,
            // not the whole dotted path (TIR's first-invalid-segment
            // rule). A path with no valid prefix reports in full.
            let failed = (1..segments.len()).rev().find_map(|cut| {
                let prefix = &segments[..cut];
                (self.lower.resolve_type_definition(prefix).is_some()
                    || self.lower.resolve_value(prefix).is_some()
                    || self.lower.resolve_compiled_function(prefix).is_some())
                .then(|| segments[cut].clone())
            });
            let name = failed.unwrap_or_else(|| {
                baml_type::Name::new(
                    segments
                        .iter()
                        .map(smol_str::SmolStr::as_str)
                        .collect::<Vec<_>>()
                        .join("."),
                )
            });
            self.pending_diags
                .push(PendingDiag::UnresolvedName { expr, name });
        }
        Ty::error()
    }

    fn own_instantiation(&mut self, own: OwnArgs, params: &[baml_type::ParamTy]) -> Vec<Ty> {
        match own {
            OwnArgs::Call(call) => self.instantiation_args(call, params, None),
            OwnArgs::Fresh => params
                .iter()
                .map(|param| self.fresh_generic_arg(param))
                .collect(),
        }
    }

    /// TIER: an interface-qualified static (`Trait.method`). Bounds
    /// register at the instantiation, whatever the spelling.
    fn interface_static_value(
        &mut self,
        prefix: &[baml_type::Name],
        member: &baml_type::Name,
        own: OwnArgs,
        anchor: ExprId,
        record_at: Option<ExprId>,
    ) -> Option<Ty> {
        let (interface, method) = self.interface_static_method(prefix, member)?;
        let signature = function_signature(self.db, method);
        let instantiation = self.own_instantiation(own, &signature.generic_params);
        self.register_call_bounds(method, &instantiation, anchor);
        if let OwnArgs::Call(call) = own {
            self.write_call_type_args(call, &instantiation, 0);
        }
        if let Some(record_at) = record_at {
            // The slot is what's statically known - interface plus member,
            // recorded uniformly for default and required methods (TIR's
            // contract; MIR keys the fn constant / virtual dispatch on it).
            self.write_member_resolution(
                record_at,
                MemberResolution::InterfaceVirtualMethod {
                    interface,
                    method: member.clone(),
                },
            );
        }
        Some(function_value_ty(signature, &instantiation))
    }

    /// TIER: a class-qualified static (`Array.filled`, `float.nan`,
    /// alias and keyword qualifiers included). The class prefix takes
    /// the qualifier's pinned args when it carries them (an alias
    /// expansion), else fresh vars; the own suffix follows `own`.
    /// `record_at` keys the recorded resolution (the callee expression
    /// for calls; recording lives here because the tier is where the
    /// method is known - r-a's path inference records its resolutions
    /// the same way, deep in the resolving fn).
    fn class_static_value(
        &mut self,
        prefix: &[baml_type::Name],
        member: &baml_type::Name,
        own: OwnArgs,
        anchor: ExprId,
        record_at: Option<ExprId>,
    ) -> Option<Ty> {
        let (class, pinned) = self.static_class_for(prefix)?;
        let method = baml_compiler2_ppir::item_data::class_data(self.db, class)
            .methods
            .iter()
            .copied()
            .find(|&method| {
                baml_compiler2_ppir::item_data::function_data(self.db, method).name == *member
            })?;
        let signature = function_signature(self.db, method);
        let frame = crate::lower::class_generic_frame(self.db, class);
        let mut instantiation: Vec<Ty> = match pinned {
            Some(args) => args,
            None => frame
                .iter()
                .map(|param| self.fresh_generic_arg(param))
                .collect(),
        };
        let own_params = signature.generic_params[frame.len()..].to_vec();
        instantiation.extend(self.own_instantiation(own, &own_params));
        self.register_call_bounds(method, &instantiation, anchor);
        if let OwnArgs::Call(call) = own {
            self.write_call_type_args(call, &instantiation, 0);
        }
        if let Some(record_at) = record_at {
            self.write_member_resolution(
                record_at,
                MemberResolution::UnboundMethod {
                    class,
                    func: method,
                },
            );
        }
        Some(function_value_ty(signature, &instantiation))
    }

    /// TIER: an enum variant value (`Shape.Rectangle`) - the singleton
    /// literal type, the same product the type position gives.
    /// `record_at` keys the recorded resolution (r-a's deep-recording
    /// discipline, as in `class_static_value`).
    fn enum_variant_value(
        &mut self,
        segments: &[baml_type::Name],
        record_at: Option<ExprId>,
    ) -> Option<Ty> {
        if segments.len() < 2 {
            return None;
        }
        let ty = self.lower.lower_type_path(segments);
        let TyKind::EnumVariant(qtn, variant, _) = ty.kind() else {
            return None;
        };
        if let Some(record_at) = record_at
            && let Some(baml_compiler2_hir::contributions::Definition::Enum(enum_loc)) =
                self.facts.definition_of(qtn)
        {
            self.write_member_resolution(
                record_at,
                MemberResolution::Variant {
                    enum_loc,
                    variant: variant.clone(),
                },
            );
        }
        Some(ty)
    }

    /// TIER: the `Type.from_json` decode desugar (`baml.json.to<Type>`).
    /// The target is the WRITTEN qualifier, nominal; only the turbofish
    /// class spelling needs the call channel's hoisted args instead. A
    /// real `from_json` static outranks this tier in every consumer.
    #[allow(clippy::wrong_self_convention)]
    fn from_json_desugar_value(
        &mut self,
        prefix: &[baml_type::Name],
        own: OwnArgs,
        record_base: Option<ExprId>,
    ) -> Option<Ty> {
        let written = self.lower.lower_type_path(prefix);
        let target = if !written.has_error() {
            written
        } else if let (OwnArgs::Call(call), Some((class, _))) = (own, self.static_class_for(prefix))
        {
            let frame = crate::lower::class_generic_frame(self.db, class);
            let args = self.instantiation_args(call, &frame, None);
            crate::lower::class_ty(crate::lower::class_qualified_name(self.db, class), args)
        } else {
            return None;
        };
        let fn_ty = self.json_desugar_callee("to", target.clone())?;
        if let Some(base) = record_base {
            self.result.type_of_expr.insert(base, target);
        }
        Some(fn_ty)
    }

    /// The `MemberAccess` spelling of a TYPE-QUALIFIED callee
    /// (`Box<int>.from_json(j)`, `Temp.from_json(j)` when lowering kept
    /// the FieldAccess): the same ladder the Path spelling walks -
    /// interface static, class static, then the `from_json` decode
    /// desugar with the class applied at the CALL's type-arg channel
    /// (where BEP-039 hoisted the receiver's written args). A real
    /// `from_json` static outranks the sugar, as in the path road.
    fn type_qualified_member_callee(
        &mut self,
        call: ExprId,
        base_expr: ExprId,
        prefix: &[baml_type::Name],
        member: &baml_type::Name,
        record_at: ExprId,
    ) -> Option<Ty> {
        let own = OwnArgs::Call(call);
        self.interface_static_value(prefix, member, own, call, Some(record_at))
            .or_else(|| self.class_static_value(prefix, member, own, call, Some(record_at)))
            .or_else(|| {
                let mut full = prefix.to_vec();
                full.push(member.clone());
                self.enum_variant_value(&full, Some(record_at))
            })
            .or_else(|| {
                (member.as_str() == "from_json")
                    .then(|| {
                        let fn_ty = self.from_json_desugar_value(prefix, own, Some(base_expr))?;
                        self.result.desugared_callees.insert(record_at);
                        Some(fn_ty)
                    })
                    .flatten()
            })
    }

    /// The class owning a TYPE-QUALIFIED static path's members: a
    /// resolved class path (`Array.filled`, `baml.Array.generate`), or a
    /// primitive KEYWORD head (`float.nan()`, `int.max_value()`) mapped
    /// through the language's builtin-class correspondence - the same
    /// rule the S11 receiver-class table applies to VALUES, applied to
    /// the written primitive name (`float`'s statics live on
    /// `class baml.Float`).
    /// A `None` args component means the qualifier wrote no
    /// instantiation (a bare class path - the call site fills fresh
    /// vars); `Some` pins it (an alias expansion carries its target's
    /// args).
    fn static_class_for(
        &self,
        prefix: &[baml_type::Name],
    ) -> Option<(baml_compiler2_hir::loc::ClassLoc<'db>, Option<Vec<Ty>>)> {
        use baml_compiler2_hir::contributions::Definition;
        if let Some(Definition::Class(class)) = self.lower.resolve_type_definition(prefix) {
            return Some((class, None));
        }
        // Anything else the qualifier can denote - a primitive or
        // media KEYWORD (annotation-grammar tokens, not paths), or an
        // ALIAS (chains included) - becomes the TYPE it names, and the
        // S11 receiver-class correspondence maps type to class, the
        // same single table instance receivers use. rust-analyzer
        // expands aliases at lowering so every consumer sees the
        // target; our lazy-alias design expands at the demand point,
        // and this is the static demand point.
        let ty = match prefix {
            [single] => match single.as_str() {
                "int" => Ty::intern(TyKind::Int {
                    attr: baml_type::TyAttr::default(),
                }),
                "bigint" => Ty::intern(TyKind::Bigint {
                    attr: baml_type::TyAttr::default(),
                }),
                "float" => Ty::intern(TyKind::Float {
                    attr: baml_type::TyAttr::default(),
                }),
                "string" => Ty::intern(TyKind::String {
                    attr: baml_type::TyAttr::default(),
                }),
                "bool" => Ty::intern(TyKind::Bool {
                    attr: baml_type::TyAttr::default(),
                }),
                "uint8array" => Ty::intern(TyKind::Uint8Array {
                    attr: baml_type::TyAttr::default(),
                }),
                "image" => Ty::intern(TyKind::Media(
                    baml_type::MediaKind::Image,
                    baml_type::TyAttr::default(),
                )),
                "audio" => Ty::intern(TyKind::Media(
                    baml_type::MediaKind::Audio,
                    baml_type::TyAttr::default(),
                )),
                "video" => Ty::intern(TyKind::Media(
                    baml_type::MediaKind::Video,
                    baml_type::TyAttr::default(),
                )),
                "pdf" => Ty::intern(TyKind::Media(
                    baml_type::MediaKind::Pdf,
                    baml_type::TyAttr::default(),
                )),
                _ => self.lower.lower_type_path(prefix),
            },
            _ => self.lower.lower_type_path(prefix),
        };
        if ty.has_error() {
            return None;
        }
        crate::method_resolution::receiver_class(&self.facts, &ty, 8)
            .map(|(class, args)| (class, Some(args)))
    }

    /// The method a TYPE-QUALIFIED interface path names
    /// (`IqDescribable.describe` - Rust's `Trait::method`, r-a's
    /// value-namespace trait path). Interface methods are ordinary
    /// items since the uniform restructure, so the ordinary signature
    /// road serves: `Self` instantiates fresh and its implements-bound
    /// rides along through `function_generic_bounds`, solved by the
    /// call's arguments.
    fn interface_static_method(
        &self,
        prefix: &[baml_type::Name],
        member: &baml_type::Name,
    ) -> Option<(
        baml_compiler2_hir::loc::InterfaceLoc<'db>,
        baml_compiler2_hir::loc::FunctionLoc<'db>,
    )> {
        use baml_compiler2_hir::contributions::Definition;
        let Some(Definition::Interface(interface)) = self.lower.resolve_type_definition(prefix)
        else {
            return None;
        };
        baml_compiler2_ppir::item_data::interface_data(self.db, interface)
            .methods
            .iter()
            .copied()
            .find(|&method| {
                baml_compiler2_ppir::item_data::function_data(self.db, method).name == *member
            })
            .map(|method| (interface, method))
    }

    /// Lambda typing (rust-analyzer's `deduce_closure_signature` shape).
    /// Written signature slots win; unannotated slots fill from the expected
    /// function type flowing down. An unannotated parameter with no
    /// expectation has no source of truth: the Error sentinel (TIR's
    /// `CannotInferLambdaParamType`; the diagnostic is S17's). An omitted
    /// `throws` stays the honest Error sentinel until S12 infers effects.
    fn infer_lambda(
        &mut self,
        body: &ExprBody,
        expr: ExprId,
        def: &baml_compiler2_ast::LambdaDef,
        expected: &Expectation,
    ) -> Ty {
        let signature = self.type_refs.lambda_signatures.get(&expr).cloned();
        let expected_fn = expected
            .only_has_type()
            .cloned()
            .map(|ty| self.structurally_resolve(&ty))
            .map(|ty| self.expand_alias_ty(&ty))
            .and_then(|ty| match ty.kind() {
                TyKind::Function {
                    params,
                    ret,
                    throws,
                    ..
                } => Some((params.clone(), ret.clone(), throws.clone())),
                _ => None,
            });

        let param_tys: Vec<Ty> = def
            .params
            .iter()
            .enumerate()
            .map(|(index, _)| {
                let annotated = signature
                    .as_ref()
                    .and_then(|sig| sig.params.get(index).copied().flatten());
                match annotated {
                    Some(type_ref) => self.lower_body_annotation(type_ref),
                    None => expected_fn
                        .as_ref()
                        .and_then(|(params, _, _)| params.get(index))
                        .map(|param| param.ty.clone())
                        .unwrap_or_else(Ty::error),
                }
            })
            .collect();

        let annotated_ret = signature
            .as_ref()
            .and_then(|sig| sig.return_type)
            .map(|type_ref| self.lower_body_annotation(type_ref));
        let ret_expectation =
            annotated_ret.or_else(|| expected_fn.as_ref().map(|(_, ret, _)| ret.clone()));

        let written_throws = signature
            .as_ref()
            .and_then(|sig| sig.throws)
            .map(|type_ref| self.lower_body_annotation(type_ref));

        // The scope the lambda opened, via the semantic index's SPAN-FREE
        // lambda join (keyed by the lambda expression itself). Registering
        // the deduced params there is what makes the body's parameter
        // references resolve.
        let lambda_scope = self
            .metadata_key(expr)
            .and_then(|key| self.index.lambda_scope(key));
        if let Some(scope) = lambda_scope {
            self.lambda_params.insert(scope, param_tys.clone());
        }

        // The body types in the owner's run and table, but under the
        // lambda's metadata scope (the semantic index keys its expressions
        // there), and its divergence is the lambda's, not the owner's.
        // The lambda's OWN effect channel: contributions inside the body
        // belong to the lambda, not the enclosing function - defining a
        // throwing lambda throws nothing; calling it does.
        self.throws_channels.push(Vec::new());
        let ret_ty = match def.body {
            Some(lambda_body) => {
                let saved_scope = self.current_scope;
                if lambda_scope.is_some() {
                    self.current_scope = lambda_scope;
                }
                let saved_diverges = std::mem::replace(&mut self.diverges, Diverges::Maybe);
                let ret_ty = match &ret_expectation {
                    // A void lambda DISCARDS its body's tail value, the
                    // same statement semantics void functions get (test
                    // bodies are synthesized `() -> void` lambdas).
                    Some(ret) if is_unit(ret) => {
                        self.infer_expr(body, lambda_body, &Expectation::None);
                        ret.clone()
                    }
                    Some(ret) if !ret.has_error() => {
                        self.check_expr(body, lambda_body, ret);
                        ret.clone()
                    }
                    _ => {
                        let body_ty = self.infer_expr(body, lambda_body, &Expectation::None);
                        self.widen_fresh(&body_ty)
                    }
                };
                self.diverges = saved_diverges;
                self.current_scope = saved_scope;
                ret_ty
            }
            None => ret_expectation.unwrap_or_else(Ty::error),
        };
        let channel = self.throws_channels.pop().expect("pushed above");
        // An OMITTED clause in a GROUND typed context inherits the
        // context's throws as its contract (TIR's rule: the lambda adopts
        // the expected surface, and its body checks against it - the
        // "local violation" road). Effect-param and open contexts skip:
        // there the channel BINDS the context instead.
        let contextual_throws = if written_throws.is_none() {
            expected_fn
                .as_ref()
                .map(|(_, _, throws)| self.structurally_resolve(throws))
                .filter(|throws| {
                    !throws.has_infer()
                        && !throws.has_error()
                        && !throws.has_typevar()
                        && !matches!(throws.kind(), TyKind::Unknown { .. })
                })
        } else {
            None
        };
        let written_throws = written_throws.or(contextual_throws);
        // A WRITTEN closed clause is the lambda's contract: its body's
        // contributions check against it exactly as a function's do
        // (open contributions judge at finalize).
        if let Some(declared) = &written_throws {
            let (_, open) = crate::lower::throws_clause_parts(declared);
            if !open && !declared.has_error() {
                for (at, contribution) in &channel {
                    if contribution.has_infer() || !self.sub(contribution, declared) {
                        self.pending_diags.push(PendingDiag::ThrowsViolation {
                            at: *at,
                            declared: declared.clone(),
                            extra: contribution.clone(),
                        });
                    }
                }
            }
        }
        let throws_ty = written_throws.unwrap_or_else(|| {
            if channel.is_empty() {
                Ty::never()
            } else {
                // The INFERRED surface keeps literal grain (spec rule;
                // TIR diverged by widening thrown literals here) - a
                // body-inferred `throw "neg"` surfaces as `throws
                // "neg"`, and the grain flows into effect params.
                let tys: Vec<Ty> = channel
                    .iter()
                    .map(|(_, ty)| self.table.resolve_completely(ty))
                    .collect();
                self.union_of(&tys)
            }
        });

        let params: Box<[baml_type::interned::FunctionParam]> = def
            .params
            .iter()
            .zip(&param_tys)
            .map(|(param, ty)| baml_type::interned::FunctionParam {
                name: Some(param.name.clone()),
                ty: ty.clone(),
                mode: if param.default.is_some() {
                    baml_type::FunctionParamMode::Optional
                } else {
                    baml_type::FunctionParamMode::Required
                },
            })
            .collect();
        Ty::intern(TyKind::Function {
            params,
            ret: ret_ty,
            throws: throws_ty,
            attr: TyAttr::default(),
        })
    }

    /// Registers one Implements obligation per declared bound of a
    /// callee's generic frame, with the call-site instantiation
    /// substituted through bound args (bounds may reference sibling
    /// params). Discharge stalls until the argument grounds -
    /// rust-analyzer's where-clause obligations.
    fn register_call_bounds(
        &mut self,
        function: baml_compiler2_hir::loc::FunctionLoc<'db>,
        instantiation: &[Ty],
        at: ExprId,
    ) {
        let bounds = crate::lower::function_generic_bounds(self.db, function);
        // The concreteness rule covers the function's OWN declared params
        // (turbofish or inferred - the user's type arguments). The frame
        // PREFIX is the receiver's business: `Self` legitimately binds an
        // existential for virtual dispatch, and class/interface args were
        // judged at the receiver's own annotation.
        let own_start = {
            let frame = crate::lower::function_generic_frame(self.db, function);
            let own = baml_compiler2_ppir::item_data::function_data(self.db, function)
                .generic_params
                .len();
            frame.len().saturating_sub(own)
        };
        for (param, param_bounds) in bounds {
            let Some(arg) = instantiation.get(param.index() as usize) else {
                continue;
            };
            for bound in param_bounds {
                let interface = baml_type::interned::InterfaceRef::new(
                    bound.name.clone(),
                    bound
                        .generics
                        .iter()
                        .map(|generic| substitute_params(generic, instantiation))
                        .collect(),
                    bound
                        .associated_types
                        .iter()
                        .map(|(name, ty)| (name.clone(), substitute_params(ty, instantiation)))
                        .collect(),
                );
                self.register_obligation(obligations::Obligation::Implements {
                    ty: arg.clone(),
                    interface,
                    at,
                    not_concrete_rejects: (param.index() as usize) >= own_start,
                });
            }
        }
    }

    fn register_compiled_bounds(
        &mut self,
        bounds: &baml_package_interface::GenericBounds,
        instantiation: &[Ty],
        own_start: usize,
        at: ExprId,
    ) {
        for (param, param_bounds) in bounds {
            let Some(arg) = instantiation.get(param.index() as usize) else {
                continue;
            };
            for bound in param_bounds {
                let bound = InterfaceRef::from_constraint(bound);
                let interface = InterfaceRef::new(
                    bound.name,
                    bound
                        .generics
                        .iter()
                        .map(|generic| substitute_params(generic, instantiation))
                        .collect(),
                    bound
                        .associated_types
                        .iter()
                        .map(|(name, ty)| (name.clone(), substitute_params(ty, instantiation)))
                        .collect(),
                );
                self.register_obligation(obligations::Obligation::Implements {
                    ty: arg.clone(),
                    interface,
                    at,
                    not_concrete_rejects: (param.index() as usize) >= own_start,
                });
            }
        }
    }

    fn register_compiled_interface_method_bounds(
        &mut self,
        bounds: &baml_package_interface::GenericBounds,
        bindings: &rustc_hash::FxHashMap<baml_type::ParamTy, Ty>,
        at: ExprId,
    ) {
        for (param, param_bounds) in bounds {
            let Some(arg) = bindings.get(param) else {
                continue;
            };
            for bound in param_bounds {
                let bound = InterfaceRef::from_constraint(bound);
                let interface = InterfaceRef::new(
                    bound.name,
                    bound
                        .generics
                        .iter()
                        .map(|generic| crate::impls::substitute_bindings(generic, bindings))
                        .collect(),
                    bound
                        .associated_types
                        .iter()
                        .map(|(name, ty)| {
                            (
                                name.clone(),
                                crate::impls::substitute_bindings(ty, bindings),
                            )
                        })
                        .collect(),
                );
                self.register_obligation(obligations::Obligation::Implements {
                    ty: arg.clone(),
                    interface,
                    at,
                    not_concrete_rejects: true,
                });
            }
        }
    }

    /// [`Self::register_call_bounds`] for a class instantiation: one
    /// Implements obligation per declared bound of the class's generic
    /// frame (`class Holder<T extends Named & Sized>` registers BOTH
    /// conjuncts for `Holder<X> { .. }`) - rustc's ADT well-formedness
    /// obligations at the constructor site.
    fn register_class_bounds(
        &mut self,
        class: baml_compiler2_hir::loc::ClassLoc<'db>,
        instantiation: &[Ty],
        at: ExprId,
    ) {
        let bounds = crate::lower::class_generic_bounds(self.db, class);
        for (param, param_bounds) in bounds {
            let Some(arg) = instantiation.get(param.index() as usize) else {
                continue;
            };
            for bound in param_bounds {
                let interface = baml_type::interned::InterfaceRef::new(
                    bound.name.clone(),
                    bound
                        .generics
                        .iter()
                        .map(|generic| substitute_params(generic, instantiation))
                        .collect(),
                    bound
                        .associated_types
                        .iter()
                        .map(|(name, ty)| (name.clone(), substitute_params(ty, instantiation)))
                        .collect(),
                );
                self.register_obligation(obligations::Obligation::Implements {
                    ty: arg.clone(),
                    interface,
                    at,
                    not_concrete_rejects: true,
                });
            }
        }
    }

    /// The instantiation vector for a generic item at a use site: explicit
    /// turbofish args (with `_` holes as fresh vars) where written, fresh
    /// variables everywhere else.
    fn instantiation_args(
        &mut self,
        site: ExprId,
        generic_params: &[baml_type::ParamTy],
        callee: Option<&baml_type::Name>,
    ) -> Vec<Ty> {
        let explicit: Vec<Ty> = self
            .type_refs
            .expr_type_args
            .get(&site)
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|&type_ref| {
                let lowered = self.lower.lower_type_ref(&self.type_refs.store, type_ref);
                self.reject_expr_position_holes(&lowered, site)
            })
            .collect();
        // Explicit turbofish counts against the WRITABLE params (synthetic
        // effect params are elaboration's, never spelled).
        if let Some(callee) = callee
            && !explicit.is_empty()
        {
            let expected = generic_params
                .iter()
                .filter(|param| !baml_type::is_synthetic_effect_param(param.name()))
                .count();
            if explicit.len() != expected {
                self.pending_diags.push(PendingDiag::WrongTypeArgArity {
                    expr: site,
                    callee: callee.clone(),
                    expected,
                    got: explicit.len(),
                });
            }
        }
        generic_params
            .iter()
            .enumerate()
            .map(|(index, param)| {
                explicit
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| self.fresh_generic_arg(param))
            })
            .collect()
    }

    /// A fresh variable for one generic param at a use site: synthetic
    /// effect params get EFFECT variables (unconstrained defaults to
    /// `never`, not Error - S12's defaulting rule).
    fn fresh_generic_arg(&mut self, param: &baml_type::ParamTy) -> Ty {
        if baml_type::is_synthetic_effect_param(param.name()) {
            self.table.new_effect_var_ty()
        } else {
            self.table.new_var_ty()
        }
    }

    /// Object-constructor typing: resolve the class, instantiate its
    /// generics (explicit args or fresh vars - `Box<_> { .. }` holes are
    /// vars too), check each written field against its substituted type.
    fn infer_object(
        &mut self,
        body: &ExprBody,
        object: ExprId,
        type_name: &baml_base::TypePath,
        fields: &[(baml_type::Name, ExprId)],
        spreads: &[baml_compiler2_ast::SpreadField],
    ) -> Ty {
        let Some(baml_compiler2_hir::contributions::Definition::Class(class)) =
            self.lower.resolve_type_definition(&type_name.0)
        else {
            // A WRITTEN constructor head that resolves nowhere reports
            // as an unresolved type with near-match suggestions
            // (`ValidationIssu { .. }` suggests the class).
            let segments: Vec<baml_type::Name> = type_name
                .0
                .iter()
                .map(|segment| baml_type::Name::new(segment.as_str()))
                .collect();
            if !segments.is_empty() {
                self.pending_diags.push(PendingDiag::UnresolvedCtor {
                    expr: object,
                    name: baml_type::Name::new(
                        segments
                            .iter()
                            .map(smol_str::SmolStr::as_str)
                            .collect::<Vec<_>>()
                            .join("."),
                    ),
                    suggestions: self.lower.type_suggestions(&segments),
                });
            }
            for (_, value) in fields {
                self.infer_expr(body, *value, &Expectation::None);
            }
            for spread in spreads {
                self.infer_expr(body, spread.expr, &Expectation::None);
            }
            return Ty::error();
        };
        let db = self.db;
        let generic_count = baml_compiler2_ppir::item_data::class_data(db, class)
            .generic_params
            .len();
        let generic_names: Vec<baml_type::ParamTy> = crate::lower::class_generic_frame(db, class);
        let instantiation = self.instantiation_args(object, &generic_names, None);
        let mut instantiation = instantiation;
        instantiation.truncate(generic_count);
        while instantiation.len() < generic_count {
            instantiation.push(self.table.new_var_ty());
        }
        self.register_class_bounds(class, &instantiation, object);
        let field_types = crate::lower::class_field_types(db, class);
        // Fresh (unwritten) instantiation slots that survive to finalize
        // unsolved are uninferrable - report instead of letting the bare
        // sentinel reach lowering. A PHANTOM param (no field mentions it)
        // stays silent: nothing could ever determine it.
        for (slot, arg) in instantiation.iter().enumerate() {
            let Some(param) = generic_names.get(slot) else {
                continue;
            };
            let mentioned = field_types
                .iter()
                .any(|(_, field_ty)| ty_mentions_param(field_ty, param));
            if mentioned && arg.has_infer() {
                self.pending_diags.push(PendingDiag::UninferredCtorParam {
                    expr: object,
                    var: arg.clone(),
                    name: baml_type::Name::new(param.name().as_str()),
                });
            }
        }
        for (name, value) in fields {
            match field_types.iter().find(|(field, _)| field == name) {
                Some((_, field_ty)) => {
                    let field_ty = substitute_params(field_ty, &instantiation);
                    self.check_expr(body, *value, &field_ty);
                }
                None => {
                    // An INTERFACE field of an implemented interface is not
                    // a constructor key - interface fields are satisfied by
                    // class-owned fields (or explicit `field as class_field`
                    // links). Name the backing class field, and still type
                    // the value against it so a wrong value ALSO reports
                    // (both diagnostics carry signal).
                    if let Some(class_field) = self.constructor_interface_field_link(class, name) {
                        self.pending_diags
                            .push(PendingDiag::InterfaceFieldInConstruction {
                                object,
                                name: name.clone(),
                                class_field: class_field.clone(),
                            });
                        match field_types.iter().find(|(field, _)| *field == class_field) {
                            Some((_, field_ty)) => {
                                let field_ty = substitute_params(field_ty, &instantiation);
                                self.check_expr(body, *value, &field_ty);
                            }
                            None => {
                                self.infer_expr(body, *value, &Expectation::None);
                            }
                        }
                        continue;
                    }
                    self.infer_expr(body, *value, &Expectation::None);
                    let shorthand = matches!(
                        &body.exprs[*value],
                        Expr::Path(segments) if segments.len() == 1 && &segments[0] == name
                    );
                    let class_name = crate::lower::qualify_def(
                        db,
                        baml_compiler2_hir::contributions::Definition::Class(class),
                        &baml_compiler2_ppir::item_data::class_data(db, class).name,
                    );
                    self.pending_diags.push(PendingDiag::UnknownObjectField {
                        object,
                        value: *value,
                        class_name,
                        declared: field_types.iter().map(|(field, _)| field.clone()).collect(),
                        name: name.clone(),
                        shorthand,
                    });
                }
            }
        }
        let short = type_name.0.last().expect("type paths are never empty");
        let object_ty = Ty::intern(TyKind::Class(
            self.lower.qualify_definition(
                baml_compiler2_hir::contributions::Definition::Class(class),
                short,
            ),
            instantiation.into(),
            TyAttr::default(),
        ));
        // A spread source must BE the constructed class at the same
        // arguments (fields are invariant slots): checking against the
        // object's own type both enforces that and lets `Left {
        // ...source }` solve open instantiation slots from the source.
        for spread in spreads {
            self.check_expr(body, spread.expr, &object_ty);
        }
        object_ty
    }

    /// Member access in value position. Inspection site: the receiver must
    /// resolve enough to look inside (rustc's `structurally_resolve`
    /// discipline). Class fields first; then methods on any receiver kind,
    /// as full signatures (self included) with the receiver pinning the
    /// class generics and fresh variables for the method's own - value
    /// position has no turbofish.
    fn field_access(&mut self, at: ExprId, base_ty: &Ty, member: &baml_type::Name) -> Ty {
        let (ty, resolution) = self.field_access_resolved(at, base_ty, member);
        if let Some(resolution) = resolution {
            self.write_member_resolution(at, resolution);
        }
        ty
    }

    /// The resolution core behind [`InferenceContext::field_access`]:
    /// hands the resolution BACK instead of recording, so the union arm
    /// can drop its per-member recursion's resolutions (one expression,
    /// one recorded entry) and `member_callee` can key a call's entry on
    /// the CALLEE expression rather than the anchor.
    fn field_access_resolved(
        &mut self,
        at: ExprId,
        base_ty: &Ty,
        member: &baml_type::Name,
    ) -> (Ty, Option<MemberResolution<'db>>) {
        // `structurally_resolve` expands weak aliases, so `json`
        // answers as its union and an alias-of-class answers as the
        // class - every arm below sees the target.
        let resolved = self.structurally_resolve(base_ty);
        // A union behaves as the intersection existential over the
        // interfaces every arm provides (TIR's rule): `member` resolves
        // through the SINGLE shared interface that declares it -
        // inference sugar for `union.as<I>.member`, `Self` bound to the
        // union itself. An arm's own class fields/methods are never
        // reachable (they need not agree across arms); zero shared
        // declarers report "no common interface", two or more are
        // ambiguous.
        if let TyKind::Union(members, _) = resolved.kind() {
            let members = members.to_vec();
            return self.union_member_access(at, &resolved, &members, member);
        }
        if let TyKind::Class(qtn, args, _) = resolved.kind()
            && let Some(baml_compiler2_hir::contributions::Definition::Class(class)) =
                self.facts.definition_of(qtn)
            && let Some((_, field_ty)) = crate::lower::class_field_types(self.db, class)
                .iter()
                .find(|(field, _)| field == member)
        {
            return (
                substitute_params(field_ty, args),
                Some(MemberResolution::Field {
                    class,
                    field: member.clone(),
                }),
            );
        }
        if let Some(candidate) =
            crate::method_resolution::lookup_method(self.db, &self.facts, &resolved, member)
        {
            // An own method does not arbitrate an interface clash (E0121)
            // - see the callee road's twin check.
            if let Some((sources, is_field)) = crate::method_resolution::concrete_member_ambiguity(
                self.db,
                &self.facts,
                &resolved,
                member,
            ) {
                if self.member_probe_depth == 0 {
                    self.pending_diags.push(PendingDiag::AmbiguousMember {
                        expr: at,
                        base: resolved.clone(),
                        member: member.clone(),
                        sources,
                        is_field,
                    });
                }
                return (Ty::error(), None);
            }
            return match candidate.target {
                crate::method_resolution::MethodCandidateTarget::Source { method, class } => {
                    let signature = function_signature(self.db, method);
                    let mut instantiation = candidate.class_args;
                    let own: Vec<Ty> = signature.generic_params[instantiation.len()..]
                        .iter()
                        .map(|param| self.fresh_generic_arg(param))
                        .collect();
                    instantiation.extend(own);
                    self.register_call_bounds(method, &instantiation, at);
                    (
                        bind_receiver(function_value_ty(signature, &instantiation)),
                        Some(MemberResolution::BoundMethod {
                            class,
                            func: method,
                        }),
                    )
                }
                crate::method_resolution::MethodCandidateTarget::Compiled {
                    class,
                    class_generic_params,
                    class_generic_bounds,
                    method,
                } => {
                    let class_count = class_generic_params.len();
                    let mut instantiation = candidate.class_args;
                    instantiation.extend(
                        method
                            .generic_params
                            .iter()
                            .map(|param| self.fresh_generic_arg(param)),
                    );
                    self.register_compiled_bounds(
                        &class_generic_bounds,
                        &instantiation,
                        usize::MAX,
                        at,
                    );
                    self.register_compiled_bounds(
                        &method.generic_bounds,
                        &instantiation,
                        class_count,
                        at,
                    );
                    (
                        bind_receiver(compiled_function_value_ty(&method, &instantiation)),
                        Some(MemberResolution::CompiledBoundMethod {
                            class,
                            method: method.name,
                        }),
                    )
                }
            };
        }
        match crate::method_resolution::lookup_interface_member(
            self.db,
            &self.facts,
            &resolved,
            member,
        ) {
            crate::method_resolution::InterfaceMemberLookup::Found(interface_member) => {
                let resolution = self.declarer_resolution(&interface_member.declarer, member);
                return (self.interface_member_value(interface_member), resolution);
            }
            crate::method_resolution::InterfaceMemberLookup::Ambiguous { sources, is_field } => {
                if self.member_probe_depth == 0 {
                    self.pending_diags.push(PendingDiag::AmbiguousMember {
                        expr: at,
                        base: resolved.clone(),
                        member: member.clone(),
                        sources,
                        is_field,
                    });
                }
                return (Ty::error(), None);
            }
            crate::method_resolution::InterfaceMemberLookup::FieldRequiresProjection {
                interface,
            } => {
                if self.member_probe_depth == 0 {
                    self.pending_diags
                        .push(PendingDiag::FieldRequiresProjection {
                            expr: at,
                            base: resolved.clone(),
                            member: member.clone(),
                            interface,
                        });
                }
                return (
                    Ty::intern(TyKind::Unknown {
                        attr: TyAttr::default(),
                    }),
                    None,
                );
            }
            crate::method_resolution::InterfaceMemberLookup::SelfRestricted {
                interface,
                position,
            } => {
                if self.member_probe_depth == 0 {
                    self.pending_diags.push(PendingDiag::SelfRestrictedMember {
                        expr: at,
                        interface,
                        member: member.clone(),
                        position,
                    });
                }
                return (Ty::error(), None);
            }
            crate::method_resolution::InterfaceMemberLookup::NotFound => {}
        }
        // A definitely-missing member on a KNOWN base reports; an
        // error/var-carrying base is a cascade of an earlier failure
        // (rustc's tainted_by_errors discipline), and a PROBE leaves
        // the report to its committed frame. `unknown` reports too:
        // it is a first-class user type that must be narrowed before
        // member access, not an error sentinel.
        if self.member_probe_depth == 0 && !resolved.has_error() && !resolved.has_infer() {
            self.pending_diags.push(PendingDiag::UnresolvedMember {
                expr: at,
                base: resolved.clone(),
                member: member.clone(),
            });
        }
        (Ty::error(), None)
    }

    /// The one realized declaring-interface view a UNION receiver's
    /// field access dispatches through, when every member matches an
    /// impl of the SAME realized interface declaring `field` - proper
    /// dyn dispatch (rust's `dyn Trait` field-object shape; TIR's
    /// union-receiver rule). Any member without a unique matching view,
    /// or two members with different realized views, resolves nothing:
    /// absent, so MIR falls back, never a wrong view.
    /// Member access on a UNION receiver in value position: resolve
    /// through the single interface every arm shares, or report which of
    /// the three failure shapes applies (no common declarer, ambiguous
    /// declarers, `Self`-restricted method).
    fn union_member_access(
        &mut self,
        at: ExprId,
        union_ty: &Ty,
        members: &[Ty],
        member: &baml_type::Name,
    ) -> (Ty, Option<MemberResolution<'db>>) {
        match crate::method_resolution::lookup_union_member(
            self.db,
            &self.facts,
            union_ty,
            members,
            member,
        ) {
            crate::method_resolution::UnionMemberLookup::Found(interface_member) => {
                let resolution = self.declarer_resolution(&interface_member.declarer, member);
                (self.interface_member_value(interface_member), resolution)
            }
            crate::method_resolution::UnionMemberLookup::Ambiguous { sources, is_field } => {
                if self.member_probe_depth == 0 {
                    self.pending_diags.push(PendingDiag::AmbiguousMember {
                        expr: at,
                        base: union_ty.clone(),
                        member: member.clone(),
                        sources,
                        is_field,
                    });
                }
                (Ty::error(), None)
            }
            crate::method_resolution::UnionMemberLookup::SelfRestricted {
                interface,
                position,
            } => {
                if self.member_probe_depth == 0 {
                    self.pending_diags.push(PendingDiag::SelfRestrictedMember {
                        expr: at,
                        interface,
                        member: member.clone(),
                        position,
                    });
                }
                (Ty::error(), None)
            }
            crate::method_resolution::UnionMemberLookup::ClassFieldJoin(field_ty) => {
                (field_ty, None)
            }
            crate::method_resolution::UnionMemberLookup::NoCommonInterface => {
                if self.member_probe_depth == 0 && !union_ty.has_error() && !union_ty.has_infer() {
                    self.pending_diags
                        .push(PendingDiag::UnionNoCommonInterface {
                            expr: at,
                            base: union_ty.clone(),
                            member: member.clone(),
                        });
                }
                (Ty::error(), None)
            }
        }
    }

    /// The recorded resolution for an interface member's declarer, when
    /// one applies (the concrete-field backing link is not resolved yet -
    /// no entry rather than a wrong one).
    // A method for call-site symmetry with the other resolution writers.
    #[allow(clippy::unused_self)]
    fn declarer_resolution(
        &self,
        declarer: &crate::method_resolution::MemberDeclarer<'db>,
        member: &baml_type::Name,
    ) -> Option<MemberResolution<'db>> {
        use crate::method_resolution::MemberDeclarer;
        match declarer {
            MemberDeclarer::VirtualField {
                interface,
                realized,
                field_index,
            } => Some(MemberResolution::InterfaceVirtualField {
                interface: *interface,
                view: realized.existential(),
                field_index: *field_index,
                field: member.clone(),
            }),
            MemberDeclarer::VirtualMethod { interface, .. } => {
                Some(MemberResolution::InterfaceVirtualMethod {
                    interface: *interface,
                    method: member.clone(),
                })
            }
            MemberDeclarer::ImplMethod { block, func } => {
                Some(MemberResolution::InterfaceConcreteMethod {
                    impl_block: *block,
                    func: *func,
                })
            }
            MemberDeclarer::ImplField { .. } => None,
            MemberDeclarer::CompiledVirtualField {
                interface,
                realized,
                field_index,
            } => Some(MemberResolution::CompiledInterfaceVirtualField {
                interface: interface.clone(),
                view: realized.existential(),
                field_index: *field_index,
                field: member.clone(),
            }),
            MemberDeclarer::CompiledVirtualMethod { interface, method } => {
                Some(MemberResolution::CompiledBoundMethod {
                    class: interface.clone(),
                    method: method.clone(),
                })
            }
            MemberDeclarer::CompiledImplMethod { method } => {
                Some(MemberResolution::CompiledBoundMethod {
                    class: baml_type::TypeName::new(
                        method.package.clone(),
                        method.namespace.clone(),
                        method.class.clone(),
                    ),
                    method: method.name.clone(),
                })
            }
            MemberDeclarer::CompiledImplField => None,
        }
    }

    /// If `name` is a FIELD declared by an interface this class
    /// implements, the class field backing it (`field as class_field`
    /// link, else the same name). `None` when no implemented interface
    /// declares it.
    fn constructor_interface_field_link(
        &self,
        class: baml_compiler2_hir::loc::ClassLoc<'db>,
        name: &baml_type::Name,
    ) -> Option<baml_type::Name> {
        let db = self.db;
        let data = baml_compiler2_ppir::item_data::class_data(db, class);
        let pkg = baml_compiler2_hir::file_package::file_package(db, class.file(db));
        let pkg_items = baml_compiler2_ppir::package_items(
            db,
            baml_compiler2_hir::package::PackageId::new(db, pkg.package.clone()),
        );
        for block in &data.implements {
            let Some(interface) = crate::interfaces::resolve_ref_to_interface(
                db,
                &data.type_refs,
                block.target,
                pkg_items,
                &pkg.namespace_path,
            ) else {
                continue;
            };
            let declares = baml_compiler2_ppir::item_data::interface_data(db, interface)
                .fields
                .iter()
                .any(|field| field.name == *name);
            if !declares {
                continue;
            }
            let link = block
                .field_links
                .iter()
                .find(|link| link.interface_field == *name)
                .map(|link| link.class_field.clone())
                .unwrap_or_else(|| name.clone());
            return Some(link);
        }
        None
    }

    /// A realized interface rendered for a diagnostic's `as<I>` hint: a
    /// local name inside a namespaced body spells its full `root...` path
    /// (the unqualified spelling may not resolve there), and generic args
    /// ride along so same-interface-different-args sources stay distinct
    /// (TIR's `qualified_interface_display`).
    fn qualified_interface_display(&self, iface: &baml_type::interned::InterfaceRef) -> String {
        let qtn = &iface.name;
        let base = if qtn.is_local() && !self.lower.namespace_context().is_empty() {
            match qtn.namespace().as_slice() {
                [] => format!("root.{}", qtn.name()),
                ns => format!(
                    "root.{}.{}",
                    ns.iter()
                        .map(baml_type::Name::as_str)
                        .collect::<Vec<_>>()
                        .join("."),
                    qtn.name()
                ),
            }
        } else {
            qtn.render_user_facing()
        };
        if iface.generics.is_empty() {
            base
        } else {
            let args = iface
                .generics
                .iter()
                .map(|arg| arg.to_plain().render_user_facing())
                .collect::<Vec<_>>()
                .join(", ");
            format!("{base}<{args}>")
        }
    }

    /// An interface member in VALUE position: no turbofish, so a default
    /// method's own generics instantiate fresh; methods bind their
    /// receiver. Shared by field access and the `default.` value road.
    fn interface_member_value(
        &mut self,
        interface_member: crate::method_resolution::InterfaceMember<'db>,
    ) -> Ty {
        if let Some(pending) = interface_member.pending_own {
            return match pending {
                crate::method_resolution::PendingOwnGenerics::Source { method, prefix } => {
                    let signature = function_signature(self.db, method);
                    let own: Vec<Ty> = signature.generic_params[prefix.len()..]
                        .iter()
                        .map(|param| self.fresh_generic_arg(param))
                        .collect();
                    let mut instantiation = prefix;
                    instantiation.extend(own);
                    bind_receiver(function_value_ty(signature, &instantiation))
                }
                crate::method_resolution::PendingOwnGenerics::Compiled {
                    method,
                    mut bindings,
                } => {
                    let own = method
                        .generic_params
                        .iter()
                        .map(|param| self.fresh_generic_arg(param))
                        .collect::<Vec<_>>();
                    bindings.extend(method.generic_params.iter().cloned().zip(own));
                    bind_receiver(crate::impls::substitute_bindings(
                        &Ty::from_plain(&method.function_ty),
                        &bindings,
                    ))
                }
            };
        }
        if interface_member.is_method {
            return bind_receiver(interface_member.ty);
        }
        interface_member.ty
    }

    /// The expectation's SHAPE for aggregate-literal adoption: nominal
    /// aliases expanded through the oracle (fuel-bounded), so the
    /// recursive `baml.json.json` union answers structurally.
    fn expectation_shape(&mut self, expected: &Expectation) -> Option<Ty> {
        let ty = expected.only_has_type()?.clone();
        Some(self.structurally_resolve(&ty))
    }

    /// Nominal aliases expanded through the oracle (fuel-bounded), for
    /// the tiers that need a type's SHAPE - the recursive
    /// `baml.json.json` union answers structurally.
    fn expand_alias_ty(&mut self, ty: &Ty) -> Ty {
        let mut resolved = ty.clone();
        let mut fuel = 8u32;
        while let TyKind::TypeAlias(qtn, _) = resolved.kind() {
            if fuel == 0 {
                break;
            }
            fuel -= 1;
            match baml_type::normalize::TypeContext::alias_def(&self.facts, qtn) {
                Some(expanded) => resolved = Ty::from_plain(&expanded),
                None => break,
            }
        }
        resolved
    }

    /// The element an ARRAY literal adopts from its expectation: the
    /// expected list itself, or the UNIQUE list member of an expected
    /// union - expected-type propagation into aggregates (r-a's
    /// coercion to the expectation). Arrays are INVARIANT (spec:
    /// Variance), so this adoption is what makes `[1, 2, 3]` a
    /// `json[]` against the recursive `json` union; an ambiguous
    /// multi-list union adopts nothing and the literal synthesizes.
    fn expected_list_element(&mut self, expected: &Expectation) -> Option<Ty> {
        let shape = self.expectation_shape(expected)?;
        match shape.kind() {
            TyKind::List(element, _) => Some(element.clone()),
            TyKind::Union(members, _) => {
                let mut lists = members.iter().filter_map(|member| match member.kind() {
                    TyKind::List(element, _) => Some(element.clone()),
                    _ => None,
                });
                let first = lists.next()?;
                lists.next().is_none().then_some(first)
            }
            _ => None,
        }
    }

    /// The MAP literal's counterpart of `expected_list_element`.
    fn expected_map_entry(&mut self, expected: &Expectation) -> Option<(Ty, Ty)> {
        let shape = self.expectation_shape(expected)?;
        match shape.kind() {
            TyKind::Map { key, value, .. } => Some((key.clone(), value.clone())),
            TyKind::Union(members, _) => {
                let mut maps = members.iter().filter_map(|member| match member.kind() {
                    TyKind::Map { key, value, .. } => Some((key.clone(), value.clone())),
                    _ => None,
                });
                let first = maps.next()?;
                maps.next().is_none().then_some(first)
            }
            _ => None,
        }
    }

    /// The type of a tagged-template body param in scope, innermost frame
    /// winning (template params shadow package items, like locals).
    fn template_param_ty(&self, name: &baml_type::Name) -> Option<Ty> {
        self.template_params
            .iter()
            .rev()
            .find_map(|frame| frame.get(name))
            .cloned()
    }

    fn template_param_root(&self, name: &baml_type::Name) -> bool {
        self.template_param_ty(name).is_some()
    }

    /// Whether a path expression names a local binding or parameter (which
    /// shadows any package-level name at a call site). Keyed under
    /// `current_scope`: a lambda body's expressions live in the semantic
    /// index under the LAMBDA's scope, not the owner's.
    fn path_resolves_locally(&self, expr: ExprId) -> bool {
        self.metadata_key(expr).is_some_and(|key| {
            matches!(
                self.index.path_resolution(key),
                Some(PathResolution::Local(_))
            )
        })
    }

    /// The `BindingId` a pattern introduces, searched within the current
    /// owner's scope subtree (`PatIds` are per-body arenas; the subtree
    /// restriction keeps other bodies' ids apart). The reverse of
    /// `LocalBinding.bind_pattern`, for bindings introduced by non-let
    /// constructs (catch clauses).
    fn binding_for_pattern(&self, pattern: baml_compiler2_ast::PatId) -> Option<BindingId> {
        let owner = self.owner_scope?;
        let descendants = self
            .index
            .scopes
            .get(owner.index() as usize)?
            .descendants
            .clone();
        let subtree = std::iter::once(owner).chain(
            (descendants.start.index()..descendants.end.index())
                .map(baml_compiler2_hir::scope::FileScopeId::new),
        );
        for scope_id in subtree {
            let bindings = self.index.scope_bindings.get(scope_id.index() as usize)?;
            for (index, binding) in bindings.bindings.iter().enumerate() {
                if binding.bind_pattern == pattern {
                    return Some(BindingId::local(scope_id, index));
                }
            }
        }
        None
    }

    /// Resolves a path expression to a local binding or a parameter through
    /// the semantic index. Owner parameters come from the lowered signature;
    /// lambda parameters from the signatures `infer_lambda` deduced.
    /// Non-local names go through `resolve_value_path`.
    fn infer_path(&mut self, expr: ExprId) -> Ty {
        let Some(key) = self.metadata_key(expr) else {
            return Ty::error();
        };
        match self.index.path_resolution(key) {
            Some(PathResolution::Local(binding_id)) => match binding_id.kind {
                BindingKind::Local(_) => {
                    // The flow overlay wins over the declared/widened
                    // binding type (narrowed within a match arm).
                    if let Some(narrowed) = self.flow.get(&binding_id) {
                        return narrowed.clone();
                    }
                    self.index
                        .local_binding(binding_id)
                        .and_then(|binding| self.result.type_of_pat.get(&binding.bind_pattern))
                        .cloned()
                        .unwrap_or_else(Ty::error)
                }
                BindingKind::Parameter(param_index) => {
                    if let Some(narrowed) = self.flow.get(&binding_id) {
                        return narrowed.clone();
                    }
                    let params = if Some(binding_id.scope) == self.owner_scope {
                        Some(&self.param_tys)
                    } else {
                        self.lambda_params.get(&binding_id.scope)
                    };
                    params
                        .and_then(|params| params.get(param_index))
                        .cloned()
                        .unwrap_or_else(Ty::error)
                }
            },
            Some(PathResolution::Unknown) | None => Ty::error(),
        }
    }

    /// One body-position annotation, lowered and hole-instantiated - the
    /// single entry for every type written inside a body (let ascriptions,
    /// lambda signature slots, turbofish go through `instantiation_args`).
    fn lower_body_annotation(&mut self, type_ref: baml_compiler2_hir::type_ref::TypeRefId) -> Ty {
        if let Some(cached) = self.annotation_cache.get(&type_ref) {
            return cached.clone();
        }
        let lowered = self.lower.lower_type_ref(&self.type_refs.store, type_ref);
        // Written-type well-formedness (rustc's wfcheck at body
        // annotations): generic arguments must satisfy their heads'
        // declared bounds. Hole-carrying annotations skip - their holes
        // solve first and the instantiation sites judge them.
        if !lowered.has_infer() {
            let env = self.wf_scope_env.get_or_init(|| match self.body_owner {
                Some(function) => crate::lower::function_generic_bounds(self.db, function)
                    .into_iter()
                    .map(|(param, refs)| {
                        (
                            param,
                            refs.iter()
                                .map(|bound| baml_type::Interface {
                                    name: bound.name.clone(),
                                    generics: bound
                                        .generics
                                        .iter()
                                        .map(baml_type::interned::Ty::to_plain)
                                        .collect(),
                                    associated_types: bound
                                        .associated_types
                                        .iter()
                                        .map(|(name, ty)| (name.clone(), ty.to_plain()))
                                        .collect(),
                                })
                                .collect(),
                        )
                    })
                    .collect(),
                None => rustc_hash::FxHashMap::default(),
            });
            for error in
                crate::interfaces::type_generic_bound_errors(self.db, env, &lowered.to_plain())
            {
                self.pending_diags
                    .push(PendingDiag::AnnotWf { type_ref, error });
            }
        }
        let instantiated = self.instantiate_holes(&lowered, HoleAnchor::TypeRef(type_ref));
        self.annotation_cache.insert(type_ref, instantiated.clone());
        instantiated
    }

    /// The `process_user_written_ty` funnel (rust-analyzer's discipline):
    /// lowering is pure and emits var-less hole nodes for `_`; the inference
    /// side instantiates each hole as a fresh table variable, filled from
    /// context.
    fn instantiate_holes(&mut self, ty: &Ty, at: HoleAnchor) -> Ty {
        if !ty.has_infer() {
            return ty.clone();
        }
        if matches!(ty.kind(), TyKind::Infer { var: None, .. }) {
            let var_ty = self.table.new_var_ty();
            if let TyKind::Infer { var: Some(var), .. } = var_ty.kind() {
                self.hole_vars.push((*var, at));
            }
            return var_ty;
        }
        Ty::intern(
            ty.kind()
                .map_children(|child| self.instantiate_holes(child, at)),
        )
    }

    /// [`Self::instantiate_holes`] for EXPRESSION-position type arguments
    /// (turbofish, generic-apply values, upcast targets): a written `_`
    /// there is a hard E0147 outright - TIR's rule; expression positions
    /// have no annotation slot for inference to fill (`is Show<_>` /
    /// `.as<Show<_>>` are ascriptions with no local source). The hole
    /// still instantiates as a fresh var so inference proceeds for
    /// RECOVERY, but the diagnostic is unconditional and immediate,
    /// never dependent on whether the var happens to solve.
    fn reject_expr_position_holes(&mut self, ty: &Ty, at: ExprId) -> Ty {
        if !ty.has_infer() {
            return ty.clone();
        }
        if matches!(ty.kind(), TyKind::Infer { var: None, .. }) {
            self.pending_diags
                .push(PendingDiag::ExprPositionHole { expr: at });
            return self.table.new_var_ty();
        }
        Ty::intern(
            ty.kind()
                .map_children(|child| self.reject_expr_position_holes(child, at)),
        )
    }

    /// `base catch (e) { arms }` / `catch_all`: narrowing on the ERROR
    /// channel. The base's effect contributions collect into their own
    /// channel; the clause binding takes that union; arms subtract what
    /// they provably handle (the pattern set-subtraction machinery); the
    /// residual propagates to the enclosing channel. The result joins the
    /// base value with the arm values. Exhaustiveness of `catch_all` is
    /// S17's diagnostic.
    fn infer_catch(
        &mut self,
        body: &ExprBody,
        base: ExprId,
        clauses: &[baml_compiler2_ast::CatchClause],
        expected: &Expectation,
    ) -> Ty {
        let branch_expectation = expected.adjust_for_branches(&mut self.table);
        self.throws_channels.push(Vec::new());
        let base_ty = self.infer_expr(body, base, &branch_expectation);
        let channel = self.throws_channels.pop().expect("pushed above");
        // catch discharges a SET of throw FACTS, never a value of a
        // union type (match scrutinizes values; catch removes facts):
        // each contribution finalizes and top-level unions split into
        // their member facts, so per-arm matching and subtraction work
        // at fact grain - which is what "rethrow the rest" means.
        let mut facts: Vec<Ty> = Vec::new();
        for (_, contribution) in &channel {
            let finalized = self.finalize_incoming_effect(contribution);
            if matches!(finalized.kind(), TyKind::Never { .. }) {
                continue;
            }
            let resolved = self.table.resolve_completely(&finalized);
            let canonical = self.matrix_scrut(&resolved);
            match canonical.kind() {
                TyKind::Union(members, _) => {
                    for member in members {
                        if !facts.contains(member) {
                            facts.push(member.clone());
                        }
                    }
                }
                _ => {
                    if !facts.contains(&canonical) {
                        facts.push(canonical);
                    }
                }
            }
        }

        let had_facts = !facts.is_empty();
        let mut arm_tys = vec![base_ty];
        for clause in clauses {
            let clause_binding_ty = if facts.is_empty() {
                Ty::never()
            } else {
                self.union_of(&facts)
            };
            self.result
                .type_of_pat
                .insert(clause.binding, clause_binding_ty);
            if let Some(context) = clause.stack_trace_binding {
                // The second binding (`catch (e, ctx)`) is the full error
                // CONTEXT - `baml.errors.ErrorContext` (the AST field's
                // "stack trace" name understates it; TIR resolves the
                // class). Lookup-gated, fail-safe to Error.
                let context_ty = match self.facts.definition_of(&baml_type::TypeName::new(
                    baml_type::Name::new("baml"),
                    vec![baml_type::Name::new("errors")],
                    baml_type::Name::new("ErrorContext"),
                )) {
                    Some(baml_compiler2_hir::contributions::Definition::Class(_)) => {
                        Ty::intern(TyKind::Class(
                            baml_type::TypeName::new(
                                baml_type::Name::new("baml"),
                                vec![baml_type::Name::new("errors")],
                                baml_type::Name::new("ErrorContext"),
                            ),
                            Box::new([]),
                            TyAttr::default(),
                        ))
                    }
                    _ => Ty::error(),
                };
                self.result.type_of_pat.insert(context, context_ty);
            }
            for &arm_id in &clause.arms {
                let arm = &body.catch_arms[arm_id];
                // The arm's CLAIM: its pattern's own denotation, probed
                // against `unknown` (TIR's probe; a wildcard or bare
                // bind denotes `unknown` and so definitely-matches every
                // fact below). Bindings recorded here are overwritten by
                // the real walk against the arm's scrutinee.
                let claim = self
                    .lower_pattern(
                        body,
                        arm.pattern,
                        &Ty::intern(TyKind::Unknown {
                            attr: TyAttr::default(),
                        }),
                    )
                    .matched_ty;
                // Fact-by-fact matching: a fact INSIDE the claim is
                // definitely handled (and subtracts); a claim inside a
                // fact MAY receive it (narrowing view, no subtraction).
                let mut may: Vec<Ty> = Vec::new();
                let mut definite: Vec<Ty> = Vec::new();
                for fact in &facts {
                    if pat::provable_subtype(fact, &claim, &self.facts) {
                        may.push(fact.clone());
                        definite.push(fact.clone());
                    } else if pat::provable_subtype(&claim, fact, &self.facts) {
                        may.push(fact.clone());
                    }
                }
                // The panic side-rule: a claim naming `baml.panics.*`
                // types is ALWAYS live - panics are catchable at runtime
                // but never part of a `throws` contract, so they fold in
                // beside the facts without consulting them.
                if let Some(panic) = self.panic_subset(&claim) {
                    may.push(panic);
                } else if had_facts && may.is_empty() && !claim.has_error() {
                    // Earlier arms drained every fact this arm could
                    // receive (and it claims no panic): unreachable. An
                    // ERRORED claim (unresolved class pattern) judges
                    // nothing - its own report stands alone.
                    self.pending_diags.push(PendingDiag::UnreachableArm {
                        expr: arm.body,
                        warning: true,
                    });
                }
                // The arm's scrutinee: its may-set; else the WRITTEN
                // claim (ruling 3's fallback - an arm no fact reaches
                // still types by what the user wrote; unreachability is
                // S17's warning). A wildcard/bare bind WROTE nothing -
                // its claim probed as `unknown` - so with no facts left
                // its world is `never`, not the top type.
                let arm_scrut = if may.is_empty() {
                    if matches!(claim.kind(), TyKind::Unknown { .. }) {
                        Ty::never()
                    } else {
                        claim.clone()
                    }
                } else {
                    self.union_of(&may)
                };
                let outcome = self.lower_pattern(body, arm.pattern, &arm_scrut);
                // The clause binding NARROWS to the arm's matched type
                // inside the arm body - the match-arm discipline applied
                // to `e` (`Boom => e.m` sees `Boom`, not the whole set).
                let entry_flow = self.flow.clone();
                if let Some(binding) = self.binding_for_pattern(clause.binding) {
                    self.flow.insert(binding, outcome.matched_ty.clone());
                }
                self.diverges = Diverges::Maybe;
                let arm_ty = self.infer_expr(body, arm.body, &branch_expectation);
                self.flow = entry_flow;
                arm_tys.push(arm_ty);
                // Definitely-handled facts leave the set; the survivors
                // are the next arm's world and, at the end, the rethrow.
                facts.retain(|fact| !definite.contains(fact));
            }
        }
        for fact in &facts {
            self.record_throw(base, fact);
        }
        self.join(&arm_tys)
    }

    /// The `baml.panics.*` component of a catch claim (alias-aware,
    /// union members collected) - the types an arm may always trap.
    fn panic_subset(&mut self, claim: &Ty) -> Option<Ty> {
        let expanded = self.expand_alias_ty(claim);
        match expanded.kind() {
            TyKind::Class(qtn, _, _) if qtn.is_panic_type() => Some(expanded.clone()),
            TyKind::Union(members, _) => {
                let members = members.to_vec();
                let panics: Vec<Ty> = members
                    .iter()
                    .filter_map(|member| self.panic_subset(member))
                    .collect();
                if panics.is_empty() {
                    None
                } else {
                    Some(self.union_of(&panics))
                }
            }
            _ => None,
        }
    }

    /// A caught effect contribution, resolved for the error channel: still
    /// live variables resolve where possible (an unconstrained effect is
    /// `never` here too).
    fn finalize_incoming_effect(&mut self, ty: &Ty) -> Ty {
        let resolved = self.table.resolve_completely(ty);
        if resolved.has_infer() {
            // Effect vars inside the base that never got constrained: the
            // conservative read for catching purposes is Error-free
            // emptiness - drop to never; real obligations arrive with I4.
            return Ty::never();
        }
        resolved
    }

    /// `$id` reads as a string value but is not a binding: member access on
    /// it reports the rewrite hint (bind it to a local first).
    fn check_runtime_id_member(
        &mut self,
        body: &ExprBody,
        expr: ExprId,
        base: ExprId,
        member: &baml_type::Name,
    ) -> bool {
        let is_id = matches!(&body.exprs[base], Expr::Path(segments)
            if segments.len() == 1 && segments[0].as_str() == "$id");
        if is_id {
            self.pending_diags.push(PendingDiag::RuntimeIdMember {
                expr,
                member: member.clone(),
            });
        }
        is_id
    }

    fn is_runtime_id_path(body: &ExprBody, expr: ExprId) -> bool {
        matches!(&body.exprs[expr], Expr::Path(segments)
            if segments.len() == 1 && segments[0].as_str() == "$id")
    }

    /// A `?.` link whose base PROVABLY cannot be null is noise the user
    /// probably didn't intend (E0004's did-you-mean family). Error/var
    /// bases stay silent as cascades; chain-internal nullability (an
    /// earlier `?.` peeled it) counts as nullable, so only the outermost
    /// truly-non-null base fires - TIR's per-link rule.
    fn check_needless_chain(&mut self, body: &ExprBody, expr: ExprId, base: ExprId, base_ty: &Ty) {
        let resolved = self.table.resolve_completely(base_ty);
        if resolved.has_error() || resolved.has_infer() {
            return;
        }
        let nullable = match resolved.kind() {
            TyKind::Null { .. } | TyKind::Unknown { .. } => true,
            TyKind::Union(members, _) => members
                .iter()
                .any(|member| matches!(member.kind(), TyKind::Null { .. })),
            _ => false,
        };
        // An optional link INSIDE a chain sees its base already peeled;
        // the sugar is what makes the chain work, so it is never noise.
        let base_is_chain = matches!(
            body.exprs[base],
            Expr::OptionalMemberAccess { .. } | Expr::OptionalCall { .. }
        );
        if !nullable && !base_is_chain {
            self.pending_diags
                .push(PendingDiag::UnnecessaryOptionalChain {
                    expr,
                    expr_text: body.display_expr(expr),
                    base_text: body.display_expr(base),
                });
        }
    }

    /// BEP-049 §11: every `${expr}` in an UNTAGGED template must be
    /// non-nullable (implicit `.to_string()` on null has no sound
    /// rendering). Nested `${for}`/`${if}` segments walk recursively.
    fn check_template_interps_strict(&mut self, segments: &[baml_compiler2_ast::TemplateSegment]) {
        use baml_compiler2_ast::TemplateSegment;
        for segment in segments {
            match segment {
                TemplateSegment::Interp(expr) => {
                    let Some(ty) = self.result.type_of_expr.get(expr).cloned() else {
                        continue;
                    };
                    let resolved = self.table.resolve_completely(&ty);
                    if resolved.has_error() || resolved.has_infer() {
                        continue;
                    }
                    let nullable = match resolved.kind() {
                        TyKind::Null { .. } => true,
                        TyKind::Union(members, _) => members
                            .iter()
                            .any(|member| matches!(member.kind(), TyKind::Null { .. })),
                        _ => false,
                    };
                    if nullable {
                        self.pending_diags.push(PendingDiag::InterpolatedMaybeNull {
                            expr: *expr,
                            ty: resolved,
                        });
                    }
                }
                TemplateSegment::For { body, .. } | TemplateSegment::CStyleFor { body, .. } => {
                    self.check_template_interps_strict(body);
                }
                TemplateSegment::If {
                    branches,
                    else_body,
                } => {
                    for branch in branches {
                        self.check_template_interps_strict(&branch.body);
                    }
                    if let Some(else_body) = else_body {
                        self.check_template_interps_strict(else_body);
                    }
                }
                TemplateSegment::Text(_) => {}
            }
        }
    }

    /// One effect contribution: a thrown value or a callee's throws,
    /// accumulated into the current channel and, when the owner DECLARED
    /// its clause, checked against that contract - including when the
    /// clause mentions rigid type vars (the check defers through bounds
    /// rather than being skipped: B-1082's rule).
    fn record_throw(&mut self, at: ExprId, ty: &Ty) {
        if matches!(ty.kind(), TyKind::Never { .. }) || ty.has_error() {
            return;
        }
        // Thrown literals KEEP their literal types (no widening): catch
        // arms match on literal error codes, and the canonical union at
        // the channel is the generation site. The RUNTIME boundary
        // widens (the provider's conversion): `reflect.signature` on a
        // `throw "negative"` lambda reconstructs `string`.
        let contribution = ty.clone();
        // An OPEN clause (`throws T | _`) admits every contribution; the
        // remainder joins the surface at finalize instead of erroring.
        if let Some(declared) = self.declared_throws.clone()
            && !self.declared_throws_open
            && !declared.has_error()
            && self.throws_channels.len() == 1
        {
            // An OPEN contribution (a callee's still-unsolved effect param)
            // is judged at finalize only - running `sub` on it here would
            // DEPOSIT `?effect <= declared` as a bound and wedge the var
            // against the callback's actual surface. Ground contributions
            // judge (and stash for finalize re-judgment) immediately.
            if contribution.has_infer() || !self.sub(&contribution, &declared) {
                self.pending_diags.push(PendingDiag::ThrowsViolation {
                    at,
                    declared,
                    extra: contribution.clone(),
                });
            }
        }
        self.throws_channels
            .last_mut()
            .expect("channel stack never empty")
            .push((at, contribution));
    }

    /// The endgame (S13 finalize): resolve bounded variables to fixpoint,
    /// drain the deferred residue, then FINALIZE every recorded type -
    /// substitute solutions, replace each surviving variable or hole with
    /// the Error sentinel LOCALLY (rust-analyzer's replace-with-error
    /// discipline, never poison-to-top; rulings 2/3 - the diagnostics land
    /// with S17), and re-canonicalize the unions that `union_of` left
    /// syntactic while variables were live. The invariant afterward: no
    /// `Infer` reaches the result.
    /// Records one member access's resolution (r-a's
    /// `write_method_resolution` family). A call's entry sits on the
    /// CALLEE expression; value reads sit on the accessing expression.
    fn write_member_resolution(&mut self, expr: ExprId, resolution: MemberResolution<'db>) {
        self.result.member_resolutions.insert(expr, resolution);
    }

    /// Records a call's solved instantiation vector (raw; ground after
    /// writeback) with the owner-prefix split. The plan's two halves have
    /// two writers keyed by the same call id - type args at the
    /// instantiation site, bindings in `check_call_args` - the r-a shape
    /// of separate tables written where each decision is made, co-located
    /// in one struct.
    fn write_call_type_args(&mut self, call: ExprId, type_args: &[Ty], own_offset: usize) {
        let explicit = self.type_refs.expr_type_args.contains_key(&call);
        let plan = self.result.call_plans.entry(call).or_default();
        plan.type_args = type_args.to_vec();
        plan.own_offset = own_offset;
        plan.explicit = explicit;
    }

    /// Walks the MEMBER segments of a value-rooted path (everything after
    /// the root name), returning the final type and the recorded ladder.
    /// Shared by the value road and the callee road; callers append their
    /// final segment when they resolve it differently (a callee's last
    /// member goes through `member_callee`) and then write the tables.
    fn walk_path_members(
        &mut self,
        expr: ExprId,
        root_ty: Ty,
        members: &[baml_type::Name],
    ) -> (Ty, Vec<ResolvedPathSegment<'db>>) {
        let mut steps = vec![ResolvedPathSegment {
            ty: root_ty.clone(),
            resolution: None,
        }];
        let mut ty = root_ty;
        for member in members {
            let (next, resolution) = self.field_access_resolved(expr, &ty, member);
            steps.push(ResolvedPathSegment {
                ty: next.clone(),
                resolution,
            });
            ty = next;
        }
        (ty, steps)
    }

    /// Writes a completed path ladder: the per-segment table entry (only
    /// multi-segment paths - a bare local is just `type_of_expr`) and the
    /// FINAL member's resolution at the path expression, where value
    /// consumers key.
    fn write_resolved_path(&mut self, expr: ExprId, steps: Vec<ResolvedPathSegment<'db>>) {
        if steps.len() < 2 {
            return;
        }
        if let Some(last) = steps.last().and_then(|step| step.resolution.clone()) {
            self.write_member_resolution(expr, last);
        }
        self.result
            .path_resolutions
            .insert(expr, ResolvedPath { segments: steps });
    }

    fn finish(mut self) -> InferenceResult<'db> {
        // The fulfillment fixpoint: solve what FULL bounds determine,
        // attempt obligations, re-drive the deferred residue, repeat
        // while any side progresses (rustc re-runs stalled obligations
        // until quiescent - the deferred subs are our stalled goals).
        // Only at QUIESCENCE does the ground-subset tier commit classes
        // from partial bounds (rustc's fallback placement): it stays the
        // operator-deadlock breaker without deciding a var whose
        // deferred lowers mention a sibling that was still solvable.
        loop {
            let solved = self.resolve_bounded_vars(SolveTier::Ground);
            let replayed = self.replay_solved_class_bounds();
            let obligations = self.discharge_obligations_once();
            let subs = self.drain_deferred_subs();
            if solved || replayed || obligations || subs {
                continue;
            }
            if !self.resolve_bounded_vars(SolveTier::GroundSubset) {
                break;
            }
        }
        // Quiescence: a residue pair still open on BOTH sides can never
        // be decided by more solving. Skolemize the unsolved vars (rigid
        // placeholders - the leak-check discipline) and judge: a pair no
        // substitution could satisfy is a definite mismatch, reported at
        // the expr that deposited it (`let m = []; m = {}` - the heads
        // clash whatever the elements become). A pair a substitution
        // COULD satisfy passes the skolem judgment too (rigid vars are
        // opaque, so only head-level impossibility fails), keeping this
        // report-only. Projection pairs stay undecidable (their bases
        // never resolved) and drop, as do anchorless VarBounds flushes.
        for (actual, expected, anchor) in std::mem::take(&mut self.deferred_subs) {
            let Some(anchor) = anchor else { continue };
            let actual = self.table.resolve_completely(&actual);
            let expected = self.table.resolve_completely(&expected);
            if actual.has_projection()
                || expected.has_projection()
                || actual.has_error()
                || expected.has_error()
            {
                continue;
            }
            let judged_actual = skolemize_infer(&actual);
            let judged_expected = skolemize_infer(&expected);
            if !is_subtype_interned(&judged_actual, &judged_expected, &self.facts) {
                self.result
                    .type_mismatches
                    .entry(anchor)
                    .or_insert((expected, actual));
            }
        }
        // BAML's only defaulting rule: an unconstrained EFFECT is `never`
        // (a value variable erases to Error instead - ruling 2).
        self.table.default_unsolved_effects_to_never();
        let throws = match self.declared_throws.clone() {
            // A closed clause IS the surface (declared wins, rule 1),
            // VERBATIM - the written member order is render fidelity
            // (ruling 3's record-what-the-user-wrote), so no finalize
            // pass here; the sweep proved canonical reordering breaks
            // agreement on the written surface.
            Some(declared) if !self.declared_throws_open => declared,
            // Open or omitted: the inferred set, with an open clause's
            // named part joining the union (spec rule 3 - callers see
            // declared + inferred).
            declared => {
                let contributions = self.throws_channels[0].clone();
                let mut resolved: Vec<Ty> = contributions
                    .iter()
                    .map(|(_, ty)| self.finalize_ty(ty))
                    .filter(|ty| !ty.has_error())
                    .collect();
                if let Some(named) = declared {
                    resolved.push(named);
                }
                if resolved.is_empty() {
                    Ty::never()
                } else {
                    self.union_of(&resolved)
                }
            }
        };
        // E0097: with a CLOSED declared clause, a declared fact nothing
        // thrown matches exactly (interface-implementor coverage aside)
        // is extraneous - a warning, anchored at the body root (the
        // clause itself lives in the signature store).
        if let Some(declared) = self.declared_throws.clone()
            && !self.declared_throws_open
            && !declared.has_error()
            && let Some(root) = self.body_root
        {
            // Coverage compares WIDENED facts (TIR's fact grain: a thrown
            // `"boom"` covers a declared `string`) while the report keeps
            // the declared spelling.
            let declared_facts = crate::package_interface::flatten_ty_to_facts(
                &self.finalize_ty(&declared).to_plain(),
            );
            let effective: std::collections::BTreeSet<baml_type::Ty> = self.throws_channels[0]
                .clone()
                .iter()
                .flat_map(|(_, ty)| {
                    crate::throw_facts::flatten_declared_ty_to_facts(
                        &self.finalize_ty(ty).to_plain(),
                    )
                })
                .collect();
            let mut extraneous: Vec<String> = declared_facts
                .iter()
                .filter(|decl| {
                    let widened_decl: std::collections::BTreeSet<baml_type::Ty> =
                        crate::throw_facts::flatten_declared_ty_to_facts(decl);
                    let covered = widened_decl.iter().all(|w| effective.contains(w));
                    !(covered
                        || matches!(decl, baml_type::Ty::Interface(..))
                            && effective.iter().any(|eff| {
                                baml_type::normalize::is_subtype(eff, decl, &self.facts)
                            }))
                })
                .map(baml_type::Ty::render_user_facing)
                .collect();
            extraneous.sort();
            if !extraneous.is_empty() {
                self.pending_diags.push(PendingDiag::ExtraneousThrows {
                    at: root,
                    extra_types: extraneous,
                });
            }
        }
        let mut result = std::mem::take(&mut self.result);
        result.throws = throws;
        for ty in result
            .type_of_expr
            .values_mut()
            .chain(result.type_of_pat.values_mut())
        {
            *ty = self.finalize_ty(ty);
        }
        // Provisional checks re-judge now that their expectations solved:
        // a definite failure joins the mismatch table (first writer per
        // expr wins - a direct mismatch is the better message).
        for (expr, expected, actual) in std::mem::take(&mut self.provisional_checks) {
            let expected = self.finalize_ty(&expected);
            let actual = self.finalize_ty(&actual);
            if expected.has_error()
                || actual.has_error()
                || is_subtype_interned(&actual, &expected, &self.facts)
            {
                continue;
            }
            result
                .type_mismatches
                .entry(expr)
                .or_insert((expected, actual));
        }
        for (expected, actual) in result.type_mismatches.values_mut() {
            *expected = self.finalize_ty(expected);
            *actual = self.finalize_ty(actual);
        }
        // S17: materialize the user-facing diagnostics - the finalized
        // mismatch table plus the pendings, in the shared vocabulary with
        // PLAIN payload types. Sorted by anchor for determinism (the
        // check layer re-sorts globally by span).
        {
            use crate::diagnostics::{
                DiagnosticLocation, DiagnosticSeverity, TirDiagnostic, TirTypeError,
            };
            let mut diags: Vec<TirDiagnostic<'db>> = Vec::new();
            for (&expr, (expected, actual)) in &result.type_mismatches {
                // rustc's tainted_by_errors discipline: a mismatch whose
                // operand IS the error sentinel is a CASCADE of a reported
                // failure. Error LEAVES inside a structural head are a
                // different case - `list<!e>` against `map<...>` is a real
                // head mismatch (unsolved container vars finalize their
                // elements to the sentinel without erasing the finding).
                let tainted = |ty: &Ty| {
                    ty.has_error()
                        && matches!(ty.kind(), TyKind::Error { .. } | TyKind::Unknown { .. })
                };
                if tainted(expected) || tainted(actual) {
                    continue;
                }
                // A check can fail MID-INFERENCE on still-open variables
                // that later resolution satisfies; only a mismatch that
                // HOLDS in the finalized world reports.
                if is_subtype_interned(actual, expected, &self.facts) {
                    continue;
                }
                // The for-desugar's iterability failure reads as its own
                // message (TIR's NotIterable), not a raw interface mismatch.
                let error = match expected.kind() {
                    TyKind::Interface(qtn, _, _, _)
                        if qtn.package().as_str() == "baml"
                            && qtn.namespace().len() == 1
                            && qtn.namespace()[0].as_str() == "iter"
                            && qtn.name().as_str() == "Iterable" =>
                    {
                        TirTypeError::NotIterable {
                            ty: actual.to_plain(),
                        }
                    }
                    // BEP-044 wf3 #G18: a value that ALMOST implements an
                    // expected interface through a blanket `implements`
                    // rule - the implementor shape matches but a generic
                    // bound fails - names the unsatisfied bound (rustc's
                    // obligation-cause refinement of a fulfillment error)
                    // rather than a bare mismatch.
                    TyKind::Interface(..) => {
                        match self.first_failing_blanket_bound(actual, expected) {
                            Some(bound) => TirTypeError::BlanketBoundNotSatisfied {
                                value_type: actual.to_plain(),
                                bound,
                            },
                            None => TirTypeError::TypeMismatch {
                                expected: expected.to_plain(),
                                got: actual.to_plain(),
                            },
                        }
                    }
                    _ => TirTypeError::TypeMismatch {
                        expected: expected.to_plain(),
                        got: actual.to_plain(),
                    },
                };
                diags.push(TirDiagnostic {
                    error,
                    severity: DiagnosticSeverity::Error,
                    primary: DiagnosticLocation::Expr(expr),
                    related: Vec::new(),
                });
            }
            // The body LowerCtx's sink: every written annotation whose
            // path resolved nowhere (E0002), anchored at its TypeRefId.
            for lowering in self.lower.take_diagnostics() {
                self.pending_diags.push(PendingDiag::BodyAnnot {
                    type_ref: lowering.type_ref,
                    kind: lowering.kind,
                });
            }
            // A `_` hole that never solved reports E0147 at the hole
            // (rustc's E0282). One WRITTEN hole may instantiate several
            // times (the typing drive and the pattern walk both lower the
            // same ascription), so instantiations group by their source
            // anchor: the hole is solved iff ANY of its instantiations
            // solved (whichever one the checking demand flowed through
            // carries the answer), and one report covers the anchor.
            let mut by_anchor: Vec<(HoleAnchor, bool)> = Vec::new();
            for (var, at) in std::mem::take(&mut self.hole_vars) {
                let solved = !self
                    .table
                    .resolve_completely(&Ty::infer_var(var))
                    .has_infer();
                match by_anchor.iter_mut().find(|(seen, _)| *seen == at) {
                    Some((_, any_solved)) => *any_solved |= solved,
                    None => by_anchor.push((at, solved)),
                }
            }
            for (at, any_solved) in by_anchor {
                if any_solved {
                    continue;
                }
                let location = match at {
                    HoleAnchor::TypeRef(type_ref) => DiagnosticLocation::TypeRef(type_ref),
                };
                diags.push(TirDiagnostic {
                    error: TirTypeError::CannotInferType,
                    severity: DiagnosticSeverity::Error,
                    primary: location,
                    related: Vec::new(),
                });
            }
            for pending in std::mem::take(&mut self.pending_diags) {
                let mut unreachable_is_warning = false;
                let (error, expr) = match pending {
                    PendingDiag::NonExhaustiveMatch {
                        expr,
                        scrutinee,
                        missing,
                    } => (
                        TirTypeError::NonExhaustiveMatch {
                            scrutinee_type: self.finalize_ty(&scrutinee).to_plain(),
                            missing_cases: missing,
                        },
                        expr,
                    ),
                    PendingDiag::UnreachableArm { expr, warning } => {
                        unreachable_is_warning = warning;
                        (TirTypeError::UnreachableArm, expr)
                    }
                    PendingDiag::MissingNamedArg { expr, name } => {
                        (TirTypeError::MissingRequiredArgument { name }, expr)
                    }
                    PendingDiag::PositionalDefaultedArg { expr, name } => (
                        TirTypeError::DefaultedParamPassedPositionally { name },
                        expr,
                    ),
                    PendingDiag::UnresolvedCtor {
                        expr,
                        name,
                        suggestions,
                    } => (TirTypeError::UnresolvedType { name, suggestions }, expr),
                    PendingDiag::PositionalAfterNamed { expr } => {
                        (TirTypeError::PositionalArgumentAfterNamed, expr)
                    }
                    PendingDiag::DuplicateNamedArg { expr, name } => {
                        (TirTypeError::DuplicateNamedArgument { name }, expr)
                    }
                    PendingDiag::UnresolvedName { expr, name } => {
                        (TirTypeError::UnresolvedName { name }, expr)
                    }
                    PendingDiag::UnresolvedMember { expr, base, member } => (
                        TirTypeError::UnresolvedMember {
                            base_type: self.finalize_ty(&base).to_plain(),
                            member,
                        },
                        expr,
                    ),
                    PendingDiag::AnnotWf { type_ref, error } => {
                        diags.push(TirDiagnostic {
                            error,
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::TypeRef(type_ref),
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::TaggedTagInvalid {
                        at,
                        name,
                        func,
                        kind,
                    } => {
                        let (error, note) = match kind {
                            TaggedTagIssue::NotAFunction => {
                                (TirTypeError::TaggedTagNotAFunction { name }, None)
                            }
                            TaggedTagIssue::NotMarked => (
                                TirTypeError::TaggedTagNotMarked { name },
                                Some(
                                    "add a `//baml:tagged_string` marker comment above this function",
                                ),
                            ),
                            TaggedTagIssue::BadBodyParam => (
                                TirTypeError::TaggedTagBadBodyParam { name },
                                Some(
                                    "the first parameter must be `body: (...) -> baml.TaggedString`",
                                ),
                            ),
                        };
                        let related = match (func, note) {
                            (Some(func), Some(note)) => vec![crate::diagnostics::RelatedNote::new(
                                crate::diagnostics::RelatedLocation::Item(
                                    baml_compiler2_hir::contributions::Definition::Function(func),
                                ),
                                note,
                            )],
                            _ => Vec::new(),
                        };
                        diags.push(TirDiagnostic {
                            error,
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::Expr(at),
                            related,
                        });
                        continue;
                    }
                    PendingDiag::ExprPositionHole { expr } => (TirTypeError::CannotInferType, expr),
                    PendingDiag::IntLiteralOutOfRange { expr, value } => {
                        (TirTypeError::IntegerLiteralOutOfRange { value }, expr)
                    }
                    PendingDiag::AmbiguousMember {
                        expr,
                        base,
                        member,
                        sources,
                        is_field,
                    } => {
                        let receiver = baml_type::Name::new(
                            self.finalize_ty(&base).to_plain().render_user_facing(),
                        );
                        let sources: Vec<String> = sources
                            .iter()
                            .map(|iface| self.qualified_interface_display(iface))
                            .collect();
                        let err = if is_field {
                            TirTypeError::AmbiguousInterfaceField {
                                class_name: receiver,
                                field_name: member,
                                sources: sources.iter().map(baml_type::Name::new).collect(),
                            }
                        } else {
                            TirTypeError::AmbiguousInterfaceMethod {
                                class_name: receiver,
                                method_name: member,
                                sources,
                            }
                        };
                        (err, expr)
                    }
                    PendingDiag::FieldRequiresProjection {
                        expr,
                        base,
                        member,
                        interface,
                    } => (
                        TirTypeError::InterfaceFieldRequiresProjection {
                            class_name: baml_type::Name::new(
                                self.finalize_ty(&base).to_plain().render_user_facing(),
                            ),
                            field_name: member,
                            interface_name: baml_type::Name::new(
                                self.qualified_interface_display(&interface),
                            ),
                        },
                        expr,
                    ),
                    PendingDiag::SelfRestrictedMember {
                        expr,
                        interface,
                        member,
                        position,
                    } => (
                        TirTypeError::InvalidSelfCallThroughInterface {
                            interface_name: baml_type::Name::new(
                                self.qualified_interface_display(&interface),
                            ),
                            method_name: member,
                            position,
                        },
                        expr,
                    ),
                    PendingDiag::UnionNoCommonInterface { expr, base, member } => (
                        TirTypeError::UnionMemberNoCommonInterface {
                            union: self.finalize_ty(&base).to_plain(),
                            member,
                        },
                        expr,
                    ),
                    PendingDiag::BoundedArgNotConcrete { expr, arg, bound } => (
                        TirTypeError::BoundedTypeArgNotConcrete {
                            arg: self.finalize_ty(&arg).to_plain(),
                            bound: Box::new([baml_type::Interface::new(
                                bound.name.clone(),
                                bound.generics.iter().map(Ty::to_plain).collect(),
                                bound
                                    .associated_types
                                    .iter()
                                    .map(|(name, ty)| (name.clone(), ty.to_plain()))
                                    .collect(),
                            )]),
                        },
                        expr,
                    ),
                    PendingDiag::InterfaceFieldInConstruction {
                        object,
                        name,
                        class_field,
                    } => (
                        TirTypeError::InterfaceFieldRequiresQualifiedConstruction {
                            field_name: name,
                            qualified_name: class_field,
                        },
                        object,
                    ),
                    PendingDiag::UpcastTargetNotInterface { expr, target } => (
                        TirTypeError::InvalidInterfaceUpcastTarget {
                            target: self.finalize_ty(&target).to_plain(),
                        },
                        expr,
                    ),
                    PendingDiag::UpcastNotImplemented {
                        expr,
                        value,
                        interface,
                    } => (
                        TirTypeError::TypeDoesNotImplementInterface {
                            value_type: self.finalize_ty(&value).to_plain(),
                            interface: self.finalize_ty(&interface).to_plain(),
                        },
                        expr,
                    ),
                    PendingDiag::WrongTypeArgArity {
                        expr,
                        callee,
                        expected,
                        got,
                    } => (
                        TirTypeError::WrongTypeArgArity {
                            callee_name: callee,
                            expected,
                            got,
                        },
                        expr,
                    ),
                    PendingDiag::NotCallable { expr, ty } => (
                        TirTypeError::NotCallable {
                            ty: self.finalize_ty(&ty).to_plain(),
                        },
                        expr,
                    ),
                    PendingDiag::ArgCountMismatch {
                        expr,
                        expected,
                        got,
                    } => (TirTypeError::ArgumentCountMismatch { expected, got }, expr),
                    PendingDiag::UnknownNamedArg { expr, name } => {
                        (TirTypeError::UnknownNamedArgument { name }, expr)
                    }
                    PendingDiag::OperatorNotApplicable {
                        expr,
                        interface,
                        lhs,
                        rhs,
                    } => {
                        use baml_compiler2_ast::{BinaryOp, UnaryOp};
                        let lhs = self.finalize_ty(&lhs).to_plain();
                        let rhs = rhs.map(|ty| self.finalize_ty(&ty).to_plain());
                        let error = match (interface, rhs) {
                            ("Index", _) => TirTypeError::NotIndexable { ty: lhs },
                            ("Negate", _) => TirTypeError::InvalidUnaryOp {
                                op: UnaryOp::Neg,
                                operand: lhs,
                            },
                            (_, Some(rhs)) => {
                                let op = match interface {
                                    "Add" => BinaryOp::Add,
                                    "Subtract" => BinaryOp::Sub,
                                    "Multiply" => BinaryOp::Mul,
                                    "Divide" => BinaryOp::Div,
                                    "Remainder" => BinaryOp::Mod,
                                    "BitAnd" => BinaryOp::BitAnd,
                                    "BitOr" => BinaryOp::BitOr,
                                    "BitXor" => BinaryOp::BitXor,
                                    "ShiftLeft" => BinaryOp::Shl,
                                    "ShiftRight" => BinaryOp::Shr,
                                    _ => BinaryOp::Add,
                                };
                                TirTypeError::InvalidBinaryOp { op, lhs, rhs }
                            }
                            (_, None) => TirTypeError::InvalidUnaryOp {
                                op: UnaryOp::Neg,
                                operand: lhs,
                            },
                        };
                        diags.push(TirDiagnostic {
                            error,
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::Expr(expr),
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::InterpolatedMaybeNull { expr, ty } => (
                        TirTypeError::InterpolatedValueMaybeNull {
                            ty: self.finalize_ty(&ty).to_plain(),
                        },
                        expr,
                    ),
                    PendingDiag::UnnecessaryOptionalChain {
                        expr,
                        expr_text,
                        base_text,
                    } => (
                        TirTypeError::UnnecessaryOptionalChaining {
                            expr: expr_text,
                            base: base_text,
                        },
                        expr,
                    ),
                    PendingDiag::RuntimeIdMember { expr, member } => {
                        (TirTypeError::RuntimeIdMemberAccess { member }, expr)
                    }
                    PendingDiag::RuntimeIdCompoundAssignment { expr } => {
                        (TirTypeError::RuntimeIdCompoundAssignment, expr)
                    }
                    PendingDiag::DeadCode {
                        at,
                        unreachable_count,
                    } => {
                        diags.push(TirDiagnostic {
                            error: TirTypeError::DeadCode {
                                after: at,
                                unreachable_count,
                            },
                            severity: DiagnosticSeverity::Warning,
                            primary: DiagnosticLocation::Stmt(at),
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::UnknownPatternField {
                        pat,
                        class_name,
                        field_name,
                        declared,
                    } => {
                        let suggestions = crate::diagnostics::similar_name_suggestions(
                            &field_name,
                            declared.iter(),
                        );
                        diags.push(TirDiagnostic {
                            error: TirTypeError::UnknownClassPatternField {
                                class_name,
                                field_name,
                                suggestions,
                            },
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::Pat(pat),
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::VoidResultUsed { expr } => {
                        (TirTypeError::VoidFunctionResultUsed, expr)
                    }
                    PendingDiag::UninferredCtorParam { expr, var, name } => {
                        if !self.finalize_ty(&var).has_error() {
                            continue;
                        }
                        diags.push(TirDiagnostic {
                            error: TirTypeError::CannotInferTypeParameter { name },
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::Expr(expr),
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::SpawnWithBad {
                        at,
                        expected_input,
                        got,
                    } => {
                        let got = self.finalize_ty(&got).to_plain();
                        // A flow-narrowed literal reads as its base in the
                        // contract wording (`got int`, not `got 7`).
                        let got = match got {
                            baml_type::Ty::Literal(lit, _, attr) => {
                                use baml_base::Literal as Lit;
                                match lit {
                                    Lit::Int(_) => baml_type::Ty::Int { attr },
                                    Lit::Bigint(_) => baml_type::Ty::Bigint { attr },
                                    Lit::Float(_) => baml_type::Ty::Float { attr },
                                    Lit::String(_) => baml_type::Ty::String { attr },
                                    Lit::Bool(_) => baml_type::Ty::Bool { attr },
                                }
                            }
                            other => other,
                        };
                        diags.push(TirDiagnostic {
                            error: TirTypeError::SpawnWithNotATransformer {
                                expected_input: self.finalize_ty(&expected_input).to_plain(),
                                got,
                            },
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::Expr(at),
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::ExtraneousThrows { at, extra_types } => {
                        diags.push(TirDiagnostic {
                            error: TirTypeError::ExtraneousThrowsDeclaration { extra_types },
                            severity: DiagnosticSeverity::Warning,
                            primary: DiagnosticLocation::Expr(at),
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::DeferEscape {
                        stmt,
                        expr,
                        keyword,
                    } => {
                        let primary = match (stmt, expr) {
                            (Some(stmt), _) => DiagnosticLocation::Stmt(stmt),
                            (None, Some(expr)) => DiagnosticLocation::Expr(expr),
                            (None, None) => unreachable!("one anchor is always set"),
                        };
                        diags.push(TirDiagnostic {
                            error: TirTypeError::DeferControlFlowEscape { keyword },
                            severity: DiagnosticSeverity::Error,
                            primary,
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::UnresolvedShorthand {
                        expr,
                        name,
                        suggestions,
                    } => {
                        diags.push(TirDiagnostic {
                            error: TirTypeError::UnresolvedPropertyShorthand { name, suggestions },
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::Expr(expr),
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::ComparisonAlwaysDisjoint { at, op, lhs, rhs } => {
                        diags.push(TirDiagnostic {
                            error: TirTypeError::ComparisonAlwaysDisjoint {
                                op,
                                lhs: self.finalize_ty(&lhs).to_plain(),
                                rhs: self.finalize_ty(&rhs).to_plain(),
                            },
                            severity: DiagnosticSeverity::Warning,
                            primary: DiagnosticLocation::Expr(at),
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::OrderingDifferentTypes { at, op, lhs, rhs } => {
                        diags.push(TirDiagnostic {
                            error: TirTypeError::OrderingDifferentTypes {
                                op,
                                lhs: self.finalize_ty(&lhs).to_plain(),
                                rhs: self.finalize_ty(&rhs).to_plain(),
                            },
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::Expr(at),
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::OrderingRequiresCompare { at, op, ty } => {
                        diags.push(TirDiagnostic {
                            error: TirTypeError::OrderingRequiresCompare {
                                op,
                                ty: self.finalize_ty(&ty).to_plain(),
                            },
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::Expr(at),
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::UnknownObjectField {
                        object,
                        value,
                        class_name,
                        declared,
                        name,
                        shorthand,
                    } => {
                        let suggestions =
                            crate::diagnostics::similar_name_suggestions(&name, declared.iter());
                        let error = if shorthand {
                            TirTypeError::UnknownClassPropertyShorthand {
                                class_name,
                                name,
                                suggestions,
                            }
                        } else {
                            TirTypeError::UnknownClassField {
                                class_name,
                                field_name: name,
                                suggestions,
                            }
                        };
                        diags.push(TirDiagnostic {
                            error,
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::ObjectFieldName(object, value),
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::RuntimeIdArgMismatch { at, got } => {
                        let got = self.finalize_ty(&got);
                        if got.has_error() {
                            continue;
                        }
                        diags.push(TirDiagnostic {
                            error: TirTypeError::RuntimeIdArgumentTypeMismatch {
                                got: got.to_plain(),
                            },
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::Expr(at),
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::DuplicateRuntimeIdArg { at } => {
                        (TirTypeError::DuplicateRuntimeIdArgument, at)
                    }
                    PendingDiag::RuntimeIdArgNotLast { at } => {
                        (TirTypeError::RuntimeIdArgumentMustBeLast, at)
                    }
                    PendingDiag::ThrowsViolation {
                        at,
                        declared,
                        extra,
                    } => {
                        let declared = self.finalize_ty(&declared);
                        let extra = self.finalize_ty(&extra);
                        // A check can fail MID-INFERENCE on open variables
                        // later resolution satisfies; cascades suppress.
                        if declared.has_error()
                            || extra.has_error()
                            || is_subtype_interned(&extra, &declared, &self.facts)
                        {
                            continue;
                        }
                        // A synthetic-effect-param extra traces to a
                        // CALLBACK parameter: the humanized wording names
                        // it (TIR's CallbackThrowsContractViolation).
                        if let TyKind::TypeVar(param, _) = extra.kind()
                            && baml_type::is_synthetic_effect_param(param.name())
                            && let Some(callback) = self.callback_param_for_effect(param)
                        {
                            diags.push(TirDiagnostic {
                                error: TirTypeError::CallbackThrowsContractViolation {
                                    callback_name: callback,
                                    declared: declared.to_plain(),
                                    concrete_throws: None,
                                },
                                severity: DiagnosticSeverity::Error,
                                primary: DiagnosticLocation::Expr(at),
                                related: Vec::new(),
                            });
                            continue;
                        }
                        let extra_types: Vec<String> =
                            crate::throw_facts::flatten_declared_ty_to_facts(&extra.to_plain())
                                .into_iter()
                                .map(|fact| fact.render_user_facing())
                                .collect();
                        if extra_types.is_empty() {
                            continue;
                        }
                        // The error-CHANNEL pin (B-1082): a surviving
                        // violation also records on the mismatch channel
                        // at the thrown value. This runs AFTER the
                        // mismatch render loop above, so E0096 stays the
                        // one user-facing diagnostic for it.
                        result
                            .type_mismatches
                            .entry(at)
                            .or_insert((declared.clone(), extra.clone()));
                        diags.push(TirDiagnostic {
                            error: TirTypeError::ThrowsContractViolation {
                                declared: declared.to_plain(),
                                extra_types,
                            },
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::Expr(at),
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::PatternScrutMismatch {
                        pat,
                        expected,
                        found,
                    } => {
                        diags.push(TirDiagnostic {
                            error: TirTypeError::TypeMismatch {
                                expected: self.finalize_ty(&expected).to_plain(),
                                got: self.finalize_ty(&found).to_plain(),
                            },
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::Pat(pat),
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::UnresolvedPatternName { pat, name } => {
                        diags.push(TirDiagnostic {
                            error: TirTypeError::UnresolvedName { name },
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::Pat(pat),
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::RestNotBinding { pat } => {
                        diags.push(TirDiagnostic {
                            error: TirTypeError::RestSubPatternNotBinding,
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::Pat(pat),
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::OrBindingConflict {
                        pat,
                        name,
                        first,
                        other,
                    } => {
                        diags.push(TirDiagnostic {
                            error: TirTypeError::OrPatternBindingTypeMismatch {
                                name,
                                first_type: self.finalize_ty(&first).to_plain(),
                                other_type: self.finalize_ty(&other).to_plain(),
                            },
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::Pat(pat),
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::GenericDestructureNoArgs { pat, class_name } => {
                        diags.push(TirDiagnostic {
                            error: TirTypeError::GenericClassDestructureRequiresTypeArgs {
                                class_name,
                            },
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::Pat(pat),
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::BodyAnnot { type_ref, kind } => {
                        diags.push(TirDiagnostic {
                            error: crate::lower::lowering_diag_error(&kind),
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::TypeRef(type_ref),
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::RefutableLet { pat, context } => {
                        diags.push(TirDiagnostic {
                            error: TirTypeError::RefutablePatternInLet { context },
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::Pat(pat),
                            related: Vec::new(),
                        });
                        continue;
                    }
                };
                let severity =
                    if matches!(error, TirTypeError::UnreachableArm) && unreachable_is_warning {
                        DiagnosticSeverity::Warning
                    } else {
                        DiagnosticSeverity::Error
                    };
                diags.push(TirDiagnostic {
                    error,
                    severity,
                    primary: DiagnosticLocation::Expr(expr),
                    related: Vec::new(),
                });
            }
            // The finish fixpoint may re-attempt a failing obligation
            // once per round - identical findings collapse to one.
            diags.sort_by_key(|d| match d.primary {
                DiagnosticLocation::Expr(id)
                | DiagnosticLocation::ExprMember(id)
                | DiagnosticLocation::ExprSegment(id, _)
                | DiagnosticLocation::ObjectFieldName(_, id) => (0u8, u32::from(id.into_raw())),
                DiagnosticLocation::Stmt(id) => (1, u32::from(id.into_raw())),
                DiagnosticLocation::TypeAnnot(id) => (2, u32::from(id.into_raw())),
                DiagnosticLocation::Pat(id) => (4, u32::from(id.into_raw())),
                DiagnosticLocation::TypeRef(id) => (5, u32::from(id.into_raw())),
                DiagnosticLocation::Span(range) => (3, u32::from(range.start())),
            });
            diags.dedup();
            result.diagnostics = diags;
        }
        // The writeback pass covers every recorded table (rustc's
        // `resolve_type_vars_in_body`): the virtual-field VIEW and the
        // path ladders' per-segment types carry types.
        for resolution in result.member_resolutions.values_mut() {
            if let MemberResolution::InterfaceVirtualField { view, .. } = resolution {
                *view = self.finalize_ty(view);
            }
        }
        for path in result.path_resolutions.values_mut() {
            for segment in &mut path.segments {
                segment.ty = self.finalize_ty(&segment.ty);
                if let Some(MemberResolution::InterfaceVirtualField { view, .. }) =
                    &mut segment.resolution
                {
                    *view = self.finalize_ty(view);
                }
            }
        }
        for plan in result.call_plans.values_mut() {
            for ty in &mut plan.type_args {
                *ty = self.finalize_ty(ty);
            }
        }
        for adjustments in result.expr_adjustments.values_mut() {
            for adjustment in adjustments.iter_mut() {
                adjustment.target = self.finalize_ty(&adjustment.target);
            }
        }
        result
    }

    /// One recorded type, finalized: solved variables substituted,
    /// survivors erased to the local Error sentinel, unions
    /// re-canonicalized (skipped for error-carrying types - the canonical
    /// algebra is Error-tolerant and would collapse them arbitrarily).
    fn finalize_ty(&mut self, ty: &Ty) -> Ty {
        let resolved = self.table.resolve_completely(ty);
        let erased = erase_infer(&resolved);
        if erased.has_error() {
            // Errors stay LOCAL, but the cleanup pass still runs: the
            // union constructor above is identity-safe on error
            // members, so one erased var no longer leaves the whole
            // tree's unions syntactic.
            return self.canonicalize_unions(&erased);
        }
        let reduced = self.reduce_projections(&erased, PROJECTION_FINALIZE_FUEL);
        self.canonicalize_unions(&reduced)
    }

    /// Post-substitution projection normalization (rustc's
    /// instantiate-then-normalize; rust-analyzer normalizes projections at
    /// the result boundary the same way): every projection the oracle can
    /// determine reduces, so results and renders show what the type IS -
    /// `(IntStore as Store).Item` finalizes as `int`. Targeted rather than
    /// full canonicalization, which would also expand nominal aliases;
    /// renders keep those by design.
    fn reduce_projections(&self, ty: &Ty, fuel: u32) -> Ty {
        if fuel == 0 || !ty.has_projection() {
            return ty.clone();
        }
        let rebuilt = Ty::intern(
            ty.kind()
                .map_children(|child| self.reduce_projections(child, fuel)),
        );
        // Node-local normalization (rustc's lazy normalize): a projection
        // reduces when ITS OWN subtree is ground - var-carrying siblings
        // elsewhere in the type are irrelevant to this lookup. A
        // var-carrying projection stays (the oracle's plain conversion
        // erases inference vars).
        if rebuilt.has_infer() {
            return rebuilt;
        }
        if let TyKind::AssociatedTypeProjection {
            base,
            interface,
            member,
            ..
        } = rebuilt.kind()
        {
            let plain_base = base.to_plain();
            let plain_interface = baml_type::Interface::new(
                interface.name.clone(),
                interface.generics.iter().map(Ty::to_plain).collect(),
                interface
                    .associated_types
                    .iter()
                    .map(|(name, pin)| (name.clone(), pin.to_plain()))
                    .collect(),
            );
            if let baml_type::normalize::ProjectionStep::Reduced(step) =
                baml_type::normalize::TypeContext::project(
                    &self.facts,
                    &plain_base,
                    &plain_interface,
                    member,
                    fuel,
                )
            {
                return self.reduce_projections(&Ty::from_plain(&step), fuel - 1);
            }
        }
        rebuilt
    }

    /// The element a `for (let x in coll)` loop yields for a non-list
    /// collection: the projection `(coll as baml.iter.Iterable).Item`,
    /// reduced through the oracle - r-a's for-loop shape (the element IS
    /// `<C as IntoIterator>::Item`, resolved by trait solving; here the
    /// I5 projection candidate order, so param-env bounds on a rigid `T`
    /// and `implements` blocks on classes both answer). A projection the
    /// oracle cannot determine stays the error sentinel: iteration over
    /// a type with no `Iterable` evidence has no element type.
    fn iteration_item(&mut self, collection: &Ty, at: ExprId) -> Ty {
        let iterable = baml_type::interned::InterfaceRef::new(
            baml_type::TypeName::new(
                baml_type::Name::new("baml"),
                vec![baml_type::Name::new("iter")],
                baml_type::Name::new("Iterable"),
            ),
            Box::new([]),
            Vec::new(),
        );
        // rustc's for-desugar is an `into_iter` CALL: the iterability
        // obligation registers against the collection (selection
        // forces its vars; an unsatisfiable subject records the
        // mismatch at the collection expression), and the element is
        // the Item projection - resolved through the same structure
        // demand receivers use, deferred while vars remain like any
        // other projection.
        self.register_obligation(obligations::Obligation::Implements {
            ty: collection.clone(),
            interface: iterable.clone(),
            at,
            not_concrete_rejects: false,
        });
        let projection = Ty::intern(TyKind::AssociatedTypeProjection {
            base: collection.clone(),
            interface: iterable,
            member: baml_type::Name::new("Item"),
            attr: baml_type::TyAttr::default(),
        });
        let reduced = self.structurally_resolve(&projection);
        if reduced.has_projection() && !reduced.has_infer() {
            // Ground and irreducible: genuinely not iterable (the
            // failed selection reports at the collection).
            return Ty::error();
        }
        reduced
    }

    /// Rebuilds `ty` with every union node in canonical form, bottom-up.
    /// Idempotent on already-canonical types; repairs the syntactic unions
    /// `union_of` built while a member still carried a variable.
    ///
    /// Presentation order: `null` moves LAST - the optional convention
    /// (`T?` reads `T | null`, the spec's own notation). The shared
    /// canonical sort is an internal detail load-bearing for the TIR-era
    /// tier snapshots, so the convention applies at this crate's result
    /// boundary; it folds into the shared algebra at cutover (S16).
    fn canonicalize_unions(&self, ty: &Ty) -> Ty {
        match ty.kind() {
            TyKind::Union(members, _) => {
                let members: Vec<Ty> = members
                    .iter()
                    .map(|member| self.canonicalize_unions(member))
                    .collect();
                // Error-carrying unions clean up STRUCTURALLY only:
                // the canonical algebra's equivalence treats the Error
                // sentinel as bidirectionally compatible (checking's
                // cascade suppression), so a container member like
                // `!error[]` would MERGE into `int[]` and vanish.
                // rustc's discipline is the opposite for identity -
                // `TyKind::Error` equals only itself in canonical
                // forms; compat lives in the relate layer alone.
                // Until the shared algebra splits those roles (S16,
                // when TIR stops consuming it), flatten/dedup/collapse
                // here and skip absorption.
                if members.iter().any(Ty::has_error) {
                    return syntactic_union(&members);
                }
                let joined = canonical_union_interned(&members, &self.facts);
                match joined.kind() {
                    TyKind::Union(members, attr) => {
                        let (mut ordered, nulls): (Vec<Ty>, Vec<Ty>) = members
                            .iter()
                            .cloned()
                            .partition(|member| !matches!(member.kind(), TyKind::Null { .. }));
                        ordered.extend(nulls);
                        Ty::intern(TyKind::Union(ordered.into(), attr.clone()))
                    }
                    _ => joined,
                }
            }
            _ => Ty::intern(
                ty.kind()
                    .map_children(|child| self.canonicalize_unions(child)),
            ),
        }
    }

    /// Derives solutions from accumulated bounds, iterating because one
    /// resolution can make another class's bounds ground. Returns whether
    /// anything solved.
    fn resolve_bounded_vars(&mut self, tier: SolveTier) -> bool {
        let mut any = false;
        loop {
            let mut progressed = false;
            for (var, bounds) in self.table.unsolved_bounded_vars() {
                if self.try_solve_bounded_var_tiered(var, &bounds, tier) {
                    progressed = true;
                }
            }
            if !progressed {
                break;
            }
            any = true;
        }
        any
    }

    /// The tier gate in front of [`InferenceContext::try_solve_bounded_var`]:
    /// the eager tier solves only FULLY-ground classes; a class with
    /// var-carrying bounds waits, because a sibling those bounds mention
    /// may still be solvable and committing now would decide from partial
    /// information (rustc runs type-variable fallback only once
    /// fulfillment is quiescent; stalled obligations re-run instead of
    /// forcing). The ground-subset tier is that fallback: it runs only
    /// when a whole finish round made no other progress, where it remains
    /// the operator-deadlock breaker.
    fn try_solve_bounded_var_tiered(
        &mut self,
        var: baml_type::interned::InferVar,
        bounds: &unify::VarBounds,
        tier: SolveTier,
    ) -> bool {
        if tier == SolveTier::Ground {
            let fully_ground = bounds
                .lowers
                .iter()
                .chain(bounds.uppers.iter())
                .all(|ty| !self.table.resolve_completely(ty).has_infer());
            if !fully_ground {
                return false;
            }
        }
        self.try_solve_bounded_var(var, bounds)
    }

    /// One bounded class's resolution step - the shared core of the
    /// finish fixpoint and [`InferenceContext::structurally_resolve`].
    /// Bounds must be ground to decide; the GROUND SUBSET decides (the
    /// obligation-deadlock rule): bounds still carrying variables move
    /// to the deferred residue for post-hoc verification instead of
    /// blocking the class forever - an operator obligation's output may
    /// bound the very variable its operand waits on. Returns whether
    /// the class was solved (or aliased).
    fn try_solve_bounded_var(
        &mut self,
        var: baml_type::interned::InferVar,
        bounds: &unify::VarBounds,
    ) -> bool {
        // A sibling's generalization step may have ALIASED this var into
        // a solved class since the caller collected its list; acting on
        // it again would union a second solution onto a Known root.
        if self.table.is_solved(var) {
            return false;
        }
        let (lowers, deferred_lowers): (Vec<Ty>, Vec<Ty>) = bounds
            .lowers
            .iter()
            .map(|ty| self.table.resolve_completely(ty))
            .partition(|ty| !ty.has_infer());
        let (uppers, deferred_uppers): (Vec<Ty>, Vec<Ty>) = bounds
            .uppers
            .iter()
            .map(|ty| self.table.resolve_completely(ty))
            // A TOP-TYPE upper is no constraint (everything satisfies
            // it) and therefore no EVIDENCE: the minimum-upper meet must
            // not commit a class to `unknown` from a vacuous bound while
            // real evidence is still en route (`reduce`'s ?E2 carries
            // only the declared `throws unknown` check when the lambda's
            // `never` has not landed yet). Informative uppers keep the
            // meet (B-898's `?D <= Generate<int>` solves the class).
            .filter(|ty| !matches!(ty.kind(), TyKind::Unknown { .. }))
            .partition(|ty| !ty.has_infer());
        if lowers.is_empty() && uppers.is_empty() {
            // GENERALIZATION (rustc's combine/generalize shape):
            // a var whose only information is one var-carrying
            // lower (or several identical ones) and no uppers is
            // an ALIAS of that lower - solving it is
            // occurs-guarded union-find aliasing, not a
            // premature meet, and it is what lets impl SELECTION
            // see the concrete head behind a call argument
            // (B-898: `?D` alias `Generate<?F>`). DISTINCT
            // var-carrying lowers stay deferred - that is the
            // operator-deadlock rule, untouched. Runs in the
            // finish fixpoint and at STRUCTURE points: rustc unifies
            // the pair the moment the argument checks, so a structure
            // demand commits the alias the same way - a later
            // conflicting bound is a mismatch, not a join.
            if deferred_uppers.is_empty()
                && let Some((first, rest)) = deferred_lowers.split_first()
                && rest.iter().all(|lower| lower == first)
            {
                // No widening here: this tier is occurs-guarded
                // ALIASING, not a meet, and a deferred (var-carrying)
                // lower can never be a top-level fresh literal anyway.
                let alias = first.clone();
                if self.table.unify(&Ty::infer_var(var), &alias).is_ok() {
                    return true;
                }
            }
            return false;
        }
        let var_ty = Ty::infer_var(var);
        for deferred in deferred_lowers {
            self.deferred_subs.push((deferred, var_ty.clone(), None));
        }
        for deferred in deferred_uppers {
            self.deferred_subs.push((var_ty.clone(), deferred, None));
        }
        let solution = if lowers.is_empty() {
            // No values flowed in: the MINIMUM upper is the meet
            // when one exists (BAML has no intersections, so
            // incomparable uppers have no representable meet -
            // unresolved, erased at finalize).
            let minimum = uppers.iter().find(|candidate| {
                uppers
                    .iter()
                    .all(|upper| is_subtype_interned(candidate, upper, &self.facts))
            });
            match minimum {
                Some(minimum) => minimum.clone(),
                None => return false,
            }
        } else {
            // The MAXIMUM lower is the join when one exists - the
            // mirror of the uppers' minimum-meet rule, TS's
            // best-common-supertype. Raw candidates first, so an
            // exact literal demand keeps the literal (`pair(three(),
            // 3)` is `3`); ruling 1's fresh widening is the RETRY
            // when raw candidates have no maximum (`pair(1, 2)`
            // widens to `int`), never a way to lose one. Still no
            // maximum - genuinely incompatible arguments - is a
            // mismatch (Error until the S17 diagnostic), and the
            // choice is checked against every upper.
            let widened: Vec<Ty> = lowers.iter().map(|ty| self.widen_fresh(ty)).collect();
            let widening: Vec<bool> = lowers
                .iter()
                .zip(&widened)
                .map(|(lower, wide)| lower != wide)
                .collect();
            let facts = &self.facts;
            let maximum_of = |candidates: &[Ty]| -> Option<Ty> {
                let subsumes_all = |candidate: &&Ty| {
                    candidates
                        .iter()
                        .all(|lower| is_subtype_interned(lower, candidate, facts))
                };
                // Prefer a non-widening witness: the solution's
                // freshness decides binding-site widening later, and a
                // rigid `3` holding a fresh `3` must keep the demand
                // rigid.
                candidates
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| !widening[*index])
                    .map(|(_, candidate)| candidate)
                    .find(subsumes_all)
                    .or_else(|| candidates.iter().find(subsumes_all))
                    .cloned()
            };
            // An ALL-widening candidate set widens by DEFAULT
            // (ruling 1's generation-site rule: `push(1)` infers
            // `int[]`, and a fresh `100 | 999` join widens the same
            // way); a non-widening candidate anchors the raw literals
            // instead (`pair(three(), 3)` keeps `3`) - TS's split
            // exactly. A default never overrides an actual
            // constraint: the call's expectation reaches the var as
            // an upper bound (r-a's check_call_arguments unifies
            // expected_output with formal_output BEFORE the args
            // commit, expr.rs:1987; rustc applies literal fallback
            // only to otherwise-unconstrained vars), so when the
            // widened join violates an upper, the raw literals get
            // their turn instead of manufacturing a conflict
            // (`-> 42 { id(42) }` is `42`, not Error).
            let maximum = if widening.iter().all(|&flag| flag) {
                maximum_of(&widened)
                    .filter(|max| {
                        uppers
                            .iter()
                            .all(|upper| is_subtype_interned(max, upper, facts))
                    })
                    .or_else(|| maximum_of(&lowers))
            } else {
                maximum_of(&lowers).or_else(|| maximum_of(&widened))
            };
            match maximum {
                Some(maximum)
                    if uppers
                        .iter()
                        .all(|upper| is_subtype_interned(&maximum, upper, &self.facts)) =>
                {
                    maximum
                }
                _ => {
                    // No join: genuinely incompatible demands. An empty
                    // CONTAINER's slot follows TIR's establishment-order
                    // rule - the FIRST ground demand wins so the binding
                    // stays usable and every later incompatible demand
                    // (and any violated upper) reports through the
                    // provisional re-check at finalize. Any other var (a
                    // call instantiation) fails resolution instead
                    // (ruling 1: disagreeing lowers reject, not join).
                    if self.table.is_establishment_var(var) {
                        widened.first().cloned().unwrap_or_else(Ty::error)
                    } else {
                        return false;
                    }
                }
            }
        };
        self.table.solve(var, solution);
        true
    }

    /// rustc's `structurally_resolve_type`: where the walk needs a
    /// type's STRUCTURE now (method receivers, field bases, index
    /// subjects), a still-unsolved head var is forced from the bounds
    /// it has accumulated SO FAR - the same ground decision the finish
    /// fixpoint applies, mirroring rustc's resolve-vars-then-demand-
    /// structure order. Committing early is rustc's semantics: the
    /// structure point fixes the type, and a later conflicting bound
    /// becomes a mismatch rather than a wider join - the occurs-guarded
    /// ALIAS tier included (rustc unifies `?T := Vec<?w>` the moment the
    /// argument checks; the structure demand commits the same way). A
    /// class with no evidence yet stays a var; the caller's lookup then
    /// misses as before (the "type annotations needed" family, S17's
    /// diagnostic).
    fn structurally_resolve(&mut self, ty: &Ty) -> Ty {
        let resolved = self.table.resolve_completely(ty);
        let mut resolved = if let TyKind::Infer { var: Some(var), .. } = resolved.kind() {
            let var = *var;
            let bounds = self.table.var_bounds(var);
            if self.try_solve_bounded_var(var, &bounds) {
                self.table.resolve_completely(&resolved)
            } else {
                resolved
            }
        } else {
            resolved
        };
        // A projection blocked behind inference vars cannot PROBE (a
        // projection is no impl subject) - the base must resolve before
        // the oracle can reduce. Force the occurring vars from their
        // accumulated bounds: the same demand-point commitment head
        // vars get (rustc would have unified them already). Commits draw
        // on EVIDENCE - lowers, aliasing, informative uppers; top-type
        // uppers are filtered in the solve itself, so a vacuous
        // declared-throws check cannot decide an effect class here.
        if resolved.has_infer() && resolved.has_projection() {
            resolved = self.force_occurring_vars(&resolved);
        }
        // rustc's `structurally_resolve_type` NORMALIZES as well as
        // resolving: a ground reducible projection is not structure -
        // `(T as Source).Item` coming back from a call IS `string`
        // wherever a consumer demands shape. Var-carrying projections
        // stay (the oracle's plain conversion erases inference vars);
        // they relate lazily through the deferred residue instead.
        if resolved.has_projection() && !resolved.has_infer() {
            let reduced = self.reduce_projections(&resolved, PROJECTION_FINALIZE_FUEL);
            return self.expand_alias_ty(&reduced);
        }
        // WEAK aliases normalize here too (rustc's `Alias::Weak` in
        // `structurally_resolve_type`): a structure consumer never
        // sees the nominal wrapper, so no consumer can forget to
        // expand. Recorded types keep the written name - this is the
        // demanded STRUCTURE, not the render.
        if matches!(resolved.kind(), TyKind::TypeAlias(..)) {
            return self.expand_alias_ty(&resolved);
        }
        resolved
    }

    /// Forces every inference var OCCURRING in `ty` from its
    /// accumulated bounds, to fixpoint (one solution can ground
    /// another's bounds) - the demand-point commitment shared by
    /// projection bases and match scrutinees, where deferral is
    /// impossible (rustc orders around this by running projection
    /// selection and match usefulness after inference; a one-pass walk
    /// commits at the demand instead).
    fn force_occurring_vars(&mut self, ty: &Ty) -> Ty {
        let mut resolved = self.table.resolve_completely(ty);
        loop {
            let mut vars = Vec::new();
            collect_infer_vars(&resolved, &mut vars);
            vars.sort_by_key(|var| var.index());
            vars.dedup();
            let mut progressed = false;
            for var in vars {
                let bounds = self.table.var_bounds(var);
                if self.try_solve_bounded_var(var, &bounds) {
                    progressed = true;
                }
            }
            if !progressed {
                break;
            }
            resolved = self.table.resolve_completely(&resolved);
            if !resolved.has_infer() {
                break;
            }
        }
        resolved
    }

    /// The name of the parameter whose function type carries `effect` in
    /// throws position - the callback a synthetic effect param belongs to.
    fn callback_param_for_effect(
        &mut self,
        effect: &baml_type::ParamTy,
    ) -> Option<baml_type::Name> {
        let function = self.body_owner?;
        let data = baml_compiler2_ppir::item_data::function_data(self.db, function);
        for (index, param_ty) in self.param_tys.iter().enumerate() {
            let resolved = self.table.resolve_completely(param_ty);
            let TyKind::Function { throws, .. } = resolved.kind() else {
                continue;
            };
            if matches!(throws.kind(), TyKind::TypeVar(p, _) if p == effect) {
                return data.params.get(index).map(|param| param.name.clone());
            }
        }
        None
    }

    /// Whether AST lowering marked `expr` as the elided value of a
    /// property shorthand (`{ key }`). The parser's marker is the only
    /// authority: `{ "key": key }` lowers to the identical hir shape and
    /// is NOT shorthand.
    fn is_property_shorthand(&self, expr: ExprId) -> bool {
        self.shorthand_exprs
            .get_or_init(|| {
                self.body_owner_id
                    .and_then(|owner| baml_compiler2_ppir::body_source_map(self.db, owner))
                    .map(|source_map| {
                        source_map
                            .property_shorthand_exprs
                            .iter()
                            .copied()
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .contains(&expr)
    }

    /// Every value name visible where `at` sits (params and bindings of
    /// every form, through the ancestor chain of the expression's OWN
    /// scope - not the body's, which would miss everything bound inside a
    /// nested block) - the near-match candidate pool for shorthand
    /// suggestions.
    fn local_binding_names(&self, at: ExprId) -> Vec<baml_type::Name> {
        let mut names = Vec::new();
        let index = self.index;
        let scope = self
            .metadata_key(at)
            .and_then(|key| index.expression_scope(key))
            .or(self.current_scope);
        let Some(scope) = scope else {
            return names;
        };
        for ancestor in index.ancestor_scopes(scope) {
            let bindings = &index.scope_bindings[ancestor.index() as usize];
            names.extend(bindings.bindings.iter().map(|b| b.name.clone()));
            names.extend(
                bindings
                    .params
                    .iter()
                    .map(|(name, _)| baml_type::Name::new(name.as_str())),
            );
        }
        names
    }

    /// The pattern-reachability oracle over this scope (TIR's
    /// `pattern_overlap_verdict`, its inputs rebuilt from the hir world):
    /// can `pat` and `member` share a value under some realization of the
    /// in-scope rigid params? `Yes`/`Unknown` = possible, `No` = provably
    /// dead. Trust a `No` only when both inputs pass
    /// `baml_type::unify::all_typevars_within` for this scope's frame.
    /// The failing bound of an ALMOST-matching blanket impl for an
    /// interface expectation - `None` when no impl's implementor shape
    /// matches the value, or every matching impl's bounds hold (the
    /// mismatch then has some other cause). Diagnostic refinement only;
    /// never consulted on a passing check.
    fn first_failing_blanket_bound(&self, actual: &Ty, expected: &Ty) -> Option<baml_type::Ty> {
        if actual.has_infer() || expected.has_infer() {
            return None;
        }
        let file = self.owner_file?;
        let info = baml_compiler2_hir::file_package::file_package(self.db, file);
        let pkg = baml_compiler2_hir::package::PackageId::new(self.db, info.package);
        let aliases = self.overlap_alias_map();
        crate::interfaces::first_failing_impl_bound(
            self.db,
            pkg,
            &actual.to_plain(),
            &expected.to_plain(),
            aliases,
            |a, b| baml_type::normalize::is_subtype(a, b, &self.facts),
        )
        .map(|(_param, bound, _actual_arg)| bound)
    }

    fn pattern_overlap_verdict(
        &self,
        pat: &baml_type::Ty,
        member: &baml_type::Ty,
    ) -> baml_type::unify::Overlap {
        use baml_type::normalize::TypeContext as _;
        let aliases = self.overlap_alias_map();
        let enum_variants = |qtn: &baml_type::QualifiedTypeName| self.facts.enum_variants(qtn);
        let implements = |ty: &baml_type::Ty, iface: &baml_type::Interface| {
            self.facts.implements_interface(ty, iface)
        };
        baml_type::pattern_overlap::pattern_overlap(
            pat,
            member,
            &baml_type::pattern_overlap::PatternOverlapEnv {
                vars: self.lower.generic_params(),
                bounds: self.facts.bounds(),
                aliases,
                enum_variants: &enum_variants,
                implements: &implements,
            },
        )
    }

    /// The oracle's alias input: every alias visible to the body's package
    /// (own plus dependency closure), bodies pre-folded to `nf`'s canonical
    /// union form - see `baml_type::unify` for why raw bodies mis-decide
    /// alias-obscured unions at invariant positions.
    fn overlap_alias_map(
        &self,
    ) -> &std::collections::HashMap<baml_type::QualifiedTypeName, baml_type::Ty> {
        self.overlap_aliases.get_or_init(|| {
            use baml_compiler2_hir::contributions::Definition;
            use baml_type::normalize::TypeContext as _;
            let mut aliases = std::collections::HashMap::new();
            let Some(file) = self.owner_file else {
                return aliases;
            };
            let info = baml_compiler2_hir::file_package::file_package(self.db, file);
            let pkg = baml_compiler2_hir::package::PackageId::new(self.db, info.package);
            let mut packages = vec![pkg];
            packages.extend(baml_compiler2_hir::package::package_dependency_closure(
                self.db, pkg,
            ));
            for pkg_id in packages {
                let items = baml_compiler2_ppir::package_items(self.db, pkg_id);
                for ns in items.namespaces.values() {
                    for (name, def) in &ns.types {
                        let Definition::TypeAlias(loc) = def else {
                            continue;
                        };
                        let def_info = baml_compiler2_hir::file_package::file_package(
                            self.db,
                            loc.file(self.db),
                        );
                        let qtn = baml_type::QualifiedTypeName::new(
                            def_info.package,
                            def_info.namespace_path,
                            name.clone(),
                        );
                        aliases.entry(qtn).or_insert_with(|| {
                            crate::lower::type_alias_value(self.db, *loc).to_plain()
                        });
                    }
                }
            }
            let enum_variants = |qtn: &baml_type::QualifiedTypeName| self.facts.enum_variants(qtn);
            for body in aliases.values_mut() {
                *body = baml_type::unify::nf(body, &enum_variants);
            }
            aliases
        })
    }

    /// Re-drives bounds accumulated on classes DIRECT unification has since
    /// solved (`?U := Future<?T, ?E>` while `Future<null, never>` still sat
    /// in `?U`'s lower bounds - the map-lambda/`future.all` shape): binding
    /// a variable discharges its pending evidence by replaying each bound
    /// against the solution - rustc's fulfillment re-drive on inference
    /// progress - never by dropping it. Replay failures follow the
    /// anchorless-drop rule (the depositing site already checked itself).
    fn replay_solved_class_bounds(&mut self) -> bool {
        let solved = self.table.take_solved_class_bounds();
        let mut progressed = false;
        for (solution, bounds) in solved {
            for lower in bounds.lowers {
                let _ = self.sub(&lower, &solution);
                progressed = true;
            }
            for upper in bounds.uppers {
                let _ = self.sub(&solution, &upper);
                progressed = true;
            }
        }
        progressed
    }

    /// Re-drives the deferred `Sub` residue inside the finish fixpoint:
    /// a pair whose counterpart has since RESOLVED decomposes through
    /// `sub` again, landing bounds on the still-open side (`?E[] <= ?T`
    /// re-drives as `?E[] <= int[]` once `?T` solves, so `?E` picks up
    /// its upper). Pairs open on both sides wait for a later round;
    /// ground pairs retire; a pair `sub` re-defers unchanged is not
    /// progress. Whatever remains at quiescence is conservatively
    /// dropped (S17 diagnostics and I4 obligations take over).
    fn drain_deferred_subs(&mut self) -> bool {
        let deferred = std::mem::take(&mut self.deferred_subs);
        let mut progressed = false;
        for (actual, expected, anchor) in deferred {
            let actual = self.table.resolve_completely(&actual);
            let expected = self.table.resolve_completely(&expected);
            if actual.has_infer() && expected.has_infer() {
                self.deferred_subs.push((actual, expected, anchor));
                continue;
            }
            if !actual.has_infer() && !expected.has_infer() {
                // A failed post-hoc bound reports at the expr that
                // deposited it (check_expr's anchor rides the pair):
                // `let m = []; m = {}` retires `map <: list` here, the
                // only place both sides are ground. Anchorless pairs
                // (the VarBounds flush) still drop - threading THEIR
                // provenance is VarBounds' business. The quiescence
                // tiering makes a failure here reachable only for
                // genuinely ill-typed programs.
                if !is_subtype_interned(&actual, &expected, &self.facts)
                    && let Some(anchor) = anchor
                {
                    self.result
                        .type_mismatches
                        .entry(anchor)
                        .or_insert((expected, actual));
                }
                continue;
            }
            let saved_anchor = std::mem::replace(&mut self.obligation_anchor, anchor);
            let before = self.deferred_subs.len();
            let _ = self.sub(&actual, &expected);
            self.obligation_anchor = saved_anchor;
            let re_deferred = self.deferred_subs[before..]
                .iter()
                .any(|(a, e, _)| *a == actual && *e == expected);
            if !re_deferred {
                progressed = true;
            }
        }
        progressed
    }
}

/// Replaces every unsolved inference var with a RIGID placeholder (a
/// `TypeVar` named after the var, same var -> same placeholder) for the
/// quiescence judgment: rigid vars are opaque to the subtype oracle, so
/// the substituted pair fails only when NO solution could satisfy it.
fn skolemize_infer(ty: &Ty) -> Ty {
    if !ty.has_infer() {
        return ty.clone();
    }
    if let TyKind::Infer {
        var: Some(var),
        attr,
    } = ty.kind()
    {
        return Ty::intern(TyKind::TypeVar(
            baml_type::ParamTy::new(
                u32::MAX - var.index(),
                baml_type::Name::new(format!("?{}", var.index())),
            ),
            attr.clone(),
        ));
    }
    Ty::intern(ty.kind().map_children(skolemize_infer))
}

/// Every unsolved inference var occurring in `ty`, for structural
/// resolution's projection-base forcing.
fn collect_infer_vars(ty: &Ty, out: &mut Vec<baml_type::interned::InferVar>) {
    if !ty.has_infer() {
        return;
    }
    if let TyKind::Infer { var: Some(var), .. } = ty.kind() {
        out.push(*var);
    }
    baml_type::interned::for_each_child(ty.kind(), |child| collect_infer_vars(child, out));
}

/// Whether `source` and `target` share a head CONSTRUCTOR - the gate
/// for recursing into a union target's single structured constituent
/// (only pairs plain unification could relate member-wise).
fn same_head_constructor(source: &Ty, target: &Ty) -> bool {
    match (source.kind(), target.kind()) {
        (TyKind::List(..), TyKind::List(..))
        | (TyKind::Map { .. }, TyKind::Map { .. })
        | (TyKind::Future(..), TyKind::Future(..)) => true,
        (TyKind::Class(a, a_args, _), TyKind::Class(b, b_args, _)) => {
            a == b && a_args.len() == b_args.len()
        }
        (TyKind::Interface(a, a_args, _, _), TyKind::Interface(b, b_args, _, _)) => {
            a == b && a_args.len() == b_args.len()
        }
        (
            TyKind::Function {
                params: a_params, ..
            },
            TyKind::Function {
                params: b_params, ..
            },
        ) => a_params.len() == b_params.len(),
        _ => false,
    }
}

/// An instance-accessed METHOD as a value is receiver-BOUND: the access
/// consumes the `self` slot (`greeter.handle` is `(req) -> Response` -
/// the runtime hands out a bound closure). Static/UFCS spellings
/// (`Type.method`) keep the full signature; there the receiver arrives
/// as the written first argument. Non-methods pass through untouched.
/// Whether `param` occurs anywhere inside `ty` (the phantom-param test
/// for constructor inference slots).
fn ty_mentions_param(ty: &Ty, param: &baml_type::ParamTy) -> bool {
    fn walk(ty: &Ty, param: &baml_type::ParamTy, found: &mut bool) {
        if *found {
            return;
        }
        if matches!(ty.kind(), TyKind::TypeVar(p, _) if p == param) {
            *found = true;
            return;
        }
        baml_type::interned::for_each_child(ty.kind(), |child| {
            walk(child, param, found);
        });
    }
    let mut found = false;
    walk(ty, param, &mut found);
    found
}

fn bind_receiver(fn_ty: Ty) -> Ty {
    let TyKind::Function {
        params,
        ret,
        throws,
        attr,
    } = fn_ty.kind()
    else {
        return fn_ty;
    };
    let binds = params.first().is_some_and(|param| {
        param
            .name
            .as_ref()
            .is_some_and(|name| name.as_str() == "self")
    });
    if !binds {
        return fn_ty;
    }
    Ty::intern(TyKind::Function {
        params: params[1..].to_vec().into_boxed_slice(),
        ret: ret.clone(),
        throws: throws.clone(),
        attr: attr.clone(),
    })
}

/// A resolved function as a first-class value: its signature instantiated
/// into an interned function type. Shared by direct calls (turbofish-aware
/// instantiation) and value-position references (fresh-var instantiation).
fn function_value_ty(signature: &crate::lower::FunctionSignature, instantiation: &[Ty]) -> Ty {
    let params: Box<[baml_type::interned::FunctionParam]> = signature
        .params
        .iter()
        .map(|param| baml_type::interned::FunctionParam {
            name: Some(param.name.clone()),
            ty: substitute_params(&param.ty, instantiation),
            mode: if param.has_default {
                baml_type::FunctionParamMode::Optional
            } else {
                baml_type::FunctionParamMode::Required
            },
        })
        .collect();
    Ty::intern(TyKind::Function {
        params,
        ret: substitute_params(&signature.ret, instantiation),
        throws: substitute_params(&signature.throws, instantiation),
        attr: TyAttr::default(),
    })
}

fn compiled_function_value_ty(
    function: &baml_package_interface::ExportedFunction,
    instantiation: &[Ty],
) -> Ty {
    let params = function
        .params
        .iter()
        .map(|param| baml_type::interned::FunctionParam {
            name: param.name.clone(),
            ty: substitute_params(&Ty::from_plain(&param.ty), instantiation),
            mode: param.mode,
        })
        .collect();
    Ty::intern(TyKind::Function {
        params,
        ret: substitute_params(&Ty::from_plain(&function.return_type), instantiation),
        throws: substitute_params(&Ty::from_plain(&function.callable_throws), instantiation),
        attr: TyAttr::default(),
    })
}

/// Replaces every `Infer` node (unsolved variable or hole) with the Error
/// sentinel, in place - the finalize half of rulings 2/3.
fn erase_infer(ty: &Ty) -> Ty {
    if !ty.has_infer() {
        return ty.clone();
    }
    if matches!(ty.kind(), TyKind::Infer { .. }) {
        return Ty::error();
    }
    Ty::intern(ty.kind().map_children(erase_infer))
}

/// A fresh literal widens to its base primitive at binding sites (the spec's
/// TypeScript-style widening); everything else passes through. Top-level
/// only - container-element widening arrives with the join machinery.
fn widen_fresh_literal(ty: &Ty) -> Ty {
    match ty.kind() {
        TyKind::Literal(literal, Freshness::Fresh, attr) => {
            Ty::intern(literal_base(literal, attr.clone()))
        }
        _ => ty.clone(),
    }
}

/// The base primitive a literal type belongs to.
pub(crate) fn literal_base(literal: &Literal, attr: TyAttr) -> TyKind {
    match literal {
        Literal::Int(_) => TyKind::Int { attr },
        Literal::Bigint(_) => TyKind::Bigint { attr },
        Literal::Float(_) => TyKind::Float { attr },
        Literal::String(_) => TyKind::String { attr },
        Literal::Bool(_) => TyKind::Bool { attr },
    }
}

/// An operand's union alternatives for operator dispatch, literals widened
/// to their bases regardless of freshness (dispatch is by base type; every
/// alternative must support the operator).
fn operand_members(ty: &Ty) -> Vec<Ty> {
    fn widen(ty: &Ty) -> Ty {
        match ty.kind() {
            TyKind::Literal(literal, _, attr) => Ty::intern(literal_base(literal, attr.clone())),
            // A builtin primitive-companion class receiver (`self` inside
            // `class Float`) IS its primitive for dispatch - the single
            // collapse rule (`baml_type::QualifiedTypeName::builtin_primitive`).
            TyKind::Class(qtn, args, attr) if args.is_empty() => {
                use baml_type::PrimitiveType;
                match qtn.builtin_primitive() {
                    Some(PrimitiveType::Int) => Ty::intern(TyKind::Int { attr: attr.clone() }),
                    Some(PrimitiveType::Bigint) => {
                        Ty::intern(TyKind::Bigint { attr: attr.clone() })
                    }
                    Some(PrimitiveType::Float) => Ty::intern(TyKind::Float { attr: attr.clone() }),
                    Some(PrimitiveType::String) => {
                        Ty::intern(TyKind::String { attr: attr.clone() })
                    }
                    Some(PrimitiveType::Bool) => Ty::intern(TyKind::Bool { attr: attr.clone() }),
                    _ => ty.clone(),
                }
            }
            _ => ty.clone(),
        }
    }
    match ty.kind() {
        TyKind::Union(members, _) => members.iter().map(widen).collect(),
        _ => vec![widen(ty)],
    }
}

#[cfg(test)]
mod syntactic_union_tests {
    use super::*;

    fn union(members: &[Ty]) -> Ty {
        Ty::union(members.iter().cloned())
    }

    #[test]
    fn singleton_collapses_to_member() {
        assert_eq!(syntactic_union(&[Ty::int()]), Ty::int());
    }

    #[test]
    fn empty_collapses_to_never() {
        assert_eq!(syntactic_union(&[]), Ty::never());
    }

    #[test]
    fn never_members_drop() {
        assert_eq!(syntactic_union(&[Ty::never(), Ty::int()]), Ty::int());
    }

    #[test]
    fn nested_unions_flatten_and_dedup() {
        let nested = union(&[Ty::int(), Ty::string()]);
        let flat = syntactic_union(&[nested, Ty::int()]);
        assert_eq!(flat, union(&[Ty::int(), Ty::string()]));
    }
}
