//! Body type inference: `infer_body` walks one body owner's expression tree
//! with an [`InferenceContext`] over an [`unify::InferenceTable`].
//!
//! S9 state: bidirectional checking. `infer_expr` synthesizes with an
//! [`Expectation`] flowing down (informing shape: container elements,
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
    let (TyKind::Literal(a, a_fresh, _), TyKind::Literal(b, b_fresh, _)) =
        (lhs.kind(), rhs.kind())
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
                (value.bits() <= baml_type::MAX_BIGINT_BITS)
                    .then(|| lit(Literal::Bigint(value)))
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
                for member in inner.iter() {
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
/// the target shape is here - TIR's bespoke FunctionCoercion struct
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
/// Sema-recorded argument matching consumed by SILGen. Keyed by the CALL
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
    /// available, r-a's node_args shape).
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
    Provided { param_index: usize, arg: ExprId },
    OmittedDefault { param_index: usize, param_name: baml_type::Name },
}

/// Inference side tables for one body owner, keyed by arena ids, mirroring
/// rust-analyzer's `InferenceResult`. Types are the hash-consed
/// `baml_type::interned` representation (this crate's native vocabulary);
/// they are materialized to plain `baml_type::Ty` only at consumer
/// boundaries, after resolve-all guarantees no inference variables remain.
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

fn infer_body_impl<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    owner: BodyOwnerId<'db>,
) -> InferenceResult<'db> {
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
                signature.params.iter().map(|param| param.ty.clone()).collect(),
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
                signature.params.iter().map(|param| param.ty.clone()).collect(),
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
            crate::lower::owner_self_ty(db, function, &frame)
        }
        BodyOwnerId::Let(_) => None,
    };
    let lower = lower_ctx_for_file(db, owner.file(db))
        .with_frame(frame)
        .with_bounds(bounds.clone())
        .with_self_ty(concrete_self);
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
    let (declared_throws, declared_throws_open) = match declared_throws_ref
        .map(|(store, throws)| lower.lower_type_ref(store, throws))
    {
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
    throws_channels: Vec<Vec<Ty>>,
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
    deferred_subs: Vec<(Ty, Ty)>,
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
    diverges: Diverges,
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
            template_params: Vec::new(),
            table: InferenceTable::new(),
            deferred_subs: Vec::new(),
            obligations: Vec::new(),
            obligation_anchor: None,
            body_owner: None,
            defaults_owner: false,
            chain_nullable: Vec::new(),
            diverges: Diverges::Maybe,
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
            Expr::Literal(lit) => Ty::intern(TyKind::Literal(
                lit.clone(),
                Freshness::Fresh,
                TyAttr::default(),
            )),
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
                for stmt in stmts {
                    self.infer_stmt(body, *stmt);
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
                    .map(|target_ref| self.lower_body_annotation(target_ref))
                    .unwrap_or_else(Ty::error);
                // The interface-view gate is a STRUCTURE demand: an
                // alias naming an interface answers as the interface.
                let target = self.expand_alias_ty(&target);
                if target.has_error() || !matches!(target.kind(), TyKind::Interface(..)) {
                    Ty::error()
                } else {
                    let saved_anchor = self.obligation_anchor.replace(expr);
                    let fits = self.sub(&base_ty, &target);
                    self.obligation_anchor = saved_anchor;
                    if !fits {
                        self.result
                            .type_mismatches
                            .insert(expr, (target.clone(), base_ty));
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
            Expr::Template { tag, .. } => match tag {
                baml_compiler2_ast::TemplateTag::Default { elaborated } => {
                    // Untagged backtick (BEP-049 §11): the value IS the
                    // desugared `string.from`-wrapped `+` concat, which
                    // types every `${expr}` in place on its original span.
                    self.infer_expr(body, *elaborated, &Expectation::None)
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
                    let (result, frame) = match resolved.kind() {
                        TyKind::Function { params, ret, .. } => {
                            let mut frame = FxHashMap::default();
                            if let Some(first) = params.first()
                                && let TyKind::Function {
                                    params: body_params,
                                    ..
                                } = first.ty.kind()
                            {
                                for param in body_params.iter() {
                                    if let Some(name) = &param.name {
                                        frame.insert(name.clone(), param.ty.clone());
                                    }
                                }
                            }
                            (ret.clone(), frame)
                        }
                        _ => (Ty::error(), FxHashMap::default()),
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
                    let saved_diverges =
                        std::mem::replace(&mut self.diverges, Diverges::Maybe);
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
                        Ty::list(self.table.new_var_ty())
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
                    Ty::intern(TyKind::Map {
                        key: self.table.new_var_ty(),
                        value: self.table.new_var_ty(),
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
                fields,
                spreads,
                ..
            } => self.infer_object(body, expr, type_name, fields, spreads),
            Expr::MemberAccess { base, member } => {
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
                let base_ty = self.infer_expr(body, *base, &Expectation::None);
                let nonnull = self.peel_chain_null(&base_ty);
                self.field_access(expr, &nonnull, member)
            }
            Expr::OptionalCall { callee, args } => {
                let callee_ty = self.infer_expr(body, *callee, &Expectation::None);
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
            // Not yet implemented: visit children generically, record the
            // sentinel.
            _ => {
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
                self.diverges = Diverges::Always;
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
                self.infer_expr(body, *loop_body, &Expectation::None);
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
                self.infer_expr(body, *loop_body, &Expectation::None);
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
                self.lower_pattern(body, *binding, &element);
                let entry_flow = self.flow.clone();
                let saved = self.diverges;
                self.infer_expr(body, *loop_body, &Expectation::None);
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
            self.infer_let_destructure(body, pattern, initializer);
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
                self.infer_let_destructure(body, pattern, initializer);
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
                    let result = self.compound_op_result(op, &element, &rhs);
                    if !element.has_error() && !self.sub(&result, &element) {
                        self.result
                            .type_mismatches
                            .insert(value, (element, result));
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
                (_, Some(place)) if !place.has_error() => {
                    self.check_expr(body, value, place)
                }
                _ => self.infer_expr(body, value, &Expectation::None),
            },
            Some(op) => {
                // Compound assignment: `target op value` through the same
                // operator machinery, the result checked against declared.
                
                let lhs = binding
                    .map(|binding| self.binding_flow_ty(binding))
                    .unwrap_or_else(|| self.infer_expr(body, target, &Expectation::None));
                let rhs = self.infer_expr(body, value, &Expectation::None);
                let result = self.compound_op_result(op, &lhs, &rhs);
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
                let (naked, targets): (Vec<Ty>, Vec<Ty>) = members
                    .into_iter()
                    .partition(|member| {
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
                self.deferred_subs.push((actual, expected));
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
                        && b_pins.iter().all(|(pin, _)| {
                            a_pins.iter().any(|(a_pin, _)| a_pin == pin)
                        })
                    {
                        let arg_pairs: Vec<(Ty, Ty)> = a_args
                            .iter()
                            .cloned()
                            .zip(b_args.iter().cloned())
                            .collect();
                        let pin_pairs: Vec<(Ty, Ty)> = b_pins
                            .iter()
                            .filter_map(|(pin, b_ty)| {
                                a_pins
                                    .iter()
                                    .find(|(a_pin, _)| a_pin == pin)
                                    .map(|(_, a_ty)| (a_ty.clone(), b_ty.clone()))
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
                            });
                        }
                        return true;
                    }
                    self.deferred_subs.push((actual, expected));
                    true
                }
            }
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
            self.deferred_subs.push((a.clone(), b.clone()));
            self.deferred_subs.push((b, a));
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
                    let value = if matches!(op, BinaryOp::Eq) { equal } else { !equal };
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
                let rhs_ty =
                    self.infer_expr(body, rhs, &Expectation::has_type(inner.clone()));
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
            let unknown = || Ty::intern(TyKind::Unknown {
                attr: TyAttr::default(),
            });
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
                                let chain =
                                    spawn_params_ty(cur_value.clone(), cur_error.clone());
                                let param_ty = param.ty.clone();
                                if !self.sub(&chain, &param_ty) {
                                    self.result
                                        .type_mismatches
                                        .insert(with_id, (param_ty, chain));
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
                    // Not a transformer at all: the readable mismatch
                    // against the expected shape.
                    self.result.type_mismatches.insert(with_id, (expected, got));
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
                let demanded =
                    Ty::intern(TyKind::Future(value.clone(), error.clone(), TyAttr::default()));
                let _ = self.table.unify(&resolved, &demanded);
                self.record_throw(expr, &error);
                value
            }
            _ if resolved.has_error() => resolved,
            _ => {
                let unknown = || Ty::intern(TyKind::Unknown {
                    attr: TyAttr::default(),
                });
                let expected =
                    Ty::intern(TyKind::Future(unknown(), unknown(), TyAttr::default()));
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
        op: baml_compiler2_ast::AssignOp,
        lhs: &Ty,
        rhs: &Ty,
    ) -> Ty {
        use baml_compiler2_ast::AssignOp;
        match op {
            AssignOp::Add => self.dispatch_operator("Add", lhs, Some(rhs)),
            AssignOp::Sub => self.dispatch_operator("Subtract", lhs, Some(rhs)),
            AssignOp::Mul => self.dispatch_operator("Multiply", lhs, Some(rhs)),
            AssignOp::Div => self.dispatch_operator("Divide", lhs, Some(rhs)),
            AssignOp::Mod => self.dispatch_operator("Remainder", lhs, Some(rhs)),
            AssignOp::BitAnd => self.dispatch_operator("BitAnd", lhs, Some(rhs)),
            AssignOp::BitOr => self.dispatch_operator("BitOr", lhs, Some(rhs)),
            AssignOp::BitXor => self.dispatch_operator("BitXor", lhs, Some(rhs)),
            AssignOp::Shl => self.dispatch_operator("ShiftLeft", lhs, Some(rhs)),
            AssignOp::Shr => self.dispatch_operator("ShiftRight", lhs, Some(rhs)),
        }
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
            if index_nullable
                && let Some(top) = self.chain_nullable.last_mut()
            {
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
        self.dispatch_operator(interface, &lhs_resolved, rhs_resolved.as_ref())
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
    fn member_operator_output(&mut self, interface: &str, lhs: &Ty, rhs: Option<&Ty>) -> Option<Ty> {
        if let TyKind::TypeVar(param, _) = lhs.kind() {
            let bounds =
                baml_type::normalize::TypeContext::type_var_bound(&self.facts, param);
            let bound = bounds.iter().find(|bound| {
                !bound.name.is_local()
                    && bound.name.package().as_str() == "baml"
                    && bound.name.namespace().len() == 1
                    && bound.name.namespace()[0].as_str() == "ops"
                    && bound.name.name().as_str() == interface
                    && match rhs {
                        Some(rhs) => {
                            bound.generics.len() == 1
                                && Ty::from_plain(&bound.generics[0]) == *rhs
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
        let ground =
            |ty: &Ty| !ty.has_infer() && !ty.has_error() && !matches!(ty.kind(), TyKind::Never { .. });
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
        // side channel (TIR's rule: not a parameter binding) - it still
        // TYPES through the positional fallback today; aligning its check
        // with the side-channel contract rides with the S17 diagnostics.
        let matched: Vec<Option<usize>> = args
            .iter()
            .enumerate()
            .map(|(index, arg)| match &arg.label {
                Some(label) => params
                    .iter()
                    .position(|param| param.name.as_ref() == Some(label))
                    .or_else(|| params.get(index).map(|_| index)),
                None => params.get(index).map(|_| index),
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
            if arg.label.as_ref().is_some_and(|label| label.as_str() == "$id") {
                runtime_id = Some(arg.expr);
                continue;
            }
            if let Some(param_index) = matched[index]
                && slots[param_index].is_none()
            {
                slots[param_index] = Some(arg.expr);
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
                    param if param.mode == baml_type::FunctionParamMode::Optional => {
                        param.name.clone().map(|param_name| {
                            ParamBinding::OmittedDefault {
                                param_index,
                                param_name,
                            }
                        })
                    }
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
            if let Some(fn_ty) =
                self.interface_static_value(prefix, &member[0], OwnArgs::Call(call), call)
            {
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
                let instantiation = self.instantiation_args(call, &signature.generic_params);
                self.register_call_bounds(function, &instantiation, call);
                self.write_call_type_args(call, &instantiation, 0);
                let fn_ty = function_value_ty(signature, &instantiation);
                self.result.type_of_expr.insert(callee, fn_ty.clone());
                self.write_member_resolution(callee, MemberResolution::Free { func: function });
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
                    if let Some(fn_ty) = self.type_qualified_member_callee(
                        call,
                        base_expr,
                        &segments,
                        &member,
                        callee,
                    ) {
                        self.result.type_of_expr.insert(callee, fn_ty.clone());
                        return (fn_ty, false);
                    }
                }
                let receiver = self.infer_expr(body, *base, &Expectation::None);
                let (ty, bound, resolution, desugar) = self.member_callee(call, &receiver, &member);
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
                let receiver = self.infer_expr(body, *base, &Expectation::None);
                let nonnull = self.peel_chain_null(&receiver);
                let (ty, bound, resolution, desugar) = self.member_callee(call, &nonnull, &member);
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
                let (ty, bound, resolution, desugar) = self.member_callee(call, &receiver, member);
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
        receiver: &Ty,
        member: &baml_type::Name,
    ) -> (Ty, bool, Option<MemberResolution<'db>>, bool) {
        let resolved = self.structurally_resolve(receiver);
        // Callee position on a UNION: every member must yield the
        // member as a callable with IDENTICAL parameters and boundness
        // (the forced case; differing signatures are S17's ambiguity) -
        // returns JOIN, throws union. `structurally_resolve` already
        // expanded weak aliases.
        if let TyKind::Union(union_members, _) = resolved.kind() {
            let union_members = union_members.to_vec();
            if let Some((ty, bound)) = self.union_member_callee(call, &union_members, member) {
                // One expression, one recorded entry: the union access
                // has no single declaration (its virtual view is an S16
                // follow-up).
                return (ty, bound, None, false);
            }
            // No per-member resolution: FALL THROUGH - the
            // operator-style sugars at the bottom of the ladder are
            // TOTAL and apply to the WHOLE union
            // (`(int | null).to_string()` is `string.from<int | null>`).
        }
        let candidate =
            crate::method_resolution::lookup_method(self.db, &self.facts, &resolved, member);
        let Some(candidate) = candidate else {
            // Interface members (I3): existential and bounded-var
            // receivers dispatch virtually; methods bind their receiver.
            // A receiver still CARRYING inference variables probes the
            // impls by unification instead (r-a's snapshot probe; the
            // ground registry fails safe on such types).
            let interface_member = crate::method_resolution::lookup_interface_member(
                self.db,
                &self.facts,
                &resolved,
                member,
            );
            if let Some(interface_member) = interface_member {
                let resolution = self.declarer_resolution(&interface_member.declarer, member);
                let (ty, bound) = self.interface_member_callee(interface_member, call);
                return (ty, bound, resolution, false);
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
            let (field, field_resolution) = self.field_access_resolved(call, &resolved, member);
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
            return (field, false, field_resolution, false);
        };
        let signature = function_signature(self.db, candidate.method);
        let class_count = candidate.class_args.len();
        let own_params = signature.generic_params[class_count..].to_vec();
        let mut instantiation = candidate.class_args;
        instantiation.extend(self.instantiation_args(call, &own_params));
        self.register_call_bounds(candidate.method, &instantiation, call);
        self.write_call_type_args(call, &instantiation, class_count);
        let fn_ty = function_value_ty(signature, &instantiation);
        let bound = signature
            .params
            .first()
            .is_some_and(|param| param.name.as_str() == "self");
        let resolution = if bound {
            MemberResolution::BoundMethod {
                class: candidate.class,
                func: candidate.method,
            }
        } else {
            MemberResolution::UnboundMethod {
                class: candidate.class,
                func: candidate.method,
            }
        };
        (fn_ty, bound, Some(resolution), false)
    }

    /// Callee position on a UNION receiver: every member must yield the
    /// member as a callable with IDENTICAL parameters and boundness (the
    /// forced case; differing signatures are S17's ambiguity) - returns
    /// JOIN, throws union. `None` when any member misses or disagrees;
    /// the caller falls through to the whole-union sugar tiers.
    fn union_member_callee(
        &mut self,
        call: ExprId,
        union_members: &[Ty],
        member: &baml_type::Name,
    ) -> Option<(Ty, bool)> {
        let mut resolved_fns = Vec::new();
        for member_ty in union_members {
            let (ty, bound, _, _) = self.member_callee(call, member_ty, member);
            if ty.has_error() {
                return None;
            }
            resolved_fns.push((ty, bound));
        }
        let (first, first_bound) = resolved_fns.first()?.clone();
        let TyKind::Function {
            params: first_params,
            ..
        } = first.kind()
        else {
            return None;
        };
        let mut rets = Vec::new();
        let mut throws_parts = Vec::new();
        for (fn_ty, bound) in &resolved_fns {
            let TyKind::Function { params, ret, throws, .. } = fn_ty.kind() else {
                return None;
            };
            if *bound != first_bound || params != first_params {
                return None;
            }
            rets.push(ret.clone());
            throws_parts.push(throws.clone());
        }
        let ret = self.join(&rets);
        let throws = self.union_of(&throws_parts);
        let fn_ty = Ty::intern(TyKind::Function {
            params: first_params.clone(),
            ret,
            throws,
            attr: TyAttr::default(),
        });
        Some((fn_ty, first_bound))
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
            baml_compiler2_ppir::item_data::method_interface_target(self.db, function)
                .as_ref()?;
        let target_ty = self
            .lower
            .lower_type_ref(&target.type_refs, target.target);
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
                    baml_compiler2_ppir::item_data::ImplSubjectData::InClass {
                        class, ..
                    } => crate::lower::class_self_ty(self.db, *class),
                }
            }
            _ => return None,
        };
        Some((
            InterfaceRef::new(
                name.clone(),
                (args.to_vec()).into(),
                pins.to_vec(),
            ),
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
            let signature = function_signature(self.db, pending.method);
            let own_offset = pending.prefix.len();
            let own_params = signature.generic_params[own_offset..].to_vec();
            let mut instantiation = pending.prefix;
            instantiation.extend(self.instantiation_args(call, &own_params));
            self.register_call_bounds(pending.method, &instantiation, call);
            self.write_call_type_args(call, &instantiation, own_offset);
            let fn_ty = function_value_ty(signature, &instantiation);
            return (fn_ty, interface_member.is_method);
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
                self.interface_static_value(prefix, &member[0], OwnArgs::Fresh, expr)
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
            && let Some(fn_ty) = self.from_json_desugar_value(
                &segments[..segments.len() - 1],
                OwnArgs::Fresh,
                None,
            )
        {
            self.result.desugared_callees.insert(expr);
            return fn_ty;
        }
        Ty::error()
    }

    fn own_instantiation(&mut self, own: OwnArgs, params: &[baml_type::ParamTy]) -> Vec<Ty> {
        match own {
            OwnArgs::Call(call) => self.instantiation_args(call, params),
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
    ) -> Option<Ty> {
        let method = self.interface_static_method(prefix, member)?;
        let signature = function_signature(self.db, method);
        let instantiation = self.own_instantiation(own, &signature.generic_params);
        self.register_call_bounds(method, &instantiation, anchor);
        if let OwnArgs::Call(call) = own {
            self.write_call_type_args(call, &instantiation, 0);
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
    fn from_json_desugar_value(
        &mut self,
        prefix: &[baml_type::Name],
        own: OwnArgs,
        record_base: Option<ExprId>,
    ) -> Option<Ty> {
        let written = self.lower.lower_type_path(prefix);
        let target = if !written.has_error() {
            written
        } else if let (OwnArgs::Call(call), Some((class, _))) =
            (own, self.static_class_for(prefix))
        {
            let frame = crate::lower::class_generic_frame(self.db, class);
            let args = self.instantiation_args(call, &frame);
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

    /// The MemberAccess spelling of a TYPE-QUALIFIED callee
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
        self.interface_static_value(prefix, member, own, call)
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
                "int" => Ty::intern(TyKind::Int { attr: baml_type::TyAttr::default() }),
                "bigint" => Ty::intern(TyKind::Bigint { attr: baml_type::TyAttr::default() }),
                "float" => Ty::intern(TyKind::Float { attr: baml_type::TyAttr::default() }),
                "string" => Ty::intern(TyKind::String { attr: baml_type::TyAttr::default() }),
                "bool" => Ty::intern(TyKind::Bool { attr: baml_type::TyAttr::default() }),
                "uint8array" => {
                    Ty::intern(TyKind::Uint8Array { attr: baml_type::TyAttr::default() })
                }
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
    ) -> Option<baml_compiler2_hir::loc::FunctionLoc<'db>> {
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
            .and_then(|ty| match ty.kind() {
                TyKind::Function { params, ret, .. } => Some((params.clone(), ret.clone())),
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
                        .and_then(|(params, _)| params.get(index))
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
            annotated_ret.or_else(|| expected_fn.as_ref().map(|(_, ret)| ret.clone()));

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
        let throws_ty = written_throws.unwrap_or_else(|| {
            if channel.is_empty() {
                Ty::never()
            } else {
                self.union_of(&channel)
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
                self.instantiate_holes(&lowered)
            })
            .collect();
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
            for (_, value) in fields {
                self.infer_expr(body, *value, &Expectation::None);
            }
            return Ty::error();
        };
        let db = self.db;
        let generic_count = baml_compiler2_ppir::item_data::class_data(db, class)
            .generic_params
            .len();
        let generic_names: Vec<baml_type::ParamTy> = crate::lower::class_generic_frame(db, class);
        let instantiation = self.instantiation_args(object, &generic_names);
        let mut instantiation = instantiation;
        instantiation.truncate(generic_count);
        while instantiation.len() < generic_count {
            instantiation.push(self.table.new_var_ty());
        }
        let field_types = crate::lower::class_field_types(db, class);
        for (name, value) in fields {
            match field_types.iter().find(|(field, _)| field == name) {
                Some((_, field_ty)) => {
                    let field_ty = substitute_params(field_ty, &instantiation);
                    self.check_expr(body, *value, &field_ty);
                }
                None => {
                    // Unknown field: S17's diagnostic.
                    self.infer_expr(body, *value, &Expectation::None);
                }
            }
        }
        for spread in spreads {
            self.infer_expr(body, spread.expr, &Expectation::None);
        }
        let short = type_name.0.last().expect("type paths are never empty");
        Ty::intern(TyKind::Class(
            self.lower.qualify_definition(
                baml_compiler2_hir::contributions::Definition::Class(class),
                short,
            ),
            instantiation.into(),
            TyAttr::default(),
        ))
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
        // TS's union-member rule (TIR follows): a member on a UNION
        // resolves on EVERY member type and the results JOIN; a member
        // type lacking it - null included: handle null first - fails
        // the whole access. Per-member resolutions are dropped: the
        // union access has no single declaration (its virtual view is
        // an S16 follow-up).
        if let TyKind::Union(members, _) = resolved.kind() {
            let members = members.to_vec();
            let mut tys = Vec::new();
            for member_ty in &members {
                let (ty, _) = self.field_access_resolved(at, member_ty, member);
                if ty.has_error() {
                    return (Ty::error(), None);
                }
                tys.push(ty);
            }
            return (self.join(&tys), None);
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
            let signature = function_signature(self.db, candidate.method);
            let mut instantiation = candidate.class_args;
            let own: Vec<Ty> = signature.generic_params[instantiation.len()..]
                .iter()
                .map(|param| self.fresh_generic_arg(param))
                .collect();
            instantiation.extend(own);
            // r-a registers required obligations at EVERY value
            // instantiation, whatever the spelling - a method read as a
            // VALUE obligates its own generics' bounds exactly as a
            // call would (add_required_obligations_for_value_path).
            self.register_call_bounds(candidate.method, &instantiation, at);
            return (
                bind_receiver(function_value_ty(signature, &instantiation)),
                Some(MemberResolution::BoundMethod {
                    class: candidate.class,
                    func: candidate.method,
                }),
            );
        }
        if let Some(interface_member) =
            crate::method_resolution::lookup_interface_member(self.db, &self.facts, &resolved, member)
        {
            let resolution = self.declarer_resolution(&interface_member.declarer, member);
            return (self.interface_member_value(interface_member), resolution);
        }
        (Ty::error(), None)
    }

    /// The recorded resolution for an interface member's declarer, when
    /// one applies (the concrete-field backing link is not resolved yet -
    /// no entry rather than a wrong one).
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
            let signature = function_signature(self.db, pending.method);
            let own: Vec<Ty> = signature.generic_params[pending.prefix.len()..]
                .iter()
                .map(|param| self.fresh_generic_arg(param))
                .collect();
            let mut instantiation = pending.prefix;
            instantiation.extend(own);
            return bind_receiver(function_value_ty(signature, &instantiation));
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
    /// owner's scope subtree (PatIds are per-body arenas; the subtree
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
        let lowered = self.lower.lower_type_ref(&self.type_refs.store, type_ref);
        self.instantiate_holes(&lowered)
    }

    /// The `process_user_written_ty` funnel (rust-analyzer's discipline):
    /// lowering is pure and emits var-less hole nodes for `_`; the inference
    /// side instantiates each hole as a fresh table variable, filled from
    /// context.
    fn instantiate_holes(&mut self, ty: &Ty) -> Ty {
        if !ty.has_infer() {
            return ty.clone();
        }
        if matches!(ty.kind(), TyKind::Infer { var: None, .. }) {
            return self.table.new_var_ty();
        }
        Ty::intern(
            ty.kind()
                .map_children(|child| self.instantiate_holes(child)),
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
        for contribution in &channel {
            let finalized = self.finalize_incoming_effect(contribution);
            if matches!(finalized.kind(), TyKind::Never { .. }) {
                continue;
            }
            let resolved = self.table.resolve_completely(&finalized);
            let canonical = self.matrix_scrut(&resolved);
            match canonical.kind() {
                TyKind::Union(members, _) => {
                    for member in members.iter() {
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
        // the channel is the generation site.
        let contribution = ty.clone();
        // An OPEN clause (`throws T | _`) admits every contribution; the
        // remainder joins the surface at finalize instead of erroring.
        if let Some(declared) = self.declared_throws.clone()
            && !self.declared_throws_open
            && !declared.has_error()
            && self.throws_channels.len() == 1
            && !self.sub(&contribution, &declared)
        {
            self.result
                .type_mismatches
                .insert(at, (declared, contribution.clone()));
        }
        self.throws_channels
            .last_mut()
            .expect("channel stack never empty")
            .push(contribution);
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
            let obligations = self.discharge_obligations_once();
            let subs = self.drain_deferred_subs();
            if solved || obligations || subs {
                continue;
            }
            if !self.resolve_bounded_vars(SolveTier::GroundSubset) {
                break;
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
                    .map(|ty| self.finalize_ty(ty))
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
        let mut result = std::mem::take(&mut self.result);
        result.throws = throws;
        for ty in result
            .type_of_expr
            .values_mut()
            .chain(result.type_of_pat.values_mut())
        {
            *ty = self.finalize_ty(ty);
        }
        for (expected, actual) in result.type_mismatches.values_mut() {
            *expected = self.finalize_ty(expected);
            *actual = self.finalize_ty(actual);
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
            _ => Ty::intern(ty.kind().map_children(|child| self.canonicalize_unions(child))),
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
            self.deferred_subs.push((deferred, var_ty.clone()));
        }
        for deferred in deferred_uppers {
            self.deferred_subs.push((var_ty.clone(), deferred));
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
                _ => Ty::error(),
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
        for (actual, expected) in deferred {
            let actual = self.table.resolve_completely(&actual);
            let expected = self.table.resolve_completely(&expected);
            if actual.has_infer() && expected.has_infer() {
                self.deferred_subs.push((actual, expected));
                continue;
            }
            if !actual.has_infer() && !expected.has_infer() {
                // KNOWN GAP (S17): a failed post-hoc bound is dropped
                // here - recording it needs provenance (an anchor expr)
                // threaded through VarBounds, the diagnostics slice's
                // business. The quiescence tiering above makes this
                // reachable only for genuinely ill-typed programs.
                let _ = is_subtype_interned(&actual, &expected, &self.facts);
                continue;
            }
            let before = self.deferred_subs.len();
            let _ = self.sub(&actual, &expected);
            let re_deferred = self.deferred_subs[before..]
                .iter()
                .any(|(a, e)| *a == actual && *e == expected);
            if !re_deferred {
                progressed = true;
            }
        }
        progressed
    }
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
    let binds = params
        .first()
        .is_some_and(|param| param.name.as_ref().is_some_and(|name| name.as_str() == "self"));
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
