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
pub(crate) mod truthy;
pub mod unify;

use std::{cell::RefCell, path::PathBuf, sync::Arc};

use baml_compiler_diagnostics::runtime_type::RuntimeTypeEscape;
use baml_compiler2_ast::{
    Expr, ExprBody, ExprId, ObjectExprField, PatId, Pattern, PropertySyntax, Stmt, StmtId,
    traverse::BodyNode,
};
use baml_compiler2_hir::{
    body::BodyOwnerId,
    body_type_refs::{BodyTypeArgRef, BodyTypeRefId, BodyTypeRefs},
    contributions::Definition,
    scope::FileScopeId,
    semantic_index::{
        BindingId, BindingKind, ExprMetadataKey, ExprMetadataScope, FileSemanticIndex,
        PathResolution,
    },
};
use baml_type::{
    Freshness, Int63, Literal, TyAttr,
    interned::{ClosedTy, InferInterface, InferTy, Ty},
    normalize::canonical_union_interned,
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
    matches!(ty.kind(), InferTy::Void { .. } | InferTy::Null { .. })
}

/// Whether an object slot initialized to `null` satisfies this type. Weak
/// aliases and solved inference variables must be resolved by the caller
/// before asking; an unconstrained type parameter is deliberately false
/// because it is not guaranteed to admit `null` for every instantiation.
fn type_admits_null(ty: &Ty) -> bool {
    match ty.kind() {
        InferTy::Null { .. } => true,
        InferTy::Union(members, _) => members.iter().any(type_admits_null),
        _ => false,
    }
}

/// The implicit `baml.spawn.Params<V, E>` a spawn's `with` chain
/// threads (BEP-034).
fn spawn_params_ty(value: Ty, error: Ty) -> Ty {
    Ty::intern(InferTy::Class(
        baml_type::TypeName::new(
            baml_type::Name::new("baml"),
            vec![baml_type::Name::new("spawn")],
            baml_type::Name::new("Params"),
        ),
        Box::new([value, error]),
        TyAttr::default(),
    ))
}

fn is_spawn_params_qtn(qtn: &baml_type::TypeName) -> bool {
    qtn.package().as_str() == "baml"
        && qtn.namespace().len() == 1
        && qtn.namespace()[0].as_str() == "spawn"
        && qtn.name().as_str() == "Params"
}

/// Negate a numeric literal into the negative literal TYPE (ruling 2:
/// `-1` is a type, TS parity). Freshness carries through. `None` skips
/// the fold: non-numeric literals, and an int result outside BAML's i63
/// value range (`-int.min_value()` = 2^62) - the unfolded dispatch result stands
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

/// The written halves of a fully-qualified item reference's qualifier — the
/// lowered `Self` type and the realized interface the determination road
/// proved for it. Always both present or both absent: a spelling either
/// writes the qualifier (`(Base as I).item`) or leaves the whole subject to
/// inference (`I.item`), never half of each.
struct WrittenQualifier<'a> {
    qself: Ty,
    realized: &'a baml_type::Interface,
}

fn negate_literal(lit: &Literal, freshness: Freshness) -> Option<Ty> {
    let negated = match lit {
        Literal::Int(n) => {
            let v = Int63::new(n.checked_neg()?)?;
            Literal::Int(v.get())
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
    Some(Ty::intern(InferTy::Literal(
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
    let (InferTy::Literal(a, a_fresh, _), InferTy::Literal(b, b_fresh, _)) =
        (lhs.kind(), rhs.kind())
    else {
        return None;
    };
    let freshness = match (a_fresh, b_fresh) {
        (Freshness::Regular, Freshness::Regular) => Freshness::Regular,
        _ => Freshness::Fresh,
    };
    let lit = |value: Literal| Ty::intern(InferTy::Literal(value, freshness, TyAttr::default()));
    let boolean = |value: bool| Some(lit(Literal::Bool(value)));
    let int = |value: i64| Int63::new(value).map(|value| lit(Literal::Int(value.get())));
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
                // The same semantic value used by the VM defines folding.
                // Negative counts remain unfolded so runtime dispatch raises
                // the typed `NegativeBitShift` panic.
                BinaryOp::Shl => Some(lit(Literal::Int(Int63::new(a)?.shift_left(b).ok()?.get()))),
                BinaryOp::Shr => Some(lit(Literal::Int(Int63::new(a)?.shift_right(b).ok()?.get()))),
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
            // The comparisons below are plain IEEE, not BAML's total float
            // order (`bex_vm_types::float_order`, which the runtime opcodes
            // use). The two agree on every finite value — they differ only on
            // NaN — and a float LITERAL is always finite: source has no NaN
            // spelling and `format_float` refuses to fold a non-finite result.
            // A NaN-valued literal type would break that, so it would have to
            // route these six through the total order instead.
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
            InferTy::Union(inner, _) => {
                for member in inner {
                    push(flat, member);
                }
            }
            InferTy::Never { .. } => {}
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
    source: &[baml_type::interned::InferFunctionParamTy],
    target: &[baml_type::interned::InferFunctionParamTy],
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
pub enum MemberResolution<'db, T = baml_type::Ty> {
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
        /// The callee's OWNER frame, carried from resolution (see
        /// `MemberDeclarer::ImplMethod::frame_type_args`): impl generic
        /// bindings for an override, `[Self, iface args..]` for a default.
        frame_type_args: Vec<T>,
        /// `true` when `func` is the interface's default body.
        from_interface_default: bool,
    },
    /// A VIRTUAL interface-field access: read through the realized
    /// declaring-interface view (`view`, the runtime resolver's key)
    /// at `field_index` in that interface's own declared field list.
    InterfaceVirtualField {
        interface: baml_compiler2_hir::loc::InterfaceLoc<'db>,
        view: T,
        field_index: u32,
        field: baml_type::Name,
    },
    /// A source-less callable, carrying its complete symbolic link and
    /// generic-frame contract.
    External(std::sync::Arc<crate::callable::ExternalCallable>),
    /// A field on a source-less class.
    ExternalField {
        class: baml_type::QualifiedTypeName,
        field: baml_type::Name,
    },
    /// A variant on a source-less enum.
    ExternalVariant {
        enum_name: baml_type::QualifiedTypeName,
        variant: baml_type::Name,
    },
    /// A virtual field on a source-less interface.
    ExternalInterfaceVirtualField {
        interface: baml_type::QualifiedTypeName,
        view: T,
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
pub struct ResolvedPath<'db, T = baml_type::Ty> {
    /// One entry per written segment: entry 0 is the root (a local,
    /// parameter, or template param - no member resolution), entry
    /// `i > 0` the member access the `i`-th segment performs.
    pub segments: Vec<ResolvedPathSegment<'db, T>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPathSegment<'db, T = baml_type::Ty> {
    /// The value's type AFTER this segment.
    pub ty: T,
    pub resolution: Option<MemberResolution<'db, T>>,
}

/// A recorded coercion step at an expression, consumed structurally by
/// MIR lowering - rust-analyzer's `Adjustment { kind, target }` exactly
/// (their infer.rs Adjust family: NeverToAny/Deref/Borrow/Pointer; BAML
/// has ONE adjustment kind today). The source shape is `type_of_expr`;
/// the target shape is here - TIR's bespoke `FunctionCoercion` struct
/// carried both redundantly.
#[derive(Debug, Clone, PartialEq)]
pub struct Adjustment<T = baml_type::Ty> {
    pub kind: Adjust,
    /// The post-adjustment type (the expectation the value was adapted
    /// to).
    pub target: T,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Adjust {
    /// The optional-parameter adapter: a function value satisfies its
    /// expectation by SUBTYPING but not by RUNTIME SHAPE (arity, mode,
    /// or optional-parameter names drift), so lowering synthesizes an
    /// adapter closure (TIR's `function_coercion_for` rule).
    FunctionAdapter,
    /// A condition position holding a non-`bool` value (B-1563): lowering
    /// synthesizes the truthiness test (`null`/`false`/zero/empty are
    /// falsy) so the branch itself stays strict-bool.
    Truthy,
}

/// One call's argument-to-parameter matching and solved instantiation -
/// TIR's `CallPlan` minus its dead fields (`instantiated_throws` was
/// TIR-internal; `call_type_instantiations` had no consumer). Rust needs
/// no analog (no named/default arguments); the prior art is Swift's
/// Sema-recorded argument matching consumed by `SILGen`. Keyed by the CALL
/// expression.
#[derive(Debug, Clone, PartialEq)]
pub struct CallPlan<T = baml_type::Ty, I = baml_type::Interface> {
    /// Parameter-ordered bindings over the callee's parameter list MINUS
    /// any bound receiver slot (the list written arguments match).
    /// Required parameters with no argument get no entry (the arity
    /// diagnostic is S17's).
    pub bindings: Vec<ParamBinding>,
    /// The callee's solved generic instantiation in declared De Bruijn
    /// order (owner frame prefix + own suffix). Recorded raw at the
    /// instantiation site; ground after writeback.
    pub type_args: Vec<T>,
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
    /// Every WRITTEN type-argument slot, in source order. Unlike
    /// `type_args`, this preserves whether the value was a static type or a
    /// runtime `unreflect(expr)` carrier.
    pub slots: Vec<CallTypeArgPlan<T>>,
    /// Checks whose declared shape mentions at least one runtime slot. They
    /// are intentionally not discharged by the static solver; MIR emits the
    /// equivalent runtime gate from this ledger.
    pub deferred_checks: Vec<RuntimeCheck<T, I>>,
    /// The trailing `$id = ...` side-channel argument (TIR's
    /// `CallSideChannels`, flattened until a second channel exists).
    pub runtime_id: Option<ExprId>,
    /// Stable source-location-free identity for a mounted callable. Source
    /// functions keep their existing location-backed `MemberResolution` and
    /// leave this empty.
    pub target: Option<crate::callable::ExternalCallTarget>,
}

/// Hand-written: the derive would bound `T: Default` + `I: Default`, which
/// neither type vocabulary provides (or needs — no field holds a bare `T`).
impl<T, I> Default for CallPlan<T, I> {
    fn default() -> Self {
        CallPlan {
            bindings: Vec::new(),
            type_args: Vec::new(),
            own_offset: 0,
            explicit: false,
            slots: Vec::new(),
            deferred_checks: Vec::new(),
            runtime_id: None,
            target: None,
        }
    }
}

/// One written generic slot after its sole lowering pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallTypeArgPlan<T = baml_type::Ty> {
    Static {
        /// Solved canonical type used by inference, equality, and digests.
        ty: T,
        /// Written type shape used only when MIR emits `LoadType`. It is
        /// resolved without union set-algebra so coercion try-order survives.
        emission_ty: T,
        /// Scoped carriers nested inside the written static shape.
        runtime_bindings: Box<[ScopedTypeBinding<T>]>,
    },
    Runtime {
        operand: ExprId,
        occurrence_ty: T,
        parameter: baml_type::ParamTy,
    },
}

/// A static check deferred precisely because its declared shape depends on a
/// runtime generic slot. Types remain symbolic over those runtime parameters;
/// all other call parameters have already been substituted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeCheck<T = baml_type::Ty, I = baml_type::Interface> {
    Argument { arg: ExprId, expected: T },
    Bound { argument: T, bound: I },
}

/// One lexical `type T = unreflect(value)` binding. The parameter is rigid and
/// statement-identity-based; `occurrence_ty` is the static replacement used
/// when the binding leaves its block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedTypeBinding<T = baml_type::Ty> {
    pub name: baml_type::Name,
    pub parameter: baml_type::ParamTy,
    /// Direct runtime carrier, or `None` when this binding is materialized
    /// from a composite type template.
    pub operand: Option<ExprId>,
    /// Composite template loaded before `BindType` for a lexical alias.
    pub template_ty: Option<T>,
    pub occurrence_ty: T,
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
/// rust-analyzer's `InferenceResult`. Generic over the type vocabulary: the
/// public artifact (the default `T = baml_type::Ty`) is PLAIN-NATIVE —
/// `finish` disposes of every inference variable and materializes through
/// the one seam (`materialize_ty`), so no interned handle escapes
/// inference. The engine itself records in the working instantiation
/// ([`WorkingResult`], `T =` interned `Ty`), its native vocabulary.
/// Which BEP-049 SS10 rule a tagged-template tag broke.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TaggedTagIssue {
    NotAFunction,
    NotMarked,
    BadBodyParam,
}

/// What the lowering→inference ingestion does with a written `_` hole.
#[derive(Debug, Clone, Copy, PartialEq)]
enum IngestHoles {
    /// An annotation-position hole: legal, anchored for unsolved reporting.
    Anchored(HoleAnchor),
    /// An expression-position hole: unconditionally diagnosed (E0147),
    /// instantiated only for recovery.
    ExprPosition(ExprId),
}

/// Where a written `_` hole sits: a body-position type annotation's ref.
#[derive(Debug, Clone, Copy, PartialEq)]
enum HoleAnchor {
    TypeRef(BodyTypeRefId),
}

/// Where an inference variable came from and the user-facing type that
/// contains it. Writeback uses this to report unsolved variables before
/// replacing them with the error sentinel.
#[derive(Debug, Clone, PartialEq, Eq)]
enum InferVarOrigin {
    TypeMustBeKnown {
        location: crate::diagnostics::DiagnosticLocation,
        containing_type: Ty,
    },
    LambdaParameter {
        lambda: ExprId,
        parameter_index: usize,
        name: baml_type::Name,
    },
}

#[derive(Debug, Clone)]
struct ReturnFrame {
    expected: Option<Ty>,
    candidates: Vec<Ty>,
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
    TopLevelLetCycle {
        expr: ExprId,
    },
    UnresolvedMember {
        expr: ExprId,
        base: Ty,
        member: baml_type::Name,
    },
    /// A body annotation failing the written-type well-formedness
    /// judgment (generic-argument bounds).
    AnnotWf {
        type_ref: BodyTypeRefId,
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
        sources: Vec<baml_type::interned::InferInterface>,
        is_field: bool,
    },
    /// An interface FIELD reached on a concrete receiver: reachable only
    /// through an explicit `obj.as<I>.field` projection.
    FieldRequiresProjection {
        expr: ExprId,
        base: Ty,
        member: baml_type::Name,
        interface: baml_type::interned::InferInterface,
    },
    /// A method declared with `Self` outside the receiver position,
    /// called through an existential/union receiver (Rust's `dyn`
    /// object-safety split).
    SelfRestrictedMember {
        expr: ExprId,
        interface: baml_type::interned::InferInterface,
        member: baml_type::Name,
        position: crate::diagnostics::SelfCallPosition,
    },
    /// A `self`-less interface method reached through a VALUE receiver.
    SelflessInstanceMember {
        expr: ExprId,
        interface_name: Option<baml_type::Name>,
        member: baml_type::Name,
    },
    /// An item projection's `Self` slot, judged once inference resolves it:
    /// erased (existential/union) `Self` is object-safety-gated for receiver
    /// methods and rejected for `self`-less ones. Pushed for EVERY item
    /// projection; a concrete or typevar slot passes silently.
    ItemProjectionSelfSlot {
        expr: ExprId,
        var: Ty,
        interface: baml_type::interned::InferInterface,
        member: baml_type::Name,
        takes_self: bool,
        /// Whether the reference is a VALUE (uncalled). A receiver method
        /// with an erased `Self` dispatches fine when CALLED (the receiver
        /// value carries the concrete type), but a reified value has no
        /// resolution moment — there is no thunk carrier yet, so it is
        /// rejected.
        value_position: bool,
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
        bound: baml_type::interned::InferInterface,
    },
    /// A constructor entry naming an implemented interface's FIELD; the
    /// backing class field is the constructor's key.
    InterfaceFieldInConstruction {
        object: ExprId,
        name: baml_type::Name,
        class_field: baml_type::Name,
    },
    /// A written interface qualifier that is not an interface at all:
    /// `.as<T>`, or `(Base as T).item`.
    QualifierNotInterface {
        expr: ExprId,
        target: Ty,
    },
    /// A written interface qualifier the subject does not implement:
    /// `x.as<I>` where `x`'s type does not, or `(Base as I).item` where
    /// `Base` does not.
    QualifierNotImplemented {
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
    GenericFunctionValueNotSpecialized {
        expr: ExprId,
        name: baml_type::Name,
        reference: String,
        inference_evidence: Vec<Ty>,
        specialization_args: Option<Vec<Ty>>,
        unconditional: bool,
        had_expected_type: bool,
        generic_params: Vec<baml_type::Name>,
        binding_name: Option<baml_type::Name>,
        function_shape: Option<String>,
        annotation_ty: Option<Ty>,
        specialization_example_is_safe: bool,
        specialization_syntax_available: bool,
    },
    ComputedGenericArgumentRequiresUnreflect {
        expr: ExprId,
        name: baml_type::Name,
    },
    MountedPackageCallUnsupported {
        expr: ExprId,
        path: baml_type::Name,
    },
    RuntimeTypeArgumentOnStreamingCall {
        expr: ExprId,
        callee: baml_type::Name,
    },
    RuntimeTypeArgumentOnIndirectCall {
        expr: ExprId,
    },
    /// An inline `unreflect(carrier)` slot whose rigid parameter survives into
    /// `enclosing`'s published type — as its value or as its error.
    RuntimeTypeMustBeNamed {
        carrier: ExprId,
        enclosing: ExprId,
        escape: RuntimeTypeEscape,
    },
    CannotConstructReflectionKind {
        expr: ExprId,
        class_name: baml_type::QualifiedTypeName,
    },
    CannotConstructBuiltinCompanion {
        expr: ExprId,
        class_name: baml_type::QualifiedTypeName,
        companion: baml_type::type_kind::BuiltinCompanion,
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
    LetElseMustDiverge {
        expr: ExprId,
        got: Ty,
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
    /// E0097: declared throws members the body can never throw.
    ExtraneousThrows {
        at: ExprId,
        extra_types: Vec<String>,
    },
    /// E0097: an `unknown`-containing contract without an escaping `unknown`.
    ImpreciseUnknownThrows {
        at: ExprId,
        inferred_types: Vec<String>,
    },
    /// Control flow that would escape a `defer` body (BEP-042): `return`
    /// always; `break`/`continue` unless a loop opened INSIDE the defer.
    DeferEscape {
        stmt: Option<StmtId>,
        expr: Option<ExprId>,
        keyword: &'static str,
    },
    ReturnTypeMismatch {
        stmt: Option<StmtId>,
        expr: Option<ExprId>,
        expected: Ty,
        actual: Ty,
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
    /// A constructor without a spread omitted fields whose resolved types do
    /// not admit `null`.
    MissingRequiredObjectFields {
        object: ExprId,
        class_name: baml_type::QualifiedTypeName,
        field_names: Vec<baml_type::Name>,
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
        type_ref: BodyTypeRefId,
        kind: crate::lower::LoweringDiagKind,
    },
    InterpolatedMaybeNull {
        expr: ExprId,
        ty: Ty,
    },
    BareOutputFormatReference {
        expr: ExprId,
    },
    /// B-1563 truthiness: a NON-literal condition whose static type
    /// decides the branch (`if (some_fn)`, `if (instance)`) - a likely
    /// bug, warned like TS 5.6's 2872/2873.
    ConditionAlwaysConst {
        expr: ExprId,
        ty: Ty,
        always_true: bool,
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
pub struct InferenceResult<'db, T = baml_type::Ty, I = baml_type::Interface> {
    pub type_of_expr: FxHashMap<ExprId, T>,
    pub type_of_pat: FxHashMap<PatId, T>,
    /// The owner's effect: the declared clause when written, else the
    /// canonical union of the body's throw sites and callee throws
    /// (`never` when nothing throws) - S12.
    pub throws: T,
    /// Definite check failures, keyed by the checked expression:
    /// `(expected, actual)`. Recorded always (rust-analyzer's discipline);
    /// rendered as diagnostics in S17.
    pub type_mismatches: FxHashMap<ExprId, (T, T)>,
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
    pub member_resolutions: FxHashMap<ExprId, MemberResolution<'db, T>>,
    /// Value-rooted multi-segment paths' per-segment ladders. S16: MIR's
    /// field-chain-vs-method decisions read the ladder instead of
    /// re-resolving.
    pub path_resolutions: FxHashMap<ExprId, ResolvedPath<'db, T>>,
    /// Per-call argument matching and solved instantiations, keyed by the
    /// CALL expression. S16: MIR's argument emission and `LoadType`
    /// operands read this instead of re-planning.
    pub call_plans: FxHashMap<ExprId, CallPlan<T, I>>,
    /// Durable lexical runtime-type bindings, keyed by the statement that
    /// evaluates and installs them. MIR consumes this identity instead of
    /// rebuilding a synthetic parameter from syntax.
    pub type_bindings: FxHashMap<StmtId, ScopedTypeBinding<T>>,
    /// Synthesized runtime slots keyed by the body-owned type reference that
    /// contains them.
    pub type_ref_bindings:
        FxHashMap<baml_compiler2_hir::type_ref::TypeRefId, Box<[ScopedTypeBinding<T>]>>,
    /// Checks whose expected shape depends on a lexical runtime-type binding.
    /// Static inference records the actual expression type but cannot decide
    /// the relation until the binding's operand has produced a runtime type;
    /// MIR emits that gate from this ledger.
    pub runtime_checks: Vec<RuntimeCheck<T, I>>,
    /// Coercion steps per expression (r-a's `expr_adjustments` shape).
    /// S16: MIR synthesizes the recorded adapters instead of re-deciding.
    pub expr_adjustments: FxHashMap<ExprId, Box<[Adjustment<T>]>>,
    /// Callee expressions the walk resolved through a LANGUAGE-SUGAR
    /// tier (`to_string`/`to_json`/`from_json` lang-item desugars).
    /// Recorded as POSITIVE knowledge; TIR's convention leaves these
    /// callees untyped and MIR keys the desugar on that absence, so the
    /// provider omits their expr types (post-flip, MIR reads this table
    /// directly instead of an absence).
    pub desugared_callees: rustc_hash::FxHashSet<ExprId>,
}

/// The working instantiation: the engine records in its native interned
/// vocabulary; `finish` finalizes in place and then materializes into the
/// public plain `InferenceResult` through the `materialize_ty` seam.
pub(crate) type WorkingResult<'db> = InferenceResult<'db, Ty, baml_type::interned::InferInterface>;

impl<T, I> InferenceResult<'_, T, I> {
    /// The empty result at the given effect seed — shared by the per-
    /// vocabulary `Default`s, whose only difference is which `never` they
    /// can spell.
    fn empty(throws: T) -> Self {
        InferenceResult {
            type_of_expr: FxHashMap::default(),
            type_of_pat: FxHashMap::default(),
            throws,
            type_mismatches: FxHashMap::default(),
            non_exhaustive_matches: rustc_hash::FxHashSet::default(),
            diagnostics: Vec::new(),
            member_resolutions: FxHashMap::default(),
            path_resolutions: FxHashMap::default(),
            call_plans: FxHashMap::default(),
            type_bindings: FxHashMap::default(),
            type_ref_bindings: FxHashMap::default(),
            runtime_checks: Vec::new(),
            expr_adjustments: FxHashMap::default(),
            desugared_callees: rustc_hash::FxHashSet::default(),
        }
    }
}

impl Default for InferenceResult<'_> {
    fn default() -> Self {
        InferenceResult::empty(baml_type::Ty::never())
    }
}

impl Default for WorkingResult<'_> {
    fn default() -> Self {
        InferenceResult::empty(Ty::never())
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

type LetOwnerKey = (PathBuf, u32);

thread_local! {
    static IN_FLIGHT_LET_OWNERS: RefCell<std::collections::HashSet<LetOwnerKey>> =
        RefCell::new(std::collections::HashSet::new());
}

fn let_owner_key(
    db: &dyn baml_compiler2_ppir::Db,
    let_binding: baml_compiler2_hir::loc::LetLoc<'_>,
) -> LetOwnerKey {
    (let_binding.file(db).path(db), let_binding.id(db).as_u32())
}

fn let_owner_is_in_flight(
    db: &dyn baml_compiler2_ppir::Db,
    let_binding: baml_compiler2_hir::loc::LetLoc<'_>,
) -> bool {
    let key = let_owner_key(db, let_binding);
    IN_FLIGHT_LET_OWNERS.with(|owners| owners.borrow().contains(&key))
}

struct InFlightLetOwner {
    key: LetOwnerKey,
    inserted: bool,
}

impl InFlightLetOwner {
    fn enter(
        db: &dyn baml_compiler2_ppir::Db,
        let_binding: baml_compiler2_hir::loc::LetLoc<'_>,
    ) -> Self {
        let key = let_owner_key(db, let_binding);
        let inserted = IN_FLIGHT_LET_OWNERS.with(|owners| owners.borrow_mut().insert(key.clone()));
        Self { key, inserted }
    }
}

impl Drop for InFlightLetOwner {
    fn drop(&mut self) {
        if self.inserted {
            IN_FLIGHT_LET_OWNERS.with(|owners| {
                owners.borrow_mut().remove(&self.key);
            });
        }
    }
}

fn infer_let_body_cycle_initial<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    _id: salsa::Id,
    let_binding: baml_compiler2_hir::loc::LetLoc<'db>,
) -> InferenceResult<'db> {
    use crate::diagnostics::{DiagnosticLocation, DiagnosticSeverity, TirDiagnostic, TirTypeError};

    let mut result = InferenceResult::default();
    let body = baml_compiler2_hir::body::let_body(db, let_binding);
    if let baml_compiler2_hir::body::LetBody::Expr(body) = body.as_ref()
        && let Some(root) = body.root_expr
    {
        result.type_of_expr.insert(root, baml_type::Ty::error());
        result.diagnostics.push(TirDiagnostic {
            error: TirTypeError::CannotInferType,
            severity: DiagnosticSeverity::Error,
            primary: DiagnosticLocation::Expr(root),
            related: Vec::new(),
        });
    }
    result
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

/// TRACKED (S2/S3): top-level `let` bodies. Session submissions can retain
/// mutually recursive lets, so the query has an error-typed cycle seed while
/// the explicit in-flight set normally diagnoses before Salsa must recover.
#[salsa::tracked(returns(ref), cycle_initial = infer_let_body_cycle_initial)]
fn infer_let_body<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    let_binding: baml_compiler2_hir::loc::LetLoc<'db>,
) -> InferenceResult<'db> {
    let _in_flight = InFlightLetOwner::enter(db, let_binding);
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

/// The bounds the owner declared — the param env a body owner's inference
/// runs in (each rigid type variable's declared bound conjunction), in the
/// declaration side's own plain vocabulary (the lowering ctx and the fact
/// oracle take it directly).
pub(crate) fn owner_declared_bounds<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    owner: BodyOwnerId<'db>,
) -> FxHashMap<baml_type::ParamTy, Vec<baml_type::Interface>> {
    match owner {
        BodyOwnerId::Function(function) | BodyOwnerId::ParameterDefaults(function) => {
            crate::lower::function_generic_bounds(db, function)
        }
        BodyOwnerId::Let(_) => FxHashMap::default(),
    }
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
                    .map(|param| crate::impls::interned_ty(&param.ty))
                    .collect(),
                Some(crate::impls::interned_ty(&signature.ret)),
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
                    .map(|param| crate::impls::interned_ty(&param.ty))
                    .collect(),
                None,
                None,
            )
        }
        BodyOwnerId::Let(_) => (Vec::new(), Vec::new(), None, None),
    };
    let concrete_self = match owner {
        BodyOwnerId::Function(function) | BodyOwnerId::ParameterDefaults(function) => {
            // BODY-position `Self` is a PLAIN-class-method error (the
            // ratified rule: signatures resolve it, bodies do not);
            // implements-block bodies (`Self` substitutes to the subject,
            // and they are Impl-owned — in-class and out-of-body alike)
            // and interface default bodies (frame slot 0) keep theirs.
            match baml_compiler2_ppir::item_data::method_owner(db, function) {
                Some(baml_compiler2_ppir::item_data::MethodOwner::Class(_)) => {
                    debug_assert!(
                        baml_compiler2_ppir::item_data::method_interface_target(db, function)
                            .is_none(),
                        "interface targets are recorded on impl-block methods, which are \
                         Impl-owned",
                    );
                    None
                }
                _ => crate::lower::owner_self_ty(db, function),
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
        .with_bounds(owner_declared_bounds(db, owner))
        .with_self_ty(concrete_self)
        .with_impl_target(impl_target);
    let type_refs = baml_compiler2_ppir::body_type_refs(db, owner);
    let plain_bounds = owner_declared_bounds(db, owner);
    // Split the declared clause into its named part and openness (spec
    // rule 3: `throws T | _` names T and opens the remainder to
    // inference); nested holes in named members stay ruling-4 errors.
    let (declared_throws, declared_throws_open) =
        match declared_throws_ref.map(|(store, throws)| lower.lower_type_ref(store, throws)) {
            Some(raw) => {
                let (named, open) = crate::lower::throws_clause_parts(&raw);
                (Some(Ty::from_plain(&named)), open)
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
        stable_body_owner_identity(db, owner),
    );
    ctx.declared_throws = declared_throws;
    ctx.declared_throws_open = declared_throws_open;
    ctx.body_owner_id = Some(owner);
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
            ctx.register_property_shorthands(arena);
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

/// Architecture-stable owner key for lexical synthetic parameters. This is
/// deliberately derived from source identity rather than Salsa intern IDs.
fn stable_body_owner_identity(db: &dyn baml_compiler2_ppir::Db, owner: BodyOwnerId<'_>) -> u32 {
    let mut hash = 0x811c_9dc5_u32;
    let mut write = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u32::from(*byte);
            hash = hash.wrapping_mul(0x0100_0193);
        }
    };
    write(owner.file(db).path(db).to_string_lossy().as_bytes());
    match owner {
        BodyOwnerId::Function(function) => {
            write(&[0]);
            write(&function.id(db).as_u32().to_le_bytes());
        }
        BodyOwnerId::Let(let_binding) => {
            write(&[1]);
            write(&let_binding.id(db).as_u32().to_le_bytes());
        }
        BodyOwnerId::ParameterDefaults(function) => {
            write(&[2]);
            write(&function.id(db).as_u32().to_le_bytes());
        }
    }
    hash
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
        if matches!(ty.kind(), InferTy::Error { .. }) {
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
                if matches!(resolved.kind(), InferTy::InferVar { .. }) {
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
    /// Active lexical runtime type names, innermost last. The declaration
    /// lowering context remains immutable; body-owned type lowering forks it
    /// with these rigid parameters.
    scoped_type_bindings: Vec<ScopedTypeBinding<Ty>>,
    /// One durable synthesized binding per `Unreflect` type-ref node.
    synthesized_type_bindings:
        FxHashMap<baml_compiler2_hir::type_ref::TypeRefId, ScopedTypeBinding<Ty>>,
    /// Stable hash of the body owner, combined with `StmtId` for scoped rigid
    /// parameter identity.
    body_owner_identity: u32,
    /// Full owner identity for the Session top-level-let value tier. Keeping
    /// this lets a malformed self-reference fail closed instead of recursively
    /// asking Salsa for the inference result currently being built.
    body_owner_id: Option<BodyOwnerId<'db>>,
    /// The owner's parameter types, from its lowered signature, indexed by
    /// declaration position.
    param_tys: Vec<Ty>,
    /// Every type annotation written in this body, pre-lowered to span-free
    /// `TypeRef`s (the rust-analyzer bodies-own-their-type-refs shape).
    type_refs: Arc<BodyTypeRefs>,
    /// One return context per callable, with the current callable last.
    /// The bottom frame belongs to the body owner; lambdas push their own.
    return_frames: Vec<ReturnFrame>,
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
    /// User-source origins for ordinary inference variables that require a
    /// concrete solution. The order vector makes diagnostic selection stable
    /// when several source variables unify into one unresolved class.
    infer_var_origins: FxHashMap<baml_type::interned::InferVar, InferVarOrigin>,
    infer_var_origin_order: Vec<baml_type::interned::InferVar>,
    diagnosed_infer_vars: rustc_hash::FxHashSet<baml_type::interned::InferVar>,
    /// One lowering per written body annotation (rust-analyzer's
    /// discipline): the let rule, the pattern walk, and the backfill all
    /// read the SAME lowered type - and so the same instantiated hole
    /// vars - for one `BodyTypeRefId`. Without this, a `_` hole instantiates
    /// once per consumer and only the demand-connected copy solves.
    annotation_cache: FxHashMap<BodyTypeRefId, Ty>,
    /// Per-body memoization of ground canonical forms. Inference-bearing types
    /// bypass it because their meaning changes as the table solves variables.
    canonical_cache: baml_type::normalize::InternedCanonicalCache,
    /// Member-lookup PROBE depth (TIR's `suppress_member_lookup_errors`
    /// discipline): a failed lookup reports only when no fallback tier
    /// remains - probes increment, the committed frame reports.
    member_probe_depth: u32,
    /// Nonzero while an optional-call callee is inferred as an ordinary
    /// expression. Its arguments still provide specialization evidence after
    /// the callee has been typed, so a generic value diagnostic must remain
    /// conditional until the whole call has been checked.
    optional_call_callee_depth: u32,
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
    /// Shorthand value expressions and the exact property name they require.
    /// Populated from the structural AST before inference so the ordinary path
    /// resolver can select the specialized unresolved-name diagnostic.
    property_shorthand_values: FxHashMap<ExprId, baml_type::Name>,
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
    /// Conditions and `!` operands whose type still carried an inference
    /// variable at check time (B-1563 truthiness); decided at finish on
    /// the final type.
    pending_truthy_conditions: Vec<crate::infer::truthy::PendingCondition>,
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
    /// Original parameter templates for argument checks that mention a
    /// runtime generic. This is inference-only staging: `check_call_args`
    /// consumes it into durable `CallPlan::deferred_checks`, so defaults and
    /// sibling bodies cannot observe it.
    runtime_dependent_call_params: FxHashMap<ExprId, FxHashMap<usize, Ty>>,
    /// Carriers already reported as escaping their call (E0168), so a callee
    /// road walked twice reports once.
    reported_runtime_escapes: rustc_hash::FxHashSet<ExprId>,
    /// Runtime carriers can be reached by more than one inference road (for
    /// example the claim and body passes over a catch pattern). Their
    /// expression effects and diagnostics must still be inferred once.
    validated_runtime_operands: rustc_hash::FxHashSet<ExprId>,
    /// Inline `unreflect(...)` carriers whose call publishes the parameter in
    /// its RESULT — the bare `-> T` included. The `?.` check consults this at
    /// the chain boundary, where the callee's signature is no longer reachable.
    runtime_slots_named_by_result: rustc_hash::FxHashSet<ExprId>,
    result: WorkingResult<'db>,
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
        body_owner_identity: u32,
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
            scoped_type_bindings: Vec::new(),
            synthesized_type_bindings: FxHashMap::default(),
            body_owner_identity,
            body_owner_id: None,
            param_tys,
            type_refs,
            return_frames: return_ty
                .into_iter()
                .map(|expected| ReturnFrame {
                    expected: Some(expected),
                    candidates: Vec::new(),
                })
                .collect(),
            declared_throws: None,
            declared_throws_open: false,
            throws_channels: vec![Vec::new()],
            pending_diags: Vec::new(),
            hole_vars: Vec::new(),
            infer_var_origins: FxHashMap::default(),
            infer_var_origin_order: Vec::new(),
            diagnosed_infer_vars: rustc_hash::FxHashSet::default(),
            annotation_cache: FxHashMap::default(),
            canonical_cache: baml_type::normalize::InternedCanonicalCache::default(),
            member_probe_depth: 0,
            optional_call_callee_depth: 0,
            or_probe_depth: 0,
            rest_reject_depth: 0,
            template_params: Vec::new(),
            table: InferenceTable::new(),
            deferred_subs: Vec::new(),
            obligations: Vec::new(),
            obligation_anchor: None,
            body_owner: None,
            defaults_owner: false,
            chain_nullable: Vec::new(),
            property_shorthand_values: FxHashMap::default(),
            defer_loop_floors: Vec::new(),
            loop_depth: 0,
            body_root: None,
            provisional_checks: Vec::new(),
            pending_truthy_conditions: Vec::new(),
            diverges: Diverges::Maybe,
            owner_file: None,
            overlap_aliases: std::cell::OnceCell::new(),
            wf_scope_env: std::cell::OnceCell::new(),
            runtime_dependent_call_params: FxHashMap::default(),
            reported_runtime_escapes: rustc_hash::FxHashSet::default(),
            validated_runtime_operands: rustc_hash::FxHashSet::default(),
            runtime_slots_named_by_result: rustc_hash::FxHashSet::default(),
            result: WorkingResult::default(),
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
        self.register_property_shorthands(body);
        self.body_root = body.root_expr;
        if let Some(root) = body.root_expr {
            match self
                .return_frames
                .last()
                .and_then(|frame| frame.expected.clone())
            {
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

    fn register_property_shorthands(&mut self, body: &ExprBody) {
        for (_, expr) in body.exprs.iter() {
            match expr {
                Expr::Map { entries } => {
                    for entry in entries {
                        if entry.syntax != PropertySyntax::Shorthand {
                            continue;
                        }
                        let Expr::Path(segments) = &body.exprs[entry.value] else {
                            debug_assert!(false, "map shorthand value must be a path");
                            continue;
                        };
                        let [name] = segments.as_slice() else {
                            debug_assert!(false, "map shorthand value must be a single name");
                            continue;
                        };
                        self.property_shorthand_values
                            .insert(entry.value, name.clone());
                    }
                }
                Expr::Object { fields, .. } => {
                    for field in fields {
                        if field.syntax == PropertySyntax::Shorthand {
                            self.property_shorthand_values
                                .insert(field.value, field.name.clone());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Reject an impossible outer shape before replacing runtime-rigid leaves
    /// with inference variables. `Sub` intentionally treats some
    /// variable-carrying pairs as deferred, so by itself it cannot distinguish
    /// `int` from `list<?runtime>` at this stage.
    fn runtime_static_skeleton_matches(
        &mut self,
        actual: &Ty,
        expected: &Ty,
        runtime_params: &[baml_type::ParamTy],
    ) -> bool {
        let actual = self.table.shallow_resolve(actual);
        let expected = self.table.shallow_resolve(expected);
        if actual.has_error() || expected.has_error() {
            return true;
        }
        if matches!(expected.kind(), InferTy::TypeVar(param, _) if runtime_params.contains(param)) {
            return true;
        }
        if !actual.has_infer()
            && !expected.has_infer()
            && !runtime_params
                .iter()
                .any(|param| ty_mentions_param(&expected, param))
        {
            return self.cached_subtype(&actual, &expected);
        }

        match (actual.kind(), expected.kind()) {
            (InferTy::Union(actual_members, _), _) => {
                for member in actual_members {
                    if !self.runtime_static_skeleton_matches(member, &expected, runtime_params) {
                        return false;
                    }
                }
                true
            }
            (_, InferTy::Union(expected_members, _)) => {
                for member in expected_members {
                    if self.runtime_static_skeleton_matches(&actual, member, runtime_params) {
                        return true;
                    }
                }
                false
            }
            (InferTy::List(actual_item, _), InferTy::List(expected_item, _)) => {
                self.runtime_static_skeleton_matches(actual_item, expected_item, runtime_params)
            }
            (
                InferTy::Map {
                    key: actual_key,
                    value: actual_value,
                    ..
                },
                InferTy::Map {
                    key: expected_key,
                    value: expected_value,
                    ..
                },
            ) => {
                self.runtime_static_skeleton_matches(actual_key, expected_key, runtime_params)
                    && self.runtime_static_skeleton_matches(
                        actual_value,
                        expected_value,
                        runtime_params,
                    )
            }
            (
                InferTy::Class(actual_name, actual_args, _),
                InferTy::Class(expected_name, expected_args, _),
            ) if actual_name == expected_name && actual_args.len() == expected_args.len() => {
                for (actual_arg, expected_arg) in actual_args.iter().zip(expected_args) {
                    if !self.runtime_static_skeleton_matches(
                        actual_arg,
                        expected_arg,
                        runtime_params,
                    ) {
                        return false;
                    }
                }
                true
            }
            (
                InferTy::Interface(actual_name, actual_args, actual_pins, _),
                InferTy::Interface(expected_name, expected_args, expected_pins, _),
            ) if actual_name == expected_name && actual_args.len() == expected_args.len() => {
                for (actual_arg, expected_arg) in actual_args.iter().zip(expected_args) {
                    if !self.runtime_static_skeleton_matches(
                        actual_arg,
                        expected_arg,
                        runtime_params,
                    ) {
                        return false;
                    }
                }
                for (name, expected_pin) in expected_pins {
                    if let Some((_, actual_pin)) = actual_pins
                        .iter()
                        .find(|(actual_name, _)| actual_name == name)
                        && !self.runtime_static_skeleton_matches(
                            actual_pin,
                            expected_pin,
                            runtime_params,
                        )
                    {
                        return false;
                    }
                }
                true
            }
            // A class may satisfy a runtime-parameterized interface. The
            // subsequent `Sub` call owns that implementation lookup.
            (InferTy::Class(..), InferTy::Interface(..)) => true,
            (
                InferTy::Function {
                    params: actual_params,
                    ret: actual_ret,
                    throws: actual_throws,
                    ..
                },
                InferTy::Function {
                    params: expected_params,
                    ret: expected_ret,
                    throws: expected_throws,
                    ..
                },
            ) if actual_params.len() == expected_params.len() => {
                for (actual_param, expected_param) in actual_params.iter().zip(expected_params) {
                    if !self.runtime_static_skeleton_matches(
                        &actual_param.ty,
                        &expected_param.ty,
                        runtime_params,
                    ) {
                        return false;
                    }
                }
                self.runtime_static_skeleton_matches(actual_ret, expected_ret, runtime_params)
                    && self.runtime_static_skeleton_matches(
                        actual_throws,
                        expected_throws,
                        runtime_params,
                    )
            }
            (
                InferTy::Future(actual_value, actual_error, _),
                InferTy::Future(expected_value, expected_error, _),
            ) => {
                self.runtime_static_skeleton_matches(actual_value, expected_value, runtime_params)
                    && self.runtime_static_skeleton_matches(
                        actual_error,
                        expected_error,
                        runtime_params,
                    )
            }
            // Projections are resolved by `Sub`; their head may legitimately
            // differ from the concrete type they reduce to.
            (_, InferTy::AssociatedTypeProjection { .. }) => true,
            // Open inference and rigid generic leaves carry no statically
            // inspectable shape. The committed `Sub` relation immediately
            // after this guard owns their bounds and obligations.
            (InferTy::InferVar { .. } | InferTy::TypeVar(..), _)
            | (_, InferTy::InferVar { .. } | InferTy::TypeVar(..)) => true,
            _ => false,
        }
    }

    /// Checking mode: infer with the expectation, then constrain -
    /// `Sub(actual, expected)`, discharged eagerly. Definite failures are
    /// recorded against the checked expression, never dropped.
    fn check_expr(&mut self, body: &ExprBody, expr: ExprId, expected: &Ty) -> Ty {
        // A lexical `type T = unreflect(value)` is rigid for identity and
        // name resolution, but its actual runtime shape is unavailable to
        // static inference. Preserve the check structurally for MIR instead
        // of either accepting it blindly or diagnosing every concrete value
        // against the opaque parameter. The expression still infers without
        // an expectation so its own diagnostics/effects and actual type are
        // retained.
        let depends_on_scoped_type = self
            .scoped_type_bindings
            .iter()
            .any(|binding| ty_mentions_param(expected, &binding.parameter));
        if depends_on_scoped_type {
            let ty = self.infer_deferred_runtime_expr(body, expr, expected);
            // Erasing only the dynamic leaves a static skeleton: `list<T>`
            // still rejects an `int`, while a `list<int>` advances to the
            // runtime gate for its element relation. This is the same
            // dependent-only discipline used by call-site runtime slots.
            // Replace each runtime-rigid leaf with one fresh inference variable
            // for this static shape check. `unknown` is still an ordinary,
            // invariant generic argument (`Wrapper<string>` is not a subtype
            // of `Wrapper<unknown>`), whereas this check needs a true hole:
            // prove the surrounding constructors line up, then leave the leaf
            // relation to MIR's runtime gate.
            let runtime_params: Vec<_> = self
                .scoped_type_bindings
                .iter()
                .map(|binding| binding.parameter.clone())
                .collect();
            let skeleton_matches =
                self.runtime_static_skeleton_matches(&ty, expected, &runtime_params);
            let dynamic_holes: Vec<_> = runtime_params
                .into_iter()
                .map(|parameter| {
                    (
                        parameter,
                        self.table.new_var_ty_of(unify::VarPolicy::RuntimeHole),
                    )
                })
                .collect();
            let static_expected = dynamic_holes
                .iter()
                .fold(expected.clone(), |expected, (parameter, hole)| {
                    replace_rigid_param(&expected, parameter, hole)
                });
            let saved_anchor = self.obligation_anchor.replace(expr);
            let fits_static_shape = skeleton_matches && self.sub(&ty, &static_expected);
            self.obligation_anchor = saved_anchor;
            if fits_static_shape {
                self.result.runtime_checks.push(RuntimeCheck::Argument {
                    arg: expr,
                    expected: expected.clone(),
                });
                self.record_checked_function_adapter(expr, &ty, expected);
            } else {
                self.result
                    .type_mismatches
                    .insert(expr, (expected.clone(), ty.clone()));
            }
            return ty;
        }
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
            self.record_checked_function_adapter(expr, &ty, expected);
        }
        ty
    }

    fn infer_return(
        &mut self,
        body: &ExprBody,
        value: Option<ExprId>,
        stmt: Option<StmtId>,
        expr: Option<ExprId>,
    ) {
        let expected = self
            .return_frames
            .last()
            .and_then(|frame| frame.expected.clone());
        let actual = match value {
            Some(value) => match &expected {
                Some(expected) if !expected.has_error() => self.check_expr(body, value, expected),
                _ => self.infer_expr(body, value, &Expectation::None),
            },
            None => {
                let actual = Ty::void();
                if let Some(expected) = &expected
                    && !expected.has_error()
                {
                    let fits = self.sub(&actual, expected);
                    if expected.has_infer() || !fits {
                        self.pending_diags.push(PendingDiag::ReturnTypeMismatch {
                            stmt,
                            expr,
                            expected: expected.clone(),
                            actual: actual.clone(),
                        });
                    }
                }
                actual
            }
        };
        if let Some(frame) = self.return_frames.last_mut() {
            frame.candidates.push(actual);
        }
    }

    /// Infer an expression whose outer type relation is checked at runtime.
    /// A lambda still receives the callback shape for contextual signature
    /// deduction: a runtime-bound parameter is opaque, not absent.
    fn infer_deferred_runtime_expr(&mut self, body: &ExprBody, expr: ExprId, expected: &Ty) -> Ty {
        let expectation = if matches!(body.exprs[expr], Expr::Lambda(_)) {
            Expectation::has_type(expected.clone())
        } else {
            Expectation::None
        };
        self.infer_expr(body, expr, &expectation)
    }

    /// rustc/r-a record coercions as per-expression adjustments consumed
    /// structurally at MIR lowering; every checked position funnels
    /// through `check_expr`, so this one probe covers TIR's five
    /// recording sites. Fires only on an ACCEPTED check whose value and
    /// expectation are both function-shaped but runtime-incompatible
    /// (TIR's `function_coercion_for`): lowering must synthesize an
    /// adapter closure. Target selection runs only after the subtype check
    /// succeeds and uses the actual function to disambiguate union arms.
    fn record_checked_function_adapter(&mut self, expr: ExprId, got: &Ty, expected: &Ty) {
        let Some(adapter_expected) = self.function_adapter_target(got, expected) else {
            return;
        };
        self.record_function_adapter(expr, got, &adapter_expected);
    }

    /// The concrete target an accepted function value must implement at
    /// runtime. Direct function expectations are already unambiguous. For a
    /// union, select only a single concrete function arm that the actual
    /// function semantically satisfies; erased or competing arms must not make
    /// adapter lowering guess.
    fn function_adapter_target(&mut self, actual: &Ty, expected: &Ty) -> Option<Ty> {
        let actual = self.table.resolve_completely(actual);
        let actual = self.expand_alias_ty(&actual);
        if !matches!(actual.kind(), InferTy::Function { .. }) {
            return None;
        }

        let expected = self.table.resolve_completely(expected);
        let expected = self.expand_alias_ty(&expected);
        match expected.kind() {
            InferTy::Function { .. } => Some(expected),
            InferTy::Union(..) => {
                fn collect(
                    this: &mut InferenceContext<'_>,
                    actual: &Ty,
                    ty: &Ty,
                    compatible: &mut Vec<Ty>,
                    fuel: u8,
                ) -> bool {
                    if fuel == 0 {
                        return false;
                    }
                    let candidate = this.expand_alias_ty(ty);
                    match candidate.kind() {
                        InferTy::Function { .. } => {
                            if this.function_adapter_candidate_compatible(actual, &candidate) {
                                compatible.push(candidate);
                            }
                            true
                        }
                        InferTy::Union(members, _) => {
                            let members = members.to_vec();
                            members.iter().all(|member| {
                                collect(this, actual, member, compatible, fuel.saturating_sub(1))
                            })
                        }
                        InferTy::TypeAlias(..) => false,
                        _ => true,
                    }
                }

                let mut compatible = Vec::new();
                if !collect(self, &actual, &expected, &mut compatible, 16) {
                    return None;
                }
                let [target] = compatible.as_slice() else {
                    return None;
                };
                Some(target.clone())
            }
            _ => None,
        }
    }

    /// Tests a union's function arm without committing any additional
    /// inference. Ground pairs can use the semantic oracle directly. An
    /// accepted generic function may still carry inference variables here,
    /// so probe the ordinary subtype relation under a table snapshot and
    /// discard any deferred work or obligations created by the probe.
    fn function_adapter_candidate_compatible(&mut self, actual: &Ty, candidate: &Ty) -> bool {
        if !actual.has_infer() && !candidate.has_infer() {
            return self.cached_subtype(actual, candidate);
        }

        let (
            InferTy::Function {
                params: actual_params,
                ret: actual_ret,
                throws: actual_throws,
                ..
            },
            InferTy::Function {
                params: candidate_params,
                ret: candidate_ret,
                throws: candidate_throws,
                ..
            },
        ) = (actual.kind(), candidate.kind())
        else {
            return false;
        };
        let actual_required: Vec<_> = actual_params
            .iter()
            .filter(|param| param.mode == baml_type::FunctionParamMode::Required)
            .collect();
        let candidate_required: Vec<_> = candidate_params
            .iter()
            .filter(|param| param.mode == baml_type::FunctionParamMode::Required)
            .collect();
        if actual_required.len() != candidate_required.len() {
            return false;
        }

        let snapshot = self.table.snapshot();
        let deferred_len = self.deferred_subs.len();
        let obligations_len = self.obligations.len();
        let mut compatible = true;
        for (actual, candidate) in actual_required.iter().zip(candidate_required.iter()) {
            compatible &= self.sub(&candidate.ty, &actual.ty);
        }
        for candidate in candidate_params
            .iter()
            .filter(|param| param.mode == baml_type::FunctionParamMode::Optional)
        {
            let Some(actual) = actual_params.iter().find(|actual| {
                actual.mode == baml_type::FunctionParamMode::Optional
                    && actual.name == candidate.name
            }) else {
                compatible = false;
                break;
            };
            compatible &= self.sub(&candidate.ty, &actual.ty);
        }
        compatible &= self.sub(actual_ret, candidate_ret);
        compatible &= self.sub(actual_throws, candidate_throws);
        self.table.rollback_to(snapshot);
        self.deferred_subs.truncate(deferred_len);
        self.obligations.truncate(obligations_len);
        compatible
    }

    fn record_function_adapter(&mut self, expr: ExprId, got: &Ty, expected: &Ty) {
        let got = self.table.resolve_completely(got);
        let got = self.expand_alias_ty(&got);
        let InferTy::Function { params: source, .. } = got.kind() else {
            return;
        };
        let target_fn = self.table.resolve_completely(expected);
        let target_fn = self.expand_alias_ty(&target_fn);
        let InferTy::Function { params: target, .. } = target_fn.kind() else {
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
                    && Int63::new(*v).is_none()
                {
                    self.pending_diags
                        .push(PendingDiag::IntLiteralOutOfRange { expr, value: *v });
                    &Literal::Int(0)
                } else {
                    lit
                };
                Ty::intern(InferTy::Literal(
                    lit.clone(),
                    Freshness::Fresh,
                    TyAttr::default(),
                ))
            }
            Expr::Null => Ty::null(),
            // A byte-string literal (`b"..."`) IS a `uint8array` value -
            // its own expr kind, not a `Literal` (no literal TYPE per
            // byte-string; TIR agrees).
            Expr::ByteStringLiteral(_) => Ty::intern(InferTy::Uint8Array {
                attr: TyAttr::default(),
            }),
            Expr::Path(segments) => self.resolve_value_path(body, expr, segments, expected),
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
                let type_scope = self.scoped_type_bindings.len();
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
                let ty = match tail_expr {
                    Some(tail) => self.infer_expr(body, *tail, expected),
                    // A tail-less block that always diverged is never;
                    // otherwise it is void.
                    None if self.diverges == Diverges::Always
                        && entry_diverges == Diverges::Maybe =>
                    {
                        Ty::never()
                    }
                    None => Ty::void(),
                };
                self.finish_scoped_type_bindings(type_scope, ty)
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.check_condition(body, *condition);
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
            Expr::QualifiedPath { member, .. } => {
                // A fully-qualified item reference used as a VALUE. Its own
                // generic suffix is fresh: only a call site can spell a
                // turbofish, exactly as for the two path spellings.
                let member = member.clone();
                self.qualified_path_value(expr, &member, OwnArgs::Fresh, Some(expr))
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
                let target =
                    if let Some(target_ref) = self.type_refs.upcast_targets.get(&expr).copied() {
                        let (lowered, diagnostics) = self.lower_body_type_ref_at(
                            target_ref,
                            crate::lower::TypePosition::Existential,
                        );
                        self.queue_body_lowering_diagnostics(diagnostics);
                        self.reject_expr_position_holes(&lowered, expr)
                    } else {
                        Ty::error()
                    };
                // The interface-view gate is a STRUCTURE demand: an
                // alias naming an interface answers as the interface.
                let target = self.expand_alias_ty(&target);
                if target.has_error() {
                    Ty::error()
                } else if !matches!(target.kind(), InferTy::Interface(..)) {
                    self.pending_diags.push(PendingDiag::QualifierNotInterface {
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
                        self.pending_diags
                            .push(PendingDiag::QualifierNotImplemented {
                                expr,
                                value: base_ty,
                                interface: target.clone(),
                            });
                    }
                    target
                }
            }
            Expr::GenericApply { base, .. } => {
                self.validate_runtime_type_arg_operands(body, expr);
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
                        InferTy::Function { params, ret, .. } => {
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
                                        InferTy::Function { ret: body_ret, .. }
                                            if matches!(
                                                body_ret.kind(),
                                                InferTy::Class(qtn, _, _)
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
                                    && let InferTy::Function {
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
                        // honest replacement for TIR's evolving-list sentinel.
                        self.untyped_empty_container_ty(expr, Ty::list)
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
                    for entry in entries {
                        self.check_expr(body, entry.key, &key_ty);
                        self.check_expr(body, entry.value, &value_ty);
                    }
                    Ty::intern(InferTy::Map {
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
                    self.untyped_empty_container_ty(expr, |value| {
                        Ty::intern(InferTy::Map {
                            key: Ty::string(),
                            value,
                            attr: TyAttr::default(),
                        })
                    })
                } else {
                    let (keys, values): (Vec<Ty>, Vec<Ty>) = entries
                        .iter()
                        .map(|entry| {
                            let key_ty = self.infer_expr(body, entry.key, &Expectation::None);
                            let value_ty = self.infer_expr(body, entry.value, &Expectation::None);
                            (self.widen_fresh(&key_ty), self.widen_fresh(&value_ty))
                        })
                        .unzip();
                    Ty::intern(InferTy::Map {
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
                self.infer_return(body, *value, None, Some(expr));
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
                self.validate_runtime_type_arg_operands(body, expr);
                // `map { .. }` is a map literal in constructor clothing
                // (identifier keys are string keys), never a class named
                // `map` - same routing guard as the parser's object form.
                if type_args.is_empty()
                    && spreads.is_empty()
                    && matches!(type_name.0.as_slice(), [seg] if seg.as_str() == "map")
                {
                    if let Some((key_ty, value_ty)) = self.expected_map_entry(expected) {
                        for field in fields {
                            self.check_expr(body, field.value, &value_ty);
                        }
                        Ty::intern(InferTy::Map {
                            key: key_ty,
                            value: value_ty,
                            attr: TyAttr::default(),
                        })
                    } else if fields.is_empty() {
                        self.untyped_empty_container_ty(expr, |value| {
                            Ty::intern(InferTy::Map {
                                key: Ty::string(),
                                value,
                                attr: TyAttr::default(),
                            })
                        })
                    } else {
                        let values: Vec<Ty> = fields
                            .iter()
                            .map(|field| {
                                let value_ty =
                                    self.infer_expr(body, field.value, &Expectation::None);
                                self.widen_fresh(&value_ty)
                            })
                            .collect();
                        Ty::intern(InferTy::Map {
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
                    self.report_chain_null_escape(body, *inner);
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
                self.validate_runtime_type_arg_operands(body, expr);
                self.optional_call_callee_depth += 1;
                let callee_ty = self.infer_expr(body, *callee, &Expectation::None);
                self.optional_call_callee_depth -= 1;
                self.report_mounted_reserved_call(expr, *callee);
                self.check_needless_chain(body, expr, *callee, &callee_ty);
                let nonnull = self.peel_chain_null(&callee_ty);
                let args = args.clone();
                let ret = self.check_call_args(body, expr, *callee, &nonnull, false, &args);
                self.report_runtime_indirect_call(expr, *callee);
                ret
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
        self.report_unspecialized_generic_method_value(body, expr, expected, &ty);
        self.result.type_of_expr.insert(expr, ty.clone());
        ty
    }

    fn infer_stmt(&mut self, body: &ExprBody, stmt: StmtId) {
        match &body.stmts[stmt] {
            Stmt::Expr(expr) => {
                self.infer_expr(body, *expr, &Expectation::None);
            }
            Stmt::TypeBinding { name, value } => {
                let Some(type_ref) = self.type_refs.stmt_type_bindings.get(&stmt).copied() else {
                    return;
                };
                self.bind_scoped_runtime_type(body, stmt, name.clone(), value, type_ref);
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
                self.infer_return(body, *value, Some(stmt), None);
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
                let condition_ty = self.check_condition(body, *condition);
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
                // ...except when the condition is statically `true` and no
                // `break` binds to this loop: then there is no zero-iteration
                // path and no exit edge, so the loop DIVERGES and everything
                // after it is unreachable - no false facts to apply.
                let never_exits =
                    self.condition_is_statically_true(body, *condition, &condition_ty)
                        && !Self::loop_body_breaks(body, *loop_body, *after);
                self.diverges = saved.or(if never_exits {
                    Diverges::Always
                } else {
                    Diverges::Maybe
                });
                self.flow = entry_flow;
                if !never_exits {
                    self.apply_facts(&facts.when_false);
                }
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
                    InferTy::List(element, _) => element.clone(),
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

    /// Whether a loop condition is `true` on every iteration.
    ///
    /// The oracle is the condition's INFERRED TYPE, not its syntax, so its
    /// reach is wider than the literal `while (true)`. Anything that lands on
    /// the literal type `true` answers yes:
    ///
    /// - the literal itself, which is also matched syntactically so the
    ///   answer never depends on inference succeeding;
    /// - a constant fold — comparisons over literal operands close under
    ///   `const_fold_binary`, so `while (1 == 1)` qualifies;
    /// - a flow-narrowed binding — after `if (c is true)`, `while (c)` sees
    ///   the narrowed `true`;
    /// - a call whose return type is declared `-> true`.
    ///
    /// All of those are genuinely true on every iteration, which is what
    /// divergence needs. Narrowed bindings stay sound because a loop havocs
    /// every binding its body assigns before the condition is checked, so a
    /// binding the loop can falsify is no longer narrowed here. A condition
    /// that merely happens to be true at runtime is not, and must not be,
    /// recognized. `for (;;)` is out of scope: its empty condition lowers to
    /// `Expr::Missing`, not to a literal.
    fn condition_is_statically_true(
        &mut self,
        body: &ExprBody,
        condition: ExprId,
        condition_ty: &Ty,
    ) -> bool {
        if matches!(
            &body.exprs[condition],
            Expr::Literal(baml_type::Literal::Bool(true))
        ) {
            return true;
        }
        // Truthiness (B-1563) widens "statically true" beyond the literal
        // `true` type: any always-truthy condition (`while ("x")`, an
        // instance, a closure) has no exit edge either.
        matches!(
            crate::infer::truthy::truthiness(&self.table.resolve_completely(condition_ty)),
            crate::infer::truthy::Truthiness::AlwaysTruthy
        )
    }

    fn bind_scoped_runtime_type(
        &mut self,
        body: &ExprBody,
        stmt: StmtId,
        name: baml_type::Name,
        value: &baml_compiler2_ast::TypeExpr,
        type_ref: BodyTypeRefId,
    ) {
        let direct_operand = match &value.kind {
            baml_compiler2_ast::TypeExprKind::Unreflect {
                operand: Some(operand),
                ..
            } => Some(*operand),
            _ => None,
        };
        // The RHS is evaluated before the new name enters scope. A composite
        // first synthesizes its nested slots, then materializes the template
        // into the named slot.
        let template_ty = if let Some(operand) = direct_operand {
            self.validate_runtime_type_operand(body, operand);
            None
        } else {
            let (ty, diagnostics) =
                self.lower_body_type_ref_at(type_ref, crate::lower::TypePosition::Existential);
            self.queue_body_lowering_diagnostics(diagnostics);
            // A composite template is a stored/structural position: a `_`
            // inside it is a ruling-4 rejection, never a fresh variable.
            Some(Ty::from_plain(&crate::lower::reject_holes(&ty)))
        };
        let mut identity = self.body_owner_identity;
        for byte in stmt.into_raw().into_u32().to_le_bytes() {
            identity ^= u32::from(byte);
            identity = identity.wrapping_mul(0x0100_0193);
        }
        let binding = ScopedTypeBinding {
            name: name.clone(),
            parameter: baml_type::ParamTy::new(0x8000_0000 | (identity & 0x7fff_ffff), name),
            operand: direct_operand,
            template_ty,
            occurrence_ty: Ty::intern(InferTy::Unknown {
                attr: TyAttr::default(),
            }),
        };
        self.result.type_bindings.insert(stmt, binding.clone());
        self.scoped_type_bindings.push(binding);
    }

    /// Erase every binding introduced by this block from values that can flow
    /// out, then restore the lexical overlay checkpoint. Interior expression
    /// and pattern tables intentionally retain the rigid identity for MIR.
    fn finish_scoped_type_bindings(&mut self, checkpoint: usize, mut block_ty: Ty) -> Ty {
        for binding in self.scoped_type_bindings[checkpoint..].iter().rev() {
            block_ty = replace_rigid_param(&block_ty, &binding.parameter, &binding.occurrence_ty);
            for ty in self.flow.values_mut() {
                *ty = replace_rigid_param(ty, &binding.parameter, &binding.occurrence_ty);
            }
            // The effect channel leaves the block exactly as the value does.
            // An undeclared `throws` is assembled from these contributions at
            // finalize, so a rigid parameter left in one becomes part of the
            // OWNER's published effect — a type naming a parameter that stops
            // existing at the closing brace, which reaches lowering with no
            // type argument to bind it to.
            for channel in &mut self.throws_channels {
                for (_, contribution) in channel.iter_mut() {
                    *contribution = replace_rigid_param(
                        contribution,
                        &binding.parameter,
                        &binding.occurrence_ty,
                    );
                }
            }
            for frame in &mut self.return_frames {
                if let Some(expected) = &mut frame.expected {
                    *expected =
                        replace_rigid_param(expected, &binding.parameter, &binding.occurrence_ty);
                }
                for candidate in &mut frame.candidates {
                    *candidate =
                        replace_rigid_param(candidate, &binding.parameter, &binding.occurrence_ty);
                }
            }
            // A contract violation stashed inside the block quotes the effect
            // it saw. `extra` is a COPY of a contribution — compiler-derived,
            // and quoted in a report about what the enclosing function may
            // throw — so it is erased for the same reason the contribution is.
            //
            // `declared` deliberately is NOT. It is the clause an author WROTE
            // at that site: only a lambda's own clause can name a block-scoped
            // binding, its violation is anchored inside that block, and the
            // line one row above the caret reads `throws Boom<Out>`. Erasing
            // it would print `Boom<unknown>` next to the user's own `Out` —
            // `a_lambda_clause_inside_the_block_is_quoted_as_written` pins
            // both halves of that asymmetry.
            for pending in &mut self.pending_diags {
                match pending {
                    PendingDiag::ThrowsViolation { extra, .. } => {
                        *extra =
                            replace_rigid_param(extra, &binding.parameter, &binding.occurrence_ty);
                    }
                    PendingDiag::ReturnTypeMismatch {
                        expected, actual, ..
                    } => {
                        *expected = replace_rigid_param(
                            expected,
                            &binding.parameter,
                            &binding.occurrence_ty,
                        );
                        *actual =
                            replace_rigid_param(actual, &binding.parameter, &binding.occurrence_ty);
                    }
                    _ => {}
                }
            }
        }
        self.scoped_type_bindings.truncate(checkpoint);
        block_ty
    }

    fn scoped_type_params(&self) -> Vec<baml_type::ParamTy> {
        self.scoped_type_bindings
            .iter()
            .map(|binding| binding.parameter.clone())
            .collect()
    }

    fn scoped_type_param(&self, name: &baml_type::Name) -> Option<&baml_type::ParamTy> {
        self.scoped_type_bindings
            .iter()
            .rev()
            .find(|binding| &binding.name == name)
            .map(|binding| &binding.parameter)
    }

    fn lower_scoped_type_ref_at(
        &mut self,
        store: &baml_compiler2_hir::type_ref::TypeRefStore,
        type_ref: baml_compiler2_hir::type_ref::TypeRefId,
        position: crate::lower::TypePosition,
    ) -> (baml_type::LoweringTy, Vec<crate::lower::LoweringDiag>) {
        let mut runtime_params = FxHashMap::default();
        let mut occurrences = Vec::new();
        collect_unreflect_type_refs(store, type_ref, &mut occurrences);
        let mut bindings = Vec::new();
        for (runtime_ref, operand) in occurrences {
            let binding = if let Some(binding) = self.synthesized_type_bindings.get(&runtime_ref) {
                binding.clone()
            } else {
                let mut identity = self.body_owner_identity;
                for byte in runtime_ref.into_raw().into_u32().to_le_bytes() {
                    identity ^= u32::from(byte);
                    identity = identity.wrapping_mul(0x0100_0193);
                }
                let name = baml_type::Name::new(format!("$unreflect${identity:08x}"));
                let binding = ScopedTypeBinding {
                    name: name.clone(),
                    parameter: baml_type::ParamTy::new(
                        0xa000_0000 | (identity & 0x1fff_ffff),
                        name,
                    ),
                    operand: Some(operand),
                    template_ty: None,
                    occurrence_ty: Ty::intern(InferTy::Unknown {
                        attr: TyAttr::default(),
                    }),
                };
                self.synthesized_type_bindings
                    .insert(runtime_ref, binding.clone());
                self.scoped_type_bindings.push(binding.clone());
                binding
            };
            runtime_params.insert(runtime_ref, binding.parameter.clone());
            bindings.push(binding);
        }
        if !bindings.is_empty() {
            self.result
                .type_ref_bindings
                .insert(type_ref, bindings.into_boxed_slice());
        }
        self.lower
            .lower_type_ref_with_runtime_bindings_and_diagnostics(
                store,
                type_ref,
                position,
                &self.scoped_type_params(),
                &runtime_params,
            )
    }

    fn queue_body_lowering_diagnostics(&mut self, diagnostics: Vec<crate::lower::LoweringDiag>) {
        self.pending_diags
            .extend(
                diagnostics
                    .into_iter()
                    .map(|diagnostic| PendingDiag::BodyAnnot {
                        type_ref: self.type_refs.diagnostic_id(diagnostic.type_ref),
                        kind: diagnostic.kind,
                    }),
            );
    }

    fn lower_body_type_ref_at(
        &mut self,
        type_ref: BodyTypeRefId,
        position: crate::lower::TypePosition,
    ) -> (baml_type::LoweringTy, Vec<crate::lower::LoweringDiag>) {
        let type_refs = Arc::clone(&self.type_refs);
        self.lower_scoped_type_ref_at(&type_refs.store, type_refs.raw_id(type_ref), position)
    }
    fn lower_scoped_type_path(&self, segments: &[baml_type::Name]) -> baml_type::LoweringTy {
        self.lower
            .lower_type_path_with_overlay(segments, &self.scoped_type_params())
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
                            if matches!(resolved.kind(), InferTy::Void { .. }) {
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
                    self.provable_subtype(&narrowed, declared)
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
            let saved_flow = self.flow.clone();
            let saved = std::mem::replace(&mut self.diverges, Diverges::Maybe);
            let got = self.infer_expr(body, else_expr, &Expectation::None);
            let branch_diverges = self.diverges;
            self.diverges = saved;
            self.flow = saved_flow;
            let resolved = self.table.resolve_completely(&got);
            if branch_diverges != Diverges::Always
                && !resolved.has_error()
                && !matches!(resolved.kind(), InferTy::Never { .. })
            {
                self.pending_diags.push(PendingDiag::LetElseMustDiverge {
                    expr: else_expr,
                    got,
                });
            }
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
        // A function-type alias used as contextual type information must
        // expose its parameter/return slots so they can determine a generic
        // function value's instantiation (`let f: StringCallback = identity`).
        // Keep the normalization shape-directed: expanding every alias here
        // would erase nominal alias identity from unrelated diagnostics.
        if matches!(actual.kind(), InferTy::Function { .. })
            && matches!(expected.kind(), InferTy::TypeAlias(..))
        {
            expected = self.expand_alias_ty(&expected);
        }
        if matches!(actual.kind(), InferTy::TypeAlias(..))
            && matches!(expected.kind(), InferTy::Function { .. })
        {
            actual = self.expand_alias_ty(&actual);
        }
        // Normalize-then-relate (rustc's FnCtxt normalize-before-unify;
        // r-a's `normalize_projection_ty` during unification): a GROUND
        // projection the oracle can already determine reduces before the
        // pair is related, so `(Iterator<Item = int> as Iterable).Item[]`
        // meets `int[]`. VAR-CARRYING projections must not enter the
        // reduction (the oracle speaks the plain algebra, whose
        // conversion erases inference vars); they relate as lazy
        // predicates through the deferred residue (`eq_piece`).
        if actual.has_projection()
            && let Ok(closed) = ClosedTy::try_from(&actual)
        {
            actual = self
                .reduce_projections(&closed, PROJECTION_FINALIZE_FUEL)
                .into_ty();
        }
        if expected.has_projection()
            && let Ok(closed) = ClosedTy::try_from(&expected)
        {
            expected = self
                .reduce_projections(&closed, PROJECTION_FINALIZE_FUEL)
                .into_ty();
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
        if matches!(expected.kind(), InferTy::Unknown { .. })
            && !matches!(actual.kind(), InferTy::InferVar { .. })
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
            (InferTy::InferVar { var, .. }, _) => {
                self.table.add_upper_bound(*var, expected.clone());
                if let InferTy::InferVar { var: other, .. } = expected.kind() {
                    self.table.add_lower_bound(*other, actual.clone());
                }
                true
            }
            (_, InferTy::InferVar { var, .. }) => {
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
            (InferTy::Union(members, _), _) if actual.has_infer() => {
                let members: Vec<Ty> = members.to_vec();
                let mut ok = true;
                for member in members {
                    ok &= self.sub(&member, &expected);
                }
                ok
            }
            // A var-carrying value flowing into a GROUND union can use a
            // single structurally compatible arm as context. This is the
            // optional-callback case: a function can only inhabit the
            // function arm of `Callback | null`, so that arm may determine
            // its generic slots. Multiple compatible arms remain ambiguous
            // and stay deferred (`Fn<int> | Fn<string>` must not guess).
            (_, InferTy::Union(members, _)) if actual.has_infer() && !expected.has_infer() => {
                let targets: Vec<Ty> = members
                    .iter()
                    .map(|member| self.expand_alias_ty(member))
                    .filter(|member| same_head_constructor(&actual, member))
                    .collect();
                if let [target] = targets.as_slice() {
                    let target = target.clone();
                    return self.sub(&actual, &target);
                }
                self.deferred_subs
                    .push((actual, expected, self.obligation_anchor));
                true
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
            (_, InferTy::Union(members, _)) if expected.has_infer() => {
                let members: Vec<Ty> = members.to_vec();
                let actual_members: Vec<Ty> = match actual.kind() {
                    InferTy::Union(actual_members, _) => actual_members.to_vec(),
                    _ => vec![actual.clone()],
                };
                let (naked, targets): (Vec<Ty>, Vec<Ty>) = members
                    .into_iter()
                    .partition(|member| matches!(member.kind(), InferTy::InferVar { .. }));
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
            (InferTy::Class(a_name, a_args, _), InferTy::Class(b_name, b_args, _))
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
            (InferTy::List(a, _), InferTy::List(b, _)) => {
                let (a, b) = (a.clone(), b.clone());
                self.eq_piece(&a, &b)
            }
            (
                InferTy::Map {
                    key: ak, value: av, ..
                },
                InferTy::Map {
                    key: bk, value: bv, ..
                },
            ) => {
                let (ak, av, bk, bv) = (ak.clone(), av.clone(), bk.clone(), bv.clone());
                let key_ok = self.eq_piece(&ak, &bk);
                let value_ok = self.eq_piece(&av, &bv);
                key_ok && value_ok
            }
            (InferTy::Future(av, ae, _), InferTy::Future(bv, be, _)) => {
                let (av, ae, bv, be) = (av.clone(), ae.clone(), bv.clone(), be.clone());
                let value_ok = self.eq_piece(&av, &bv);
                let error_ok = self.eq_piece(&ae, &be);
                value_ok && error_ok
            }
            // Function types: contravariant params, covariant ret/throws.
            (
                InferTy::Function {
                    params: a_params,
                    ret: a_ret,
                    throws: a_throws,
                    ..
                },
                InferTy::Function {
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
            // `reflect.AnyFunction` is compiler-derived for concrete function
            // values, rather than an ordinary impl obligation.  When a
            // generic consumer such as `reflect.call_any<R, E>` receives a
            // function value directly, carry its output channels into the
            // requested associated pins so `R`/`E` are inferred from the
            // function signature.  The generic-interface fallback below can
            // prove conformance, but it has no way to recover these pins and
            // leaves the call's type arguments as `Error`.
            (
                InferTy::Function {
                    ret: actual_ret,
                    throws: actual_throws,
                    ..
                },
                InferTy::Interface(name, _, expected_pins, _),
            ) if name.is_reflect_root_type("AnyFunction") => {
                let mut ok = true;
                for (pin, expected_pin) in expected_pins {
                    let Some(actual_pin) = (match pin.as_str() {
                        "Returns" => Some(actual_ret),
                        "Throws" => Some(actual_throws),
                        _ => None,
                    }) else {
                        continue;
                    };
                    ok &= self.sub(actual_pin, expected_pin);
                }
                ok
            }
            _ => {
                // Ground on both sides: one oracle verdict. Otherwise the
                // pair is the deferred residue.
                let actual = self.table.resolve_completely(&actual);
                let expected = self.table.resolve_completely(&expected);
                if !actual.has_infer() && !expected.has_infer() {
                    self.cached_subtype(&actual, &expected)
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
                        InferTy::Interface(a_name, a_args, a_pins, _),
                        InferTy::Interface(b_name, b_args, b_pins, _),
                    ) = (actual.kind(), expected.kind())
                        && a_name == b_name
                        && a_args.len() == b_args.len()
                        && (a_name.is_reflect_root_type("AnyFunction")
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
                                        Ty::intern(InferTy::Unknown {
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
                    if let InferTy::Interface(name, args, pins, _) = expected.kind()
                        && !matches!(actual.kind(), InferTy::InferVar { .. })
                        && let Some(anchor) = self.obligation_anchor
                    {
                        let interface = baml_type::interned::InferInterface::new(
                            name.clone(),
                            args.clone(),
                            pins.clone(),
                        );
                        // A union is a subtype of an existential iff ALL
                        // members are (spec: Variance rule 2.1) - and
                        // only concrete types implement, so the goal
                        // decomposes per member before registration.
                        let goals: Vec<Ty> = match actual.kind() {
                            InferTy::Union(members, _) => members.to_vec(),
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
        if let InferTy::InferVar { var, .. } = resolved.kind() {
            if self
                .table
                .unsolved_policy(*var)
                .is_some_and(unify::VarPolicy::absorbs_unknown)
            {
                self.table.solve(
                    *var,
                    Ty::intern(InferTy::Unknown {
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
            return self.cached_equivalent(&a, &b);
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

    fn cached_equivalent(&self, a: &Ty, b: &Ty) -> bool {
        use baml_type::interned::ClosedTy;
        match (ClosedTy::try_from(a), ClosedTy::try_from(b)) {
            (Ok(a), Ok(b)) => self.canonical_cache.equivalent(&a, &b, &self.facts),
            // An OPEN operand: only shallow identity decides before
            // resolution — fail closed (a mismatch recorded mid-flight is
            // re-judged at finish on the finalized types).
            _ => a == b,
        }
    }

    fn cached_subtype(&self, sub: &Ty, sup: &Ty) -> bool {
        use baml_type::interned::ClosedTy;
        match (ClosedTy::try_from(sub), ClosedTy::try_from(sup)) {
            (Ok(sub), Ok(sup)) => self.canonical_cache.is_subtype(&sub, &sup, &self.facts),
            _ => sub == sup,
        }
    }

    /// A PROVABLE subtype verdict: ground on both sides and confirmed by this
    /// inference body's paired facts and canonical cache. Rigid or unresolved
    /// pairs are not provable, the conservative direction for coverage and
    /// claiming.
    fn provable_subtype(&self, sub: &Ty, sup: &Ty) -> bool {
        if sub == sup {
            return true;
        }
        if sub.has_error() || sup.has_error() {
            return false;
        }
        let (Ok(sub), Ok(sup)) = (
            baml_type::interned::ClosedTy::try_from(sub),
            baml_type::interned::ClosedTy::try_from(sup),
        ) else {
            // Unresolved pairs are not provable — the conservative
            // direction for coverage and claiming.
            return false;
        };
        // Rigid variables go to the oracle too: its typevar arms are already
        // conservative (`T <: T`, `T <: unknown`, `never <: T` prove; a rigid
        // against an unrelated concrete does not - which is exactly the B-633
        // rule). The corpus pins the case this matters for: a synthetic effect
        // var IS covered by `throws unknown`.
        self.canonical_cache.is_subtype(&sub, &sup, &self.facts)
    }

    /// A union of members that may still contain inference variables. The
    /// canonical algebra consults the semantic oracle and REQUIRES
    /// var-free input (the normalizer's invariant), so a var-containing
    /// join stays syntactic until resolution - the S13 finalize pass
    /// re-canonicalizes once every variable is solved or ruled an error.
    fn union_of(&mut self, members: &[Ty]) -> Ty {
        let closed: Result<Vec<_>, _> = members
            .iter()
            .map(baml_type::interned::ClosedTy::try_from)
            .collect();
        match closed {
            Ok(closed) => canonical_union_interned(&closed, &self.facts).into_ty(),
            Err(baml_type::interned::OpenTy) => syntactic_union(members),
        }
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
            if let InferTy::Literal(lit, freshness, _) = ty.kind() {
                match freshness {
                    Freshness::Fresh => fresh.push(lit.clone()),
                    Freshness::Regular => regular.push(lit.clone()),
                }
            }
        };
        for member in members {
            match member.kind() {
                InferTy::Union(inner, _) => inner.iter().for_each(&mut collect),
                _ => collect(member),
            }
        }
        let joined = self.union_of(members);
        if fresh.is_empty() {
            return joined;
        }
        let remark = |ty: &Ty| -> Ty {
            match ty.kind() {
                InferTy::Literal(lit, Freshness::Regular, attr)
                    if fresh.contains(lit) && !regular.contains(lit) =>
                {
                    Ty::intern(InferTy::Literal(
                        lit.clone(),
                        Freshness::Fresh,
                        attr.clone(),
                    ))
                }
                _ => ty.clone(),
            }
        };
        match joined.kind() {
            InferTy::Union(joined_members, attr) => Ty::intern(InferTy::Union(
                joined_members.iter().map(remark).collect(),
                attr.clone(),
            )),
            _ => remark(&joined),
        }
    }

    fn join_return_candidates(&mut self, candidates: &[Ty]) -> Ty {
        let mut candidates = candidates
            .iter()
            .filter(|candidate| !matches!(candidate.kind(), InferTy::Never { .. }));
        let Some(first) = candidates.next() else {
            return Ty::never();
        };
        let rest: Vec<_> = candidates.collect();
        if rest.iter().all(|candidate| *candidate == first) {
            return first.clone();
        }
        let mut members = Vec::with_capacity(rest.len() + 1);
        members.push(first.clone());
        members.extend(rest.into_iter().cloned());
        self.join(&members)
    }

    /// Fresh literals widen to their base primitive at binding sites and
    /// joins; a union of fresh literals widens member-wise and
    /// re-canonicalizes (`1 | 2` at a binding is `int`).
    fn widen_fresh(&mut self, ty: &Ty) -> Ty {
        match ty.kind() {
            InferTy::Literal(_, Freshness::Fresh, _) => widen_fresh_literal(ty),
            InferTy::Union(members, _)
                if members.iter().any(|member| {
                    matches!(member.kind(), InferTy::Literal(_, Freshness::Fresh, _))
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
                let lhs_ty = self.check_condition(body, lhs);
                let rhs_ty = self.check_condition(body, rhs);
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
                if !lhs_ty.has_error()
                    && !rhs_ty.has_error()
                    && let (Ok(lhs_closed), Ok(rhs_closed)) =
                        (ClosedTy::try_from(&lhs_ty), ClosedTy::try_from(&rhs_ty))
                    && let Some(equal) = baml_type::normalize::TypeContext::constant_equality(
                        &self.facts,
                        &lhs_closed.to_plain(),
                        &rhs_closed.to_plain(),
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
                    return Ty::intern(InferTy::Literal(
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
                // (cascades; a var may still solve either way) - but the
                // `unknown` top type does NOT skip: it is a type the reader
                // wrote, not a cascade, and it orders against nothing, so it
                // reports here like any other non-comparable operand.
                if !lhs_ty.has_infer()
                    && !rhs_ty.has_infer()
                    && !lhs_ty.has_error()
                    && !rhs_ty.has_error()
                {
                    let widen = |this: &mut Self, ty: &Ty| -> baml_type::Ty {
                        use baml_base::Literal as Lit;
                        let expanded = this.expand_alias_ty(ty);
                        let plain = this.materialize_ty(&expanded);
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
                            Box::new([]),
                            Box::new([]),
                            baml_type::TyAttr::default(),
                        );
                        let comparable = !matches!(
                            lhs_base,
                            baml_type::Ty::Union(..) | baml_type::Ty::Interface(..)
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
                let dispatch = crate::ops::binary_operator(op)
                    .unwrap_or_else(|| unreachable!("outer match arm covers dispatching ops"));
                self.operator_or_obligation(expr, dispatch.interface, &lhs_ty, Some(&rhs_ty))
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
                let dispatch = crate::ops::binary_operator(op)
                    .unwrap_or_else(|| unreachable!("outer match arm covers dispatching ops"));
                self.operator_or_obligation(expr, dispatch.interface, &lhs_ty, Some(&rhs_ty))
            }
        }
    }

    /// `spawn name? with...? { body } : Future<T, E>` (BEP-034; rustc's
    /// async-block shape). The body arrives as a synthetic 0-arg lambda
    /// and types through the ordinary lambda path - its OWN effect
    /// channel (the S12 discipline) is the future's error side, read
    /// straight off the lambda's fn type. Fresh literals widen out of
    /// both slots. `with` transformers fold left-to-right over
    /// `Params<T, E>`: each checks against
    /// `(Params<cur>) -> Params<unknown, unknown>`, the
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
            InferTy::Function { ret, throws, .. } => (ret.clone(), throws.clone()),
            _ => (resolved.clone(), Ty::never()),
        };
        let mut cur_value = self.widen_fresh(&value);
        let mut cur_error = self.widen_fresh(&error);
        for &with_id in with_exprs {
            let unknown = || {
                Ty::intern(InferTy::Unknown {
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
            let expected = Ty::intern(InferTy::Function {
                params: Box::new([baml_type::interned::InferFunctionParamTy {
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
                InferTy::Function { params, ret, .. } => {
                    let ret = ret.clone();
                    let ret = self.structurally_resolve(&ret);
                    match ret.kind() {
                        InferTy::Class(qn, args, _)
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
                                InferTy::Class(_, args, _) => {
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
                    if (is_value_ref || matches!(got_resolved.kind(), InferTy::Function { .. }))
                        && !got_resolved.has_error()
                        && !matches!(got_resolved.kind(), InferTy::Unknown { .. })
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
        Ty::intern(InferTy::Future(cur_value, cur_error, TyAttr::default()))
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
            InferTy::Future(value, error, _) => {
                let (value, error) = (value.clone(), error.clone());
                self.record_throw(expr, &error);
                value
            }
            InferTy::Union(members, _)
                if !members.is_empty()
                    && members
                        .iter()
                        .all(|member| matches!(member.kind(), InferTy::Future(..))) =>
            {
                let mut values = Vec::new();
                for member in members {
                    if let InferTy::Future(value, error, _) = member.kind() {
                        values.push(value.clone());
                        let error = error.clone();
                        self.record_throw(expr, &error);
                    }
                }
                self.union_of(&values)
            }
            InferTy::Never { .. } => resolved,
            InferTy::InferVar { .. } => {
                let value = self.table.new_var_ty();
                let error = self.table.new_var_ty_of(unify::VarPolicy::Effect);
                let demanded = Ty::intern(InferTy::Future(
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
                    Ty::intern(InferTy::Unknown {
                        attr: TyAttr::default(),
                    })
                };
                let expected = Ty::intern(InferTy::Future(unknown(), unknown(), TyAttr::default()));
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
        // The reporting road (an inapplicable compound operator is the
        // same E0004 the binary spelling gets), anchored at the value.
        let dispatch = crate::ops::assign_operator(op);
        self.operator_or_obligation(at, dispatch.interface, lhs, Some(rhs))
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
            InferTy::List(element, _) => {
                let element = element.clone();
                let expectation = key_expectation(self, Ty::int());
                self.check_expr(body, index, &expectation);
                element
            }
            InferTy::Map { key, value, .. } => {
                let key = key.clone();
                let value = value.clone();
                let expectation = key_expectation(self, key);
                self.check_expr(body, index, &expectation);
                value
            }
            InferTy::Uint8Array { .. } => {
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
                let ty = self.check_not_operand(body, operand);
                // `!` on a LITERAL constant-FOLDS through its truthiness
                // (TIR's `try_fold_unary`, extended to the non-bool
                // literals truthiness admits), freshness preserved.
                let resolved = self.table.resolve_completely(&ty);
                if let InferTy::Literal(_, freshness, _) = resolved.kind() {
                    let negated = match crate::infer::truthy::truthiness(&resolved) {
                        crate::infer::truthy::Truthiness::AlwaysTruthy => false,
                        crate::infer::truthy::Truthiness::AlwaysFalsy => true,
                        crate::infer::truthy::Truthiness::Runtime => return Ty::bool(),
                    };
                    return Ty::intern(InferTy::Literal(
                        Literal::Bool(negated),
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
                if let InferTy::Literal(lit, freshness, _) = resolved.kind()
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
    /// committed E0004 (an error or inference-var operand is a cascade and
    /// stays silent, the same `tainted_by_errors` rule everywhere).
    pub(super) fn report_operator_failure(
        &mut self,
        at: ExprId,
        interface: &'static str,
        lhs: &Ty,
        rhs: Option<&Ty>,
    ) {
        // SHALLOW error screening (TIR's gate): a nested error slot (an
        // incomplete existential's recovered pin) does not silence the
        // operator report - the operand as written still has no impl, and
        // that is this diagnostic's claim.
        //
        // The `unknown` top type is NOT screened: it is a type the reader
        // wrote, not a cascade from an earlier failure, and it implements
        // no operator - so `a + 1` on an `unknown` is a real E0004 rather
        // than something to stay quiet about. Only the error sentinel and
        // an unresolved inference var are cascades. (`dispatch_operator`
        // still suppresses the RESULT to the sentinel for unknown - that is
        // the ordinary diagnose-then-fill split, not a disagreement.)
        let dirty = |ty: &Ty| matches!(ty.kind(), InferTy::Error { .. }) || ty.has_infer();
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
        if matches!(lhs.kind(), InferTy::Never { .. })
            || rhs
                .as_ref()
                .is_some_and(|ty| matches!(ty.kind(), InferTy::Never { .. }))
        {
            return Ty::never();
        }
        let undispatchable = |ty: &Ty| {
            ty.has_error() || ty.has_infer() || matches!(ty.kind(), InferTy::Unknown { .. })
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
        if let InferTy::TypeVar(param, _) = lhs.kind() {
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
            return Some(Ty::intern(InferTy::AssociatedTypeProjection {
                base: lhs.clone(),
                interface: baml_type::interned::InferInterface::new(
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
        if let InferTy::Interface(qtn, args, pins, _) = lhs.kind() {
            let root =
                baml_type::interned::InferInterface::new(qtn.clone(), args.clone(), pins.clone());
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
            InferTy::Null { .. } => Ty::never(),
            InferTy::Union(members, _) => {
                let non_null: Vec<Ty> = members
                    .iter()
                    .filter(|member| !matches!(member.kind(), InferTy::Null { .. }))
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
            !ty.has_infer() && !ty.has_error() && !matches!(ty.kind(), InferTy::Never { .. })
        };
        if ground(&inner) && ground(&rhs) {
            if self.cached_subtype(&rhs, &inner) {
                return inner;
            }
            if self.cached_subtype(&inner, &rhs) {
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
        self.validate_runtime_type_arg_operands(body, call);
        let (callee_fn_ty, bound_receiver) = self.infer_callee(body, call, callee);
        self.report_mounted_reserved_call(call, callee);
        self.report_runtime_streaming_call(body, call, callee);
        self.seed_implicit_llm_schema(body, call, callee, args);
        let ret = self.check_call_args(body, call, callee, &callee_fn_ty, bound_receiver, args);
        self.report_runtime_indirect_call(call, callee);
        self.default_uncontracted_session_eval(body, call, callee);
        ret
    }

    fn report_runtime_indirect_call(&mut self, call: ExprId, callee: ExprId) {
        let Some(plan) = self.result.call_plans.get(&call) else {
            return;
        };
        let requires_runtime_check = plan
            .slots
            .iter()
            .any(|slot| match slot {
                CallTypeArgPlan::Runtime { .. } => true,
                CallTypeArgPlan::Static {
                    runtime_bindings, ..
                } => !runtime_bindings.is_empty(),
            })
            || !plan.deferred_checks.is_empty()
            || self.result.runtime_checks.iter().any(|check| match check {
                RuntimeCheck::Argument { arg, .. } => plan.bindings.iter().any(|binding| {
                    matches!(binding, ParamBinding::Provided { arg: provided, .. } if provided == arg)
                }),
                RuntimeCheck::Bound { .. } => !self.scoped_type_bindings.is_empty(),
            });
        if !requires_runtime_check {
            return;
        }

        let resolution = self.result.member_resolutions.get(&callee).or_else(|| {
            self.result
                .path_resolutions
                .get(&callee)
                .and_then(|path| path.segments.last())
                .and_then(|segment| segment.resolution.as_ref())
        });
        let direct_or_checked = matches!(
            resolution,
            Some(
                MemberResolution::Free { .. }
                    | MemberResolution::BoundMethod { .. }
                    | MemberResolution::UnboundMethod { .. }
                    | MemberResolution::InterfaceVirtualMethod { .. }
                    | MemberResolution::InterfaceConcreteMethod { .. }
                    | MemberResolution::External(_)
            )
        );
        if !direct_or_checked {
            self.pending_diags
                .push(PendingDiag::RuntimeTypeArgumentOnIndirectCall { expr: call });
        }
    }

    fn report_mounted_reserved_call(&mut self, call: ExprId, callee: ExprId) {
        let Some(MemberResolution::External(external)) =
            self.result.member_resolutions.get(&callee)
        else {
            return;
        };
        let package = match &external.target {
            crate::callable::ExternalCallTarget::Free { package, .. }
            | crate::callable::ExternalCallTarget::Method { package, .. } => package,
            crate::callable::ExternalCallTarget::Interface { interface, .. } => interface.package(),
        };
        let trusted_callsite_lowering =
            matches!(
                external.builtin_kind,
                Some(
                    baml_compiler2_ast::BuiltinKind::Intrinsic
                        | baml_compiler2_ast::BuiltinKind::AwaitAny
                )
            ) && baml_compiler2_hir::package::is_precompiled_package(self.db, package);
        if external.linkability == crate::callable::ExternalLinkability::ReservedBuiltin
            && !trusted_callsite_lowering
        {
            self.pending_diags
                .push(PendingDiag::MountedPackageCallUnsupported {
                    expr: call,
                    path: external_target_path(&external.target),
                });
        }
    }

    fn validate_runtime_type_arg_operands(&mut self, body: &ExprBody, call: ExprId) {
        let operands: Vec<_> = self
            .type_refs
            .expr_type_args
            .get(&call)
            .into_iter()
            .flat_map(|slots| slots.iter())
            .flat_map(|slot| match slot {
                BodyTypeArgRef::Runtime { operand } => vec![*operand],
                BodyTypeArgRef::Static(type_ref) => {
                    let mut nested = Vec::new();
                    collect_unreflect_type_refs(
                        &self.type_refs.store,
                        self.type_refs.raw_id(*type_ref),
                        &mut nested,
                    );
                    nested.into_iter().map(|(_, operand)| operand).collect()
                }
            })
            .collect();
        for operand in operands {
            self.validate_runtime_type_operand(body, operand);
        }
    }

    fn validate_runtime_type_operand(&mut self, body: &ExprBody, operand: ExprId) {
        if !self.validated_runtime_operands.insert(operand) {
            return;
        }
        let got = self.infer_expr(body, operand, &Expectation::None);
        let pending_type = matches!(got.kind(), InferTy::Class(name, _, _)
            if name.package().as_str() == "reflect"
                && name.namespace().iter().map(baml_type::Name::as_str)
                    .eq(["class"])
                && name.name().as_str() == "PendingType");
        if pending_type
            || got.has_error()
            || got.has_infer()
            || matches!(got.kind(), InferTy::Unknown { .. })
        {
            return;
        }
        // The operand contract is `reflect.Type | reflect.TypeView`: a kind
        // view is accepted and converts to the `type` value it wraps at the
        // VM's type-operand boundary — the same explicit-computation-point
        // model as `int + float`, never a subtyping edge.
        let expected = Ty::intern(InferTy::Union(
            Box::new([
                Ty::intern(InferTy::Type {
                    attr: TyAttr::default(),
                }),
                Ty::intern(InferTy::Interface(
                    baml_type::QualifiedTypeName::from_dotted_path("reflect.TypeView"),
                    Box::new([]),
                    Box::new([]),
                    TyAttr::default(),
                )),
            ]),
            TyAttr::default(),
        ));
        let saved_anchor = self.obligation_anchor.replace(operand);
        let fits = self.sub(&got, &expected);
        self.obligation_anchor = saved_anchor;
        if !fits {
            self.result.type_mismatches.insert(operand, (expected, got));
        }
    }

    fn report_runtime_streaming_call(&mut self, body: &ExprBody, call: ExprId, callee: ExprId) {
        let has_runtime = self
            .type_refs
            .expr_type_args
            .get(&call)
            .is_some_and(|slots| {
                slots
                    .iter()
                    .any(|slot| matches!(slot, BodyTypeArgRef::Runtime { .. }))
            });
        if !has_runtime {
            return;
        }
        let name = match &body.exprs[callee] {
            Expr::Path(segments) => segments.last(),
            Expr::MemberAccess { member, .. } | Expr::OptionalMemberAccess { member, .. } => {
                Some(member)
            }
            _ => None,
        };
        if let Some(name) = name
            && (name.as_str().ends_with("$stream") || name.as_str() == "__make_stream")
        {
            self.pending_diags
                .push(PendingDiag::RuntimeTypeArgumentOnStreamingCall {
                    expr: call,
                    callee: name.clone(),
                });
        }
    }

    /// Seed the otherwise-unconstrained schema parameter of the three legacy
    /// `baml.llm` helper functions from their named, non-generic LLM target.
    fn seed_implicit_llm_schema(
        &mut self,
        body: &ExprBody,
        call: ExprId,
        callee: ExprId,
        args: &[baml_compiler2_ast::CallArg],
    ) {
        let callee_name = match &body.exprs[callee] {
            Expr::Path(segments) => segments.last(),
            Expr::MemberAccess { member, .. } | Expr::OptionalMemberAccess { member, .. } => {
                Some(member)
            }
            _ => None,
        };
        if !callee_name.is_some_and(|name| {
            matches!(
                name.as_str(),
                "render_prompt" | "build_request" | "build_request_stream"
            )
        }) {
            return;
        }
        if self.type_refs.expr_type_args.contains_key(&call) {
            return;
        }
        let Some(MemberResolution::Free { func }) =
            self.result.member_resolutions.get(&callee).cloned()
        else {
            return;
        };
        let package = baml_compiler2_hir::file_package::file_package(self.db, func.file(self.db));
        let data = baml_compiler2_ppir::item_data::function_data(self.db, func);
        if package.package.as_str() != "baml"
            || !package
                .namespace_path
                .iter()
                .map(baml_type::Name::as_str)
                .eq(["llm"])
            || !matches!(
                data.name.as_str(),
                "render_prompt" | "build_request" | "build_request_stream"
            )
        {
            return;
        }
        let signature = function_signature(self.db, func);
        let writable: Vec<_> = signature
            .generic_params
            .iter()
            .filter(|param| !baml_type::is_synthetic_effect_param(param.name()))
            .collect();
        let [schema_param] = writable.as_slice() else {
            return;
        };
        let Some(function_name_arg) = args
            .iter()
            .find(|arg| {
                arg.label
                    .as_ref()
                    .is_some_and(|label| label.as_str() == "function_name")
            })
            .or_else(|| args.get(1))
        else {
            return;
        };
        let Expr::Literal(Literal::String(function_name)) = &body.exprs[function_name_arg.expr]
        else {
            return;
        };
        let path: Vec<baml_type::Name> =
            function_name.split('.').map(baml_type::Name::new).collect();
        let Some(baml_compiler2_hir::contributions::Definition::Function(target)) =
            self.lower.resolve_value(&path)
        else {
            return;
        };
        if baml_compiler2_ppir::item_data::function_llm_meta(self.db, target).is_none()
            || !baml_compiler2_ppir::item_data::function_data(self.db, target)
                .generic_params
                .is_empty()
        {
            return;
        }
        let target_ret = function_signature(self.db, target).ret.clone();
        if target_ret.as_lowering_ty().contains_error()
            || matches!(target_ret, baml_type::Ty::Unknown { .. })
        {
            return;
        }
        let Some(schema_arg) = self
            .result
            .call_plans
            .get(&call)
            .and_then(|plan| plan.type_args.get(schema_param.index() as usize))
            .cloned()
        else {
            return;
        };
        let _ = self
            .table
            .unify(&schema_arg, &crate::impls::interned_ty(&target_ret));
    }

    fn default_uncontracted_session_eval(&mut self, body: &ExprBody, call: ExprId, callee: ExprId) {
        let callee_name = match &body.exprs[callee] {
            Expr::Path(segments) => segments.last(),
            Expr::MemberAccess { member, .. } | Expr::OptionalMemberAccess { member, .. } => {
                Some(member)
            }
            _ => None,
        };
        if callee_name.is_none_or(|name| name.as_str() != "eval") {
            return;
        }
        if self.type_refs.expr_type_args.contains_key(&call) {
            return;
        }
        let Some(MemberResolution::BoundMethod { class, func }) =
            self.result.member_resolutions.get(&callee).cloned()
        else {
            return;
        };
        let qtn = crate::lower::class_qualified_name(self.db, class);
        if qtn.package().as_str() != "reflect"
            || !qtn.namespace().is_empty()
            || qtn.name().as_str() != "Session"
            || baml_compiler2_ppir::item_data::function_data(self.db, func)
                .name
                .as_str()
                != "eval"
        {
            return;
        }
        let signature = function_signature(self.db, func);
        let owner_count = crate::lower::class_generic_frame(self.db, class).len();
        let Some((index, _)) = signature
            .generic_params
            .iter()
            .enumerate()
            .skip(owner_count)
            .find(|(_, param)| !baml_type::is_synthetic_effect_param(param.name()))
        else {
            return;
        };
        let Some(arg) = self
            .result
            .call_plans
            .get(&call)
            .and_then(|plan| plan.type_args.get(index))
            .cloned()
        else {
            return;
        };
        if arg.has_infer() {
            let _ = self.table.unify(
                &arg,
                &Ty::intern(InferTy::Unknown {
                    attr: TyAttr::default(),
                }),
            );
        }
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
        let InferTy::Function {
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
                InferTy::Union(members, _) => {
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
                                    InferTy::Function { .. } | InferTy::Unknown { .. }
                                )
                        }))
                    .then(|| syntactic_union(&clean))
                }
                _ => None,
            };
            // A value typed `unknown` is not callable, and saying so is
            // this diagnostic's whole job - the top type is not a cascade.
            // Only a genuine one (an errored callee, or one still carrying
            // inference vars) stays quiet.
            if !callee_fn_ty.has_error() && !callee_fn_ty.has_infer() {
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
        let params: Vec<baml_type::interned::InferFunctionParamTy> = params
            .iter()
            .skip(usize::from(bound_receiver))
            .cloned()
            .collect();
        let runtime_dependent = self
            .runtime_dependent_call_params
            .remove(&call)
            .unwrap_or_default();
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
                match matched[index] {
                    Some(param_index) if runtime_dependent.contains_key(&param_index) => {
                        let expected = runtime_dependent[&param_index].clone();
                        self.infer_deferred_runtime_expr(body, arg.expr, &expected);
                        self.result
                            .call_plans
                            .entry(call)
                            .or_default()
                            .deferred_checks
                            .push(RuntimeCheck::Argument {
                                arg: arg.expr,
                                expected,
                            });
                    }
                    Some(param_index) => {
                        self.check_expr(body, arg.expr, &params[param_index].ty);
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
                    let local_id = Ty::intern(InferTy::Class(
                        baml_type::QualifiedTypeName::new(
                            baml_type::Name::new(baml_builtins2::PACKAGE_BOUNDARY),
                            vec![],
                            baml_type::Name::new("LocalId"),
                        ),
                        Box::new([]),
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
        // A FULLY-qualified direct call (`(Box as Sized).pick(a, b)`): the
        // call site supplies the turbofish, and `self` stays the written
        // first argument (no bound receiver), the UFCS shape.
        if let Expr::QualifiedPath { member, .. } = &body.exprs[callee] {
            let member = member.clone();
            let fn_ty =
                self.qualified_path_value(callee, &member, OwnArgs::Call(call), Some(callee));
            self.result.type_of_expr.insert(callee, fn_ty.clone());
            return (fn_ty, false);
        }
        // An INTERFACE-qualified direct call
        // (`IqDescribable.describe(t)`): turbofish-aware instantiation
        // with bounds registered - `Self`'s implements-bound becomes an
        // obligation the argument discharges.
        if let Expr::Path(segments) = &body.exprs[callee]
            && segments.len() >= 2
            && (!self.path_resolves_locally(callee)
                || self.qualified_path_root_is_top_level_let(segments))
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
                let (ty, bound) = self.interface_member_callee(interface_member, call, true);
                self.result.type_of_expr.insert(callee, ty.clone());
                return (ty, bound);
            }
        }
        if let Expr::Path(segments) = &body.exprs[callee]
            && (!self.path_resolves_locally(callee)
                || self.qualified_path_root_is_top_level_let(segments))
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
                let bounds = crate::lower::function_generic_bounds(self.db, function);
                let instantiation = self.instantiation_args_with_bounds(
                    call,
                    &signature.generic_params,
                    Some(&callee_name),
                    &bounds,
                    crate::lower::TypePosition::Existential,
                );
                let instantiation = self.write_call_type_args(call, &instantiation, 0);
                self.register_call_bounds(function, &instantiation, call);
                self.record_runtime_dependent_arguments(call, signature, false);
                let fn_ty = function_value_ty(signature, &instantiation);
                self.result.type_of_expr.insert(callee, fn_ty.clone());
                self.write_member_resolution(callee, MemberResolution::Free { func: function });
                return (fn_ty, false);
            }
            if let Some(function) = self.lower.resolve_exported_value(segments) {
                let external = function
                    .external
                    .clone()
                    .expect("mounted free function carries an external descriptor");
                let bounds = external_bounds_map(&external);
                let instantiation = self.instantiation_args_with_bounds(
                    call,
                    &function.generic_params,
                    Some(&function.name),
                    &bounds,
                    external_type_position(&external.target),
                );
                let instantiation = self.write_call_type_args(call, &instantiation, 0);
                self.register_external_call_bounds(&external, &instantiation, call);
                self.record_external_runtime_dependent_arguments(
                    call,
                    &function,
                    false,
                    &instantiation,
                );
                self.result.call_plans.entry(call).or_default().target =
                    Some(external.target.clone());
                let fn_ty = crate::method_resolution::instantiate_external_signature(
                    &function,
                    &instantiation,
                );
                self.result.type_of_expr.insert(callee, fn_ty.clone());
                self.write_member_resolution(callee, MemberResolution::External(external));
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
    ) -> (Ty, bool, Option<MemberResolution<'db, Ty>>, bool) {
        let resolved = self.structurally_resolve(receiver);
        // Callee position on a UNION: the member resolves through the
        // single interface every arm shares (TIR's rule - the union as
        // the intersection existential), `Self` bound to the union. No
        // shared declarer FALLS THROUGH - the operator-style sugars at
        // the bottom of the ladder are TOTAL and apply to the WHOLE
        // union (`(int | null).to_string()` is `string.from<int | null>`);
        // a full miss then reports "no common interface" instead of the
        // bare "no member".
        if let InferTy::Union(union_members, _) = resolved.kind() {
            let union_members = union_members.to_vec();
            match crate::method_resolution::lookup_union_member(
                self.db,
                &self.facts,
                &resolved,
                &union_members,
                member,
            ) {
                crate::method_resolution::UnionMemberLookup::Found(interface_member) => {
                    // A union receiver is ERASED: a `self`-less member has
                    // no value-derived dispatch key, exactly as through an
                    // existential — same rejection as the value road's.
                    if self.reject_selfless_instance_member(&interface_member, member, member_expr)
                    {
                        return (Ty::error(), false, None, false);
                    }
                    let resolution = self.declarer_resolution(&interface_member.declarer, member);
                    let (ty, bound) = self.interface_member_callee(interface_member, call, true);
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
                    if self.reject_selfless_instance_member(&interface_member, member, member_expr)
                    {
                        return (Ty::error(), false, None, false);
                    }
                    let resolution = self.declarer_resolution(&interface_member.declarer, member);
                    let (ty, bound) = self.interface_member_callee(interface_member, call, true);
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
                        Ty::intern(InferTy::Unknown {
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
                let (ty, bound) = self.interface_member_callee(interface_member, call, true);
                return (ty, bound, None, false);
            }
            self.member_probe_depth += 1;
            let (field, field_resolution) = self.field_access_resolved(call, &resolved, member);
            self.member_probe_depth -= 1;
            // A nested inference variable can already have enough evidence
            // to resolve even though the receiver's class head was known
            // (`Probe.new(1)` initially returns `Probe<?T>`). Keep the first
            // pass non-committal so conditional impl probing can constrain the
            // receiver, but after the ordinary lookup tiers miss, force the evidence
            // we have and retry. Otherwise the `has_infer` cascade guard below
            // permanently suppresses E0007 even though finalization later
            // records a fully-ground `Probe<int>` receiver.
            if field.has_error() && resolved.has_infer() {
                let forced = self.force_occurring_vars(&resolved);
                if forced != resolved {
                    return self.member_callee(call, member_expr, &forced, member);
                }
            }
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
            {
                if matches!(resolved.kind(), InferTy::Union(..)) {
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
        match candidate.source {
            crate::method_resolution::MethodCandidateSource::Source { method, class } => {
                if self.reject_selfless_inherent_method(method, member, member_expr) {
                    return (Ty::error(), false, None, false);
                }
                let signature = function_signature(self.db, method);
                let class_count = candidate.class_args.len();
                let own_params = signature.generic_params[class_count..].to_vec();
                let method_name = baml_compiler2_ppir::item_data::function_data(self.db, method)
                    .name
                    .clone();
                let mut instantiation = candidate.class_args;
                let bounds = crate::lower::function_generic_bounds(self.db, method);
                let position = if self.is_reflect_package_get_function(class, method) {
                    crate::lower::TypePosition::ExtractionContract
                } else {
                    crate::lower::TypePosition::Existential
                };
                instantiation.extend(self.instantiation_args_with_bounds(
                    call,
                    &own_params,
                    Some(&method_name),
                    &bounds,
                    position,
                ));
                let instantiation = self.write_call_type_args(call, &instantiation, class_count);
                self.register_call_bounds(method, &instantiation, call);
                let fn_ty = function_value_ty(signature, &instantiation);
                let bound = signature
                    .params
                    .first()
                    .is_some_and(|param| param.name.as_str() == "self");
                self.record_runtime_dependent_arguments(call, signature, bound);
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
            crate::method_resolution::MethodCandidateSource::External(function) => {
                let external = function
                    .external
                    .clone()
                    .expect("mounted method carries an external descriptor");
                let own_offset = candidate.class_args.len();
                let bounds = external_bounds_map(&external);
                let mut instantiation = candidate.class_args;
                instantiation.extend(self.instantiation_args_with_bounds(
                    call,
                    &function.generic_params,
                    Some(&function.name),
                    &bounds,
                    external_type_position(&external.target),
                ));
                let instantiation = self.write_call_type_args(call, &instantiation, own_offset);
                self.register_external_call_bounds(&external, &instantiation, call);
                self.record_external_runtime_dependent_arguments(
                    call,
                    &function,
                    external.takes_self,
                    &instantiation,
                );
                self.result.call_plans.entry(call).or_default().target =
                    Some(external.target.clone());
                (
                    crate::method_resolution::instantiate_external_signature(
                        &function,
                        &instantiation,
                    ),
                    external.takes_self,
                    Some(MemberResolution::External(external)),
                    false,
                )
            }
        }
    }

    /// The `default` receiver's meaning inside an `implements` block:
    /// the block's target interface (its written args and associated
    /// bindings lowered in the owner's frame) plus the IMPLEMENTOR as
    /// `Self`, off the uniform [`impl_self_ty`](crate::lower::impl_self_ty)
    /// surface. `None` anywhere else; the caller falls back to ordinary
    /// resolution.
    fn default_receiver_target(&mut self) -> Option<(InferInterface, Ty)> {
        let function = self.body_owner?;
        let target =
            baml_compiler2_ppir::item_data::method_interface_target(self.db, function).as_ref()?;
        let target_ty = self.lower.lower_type_ref_at(
            &target.type_refs,
            target.target,
            crate::lower::TypePosition::ConstraintHead,
        );
        let target_interned = crate::impls::interned_ty(&crate::lower::reject_holes(&target_ty));
        let InferTy::Interface(name, args, pins, _) = target_interned.kind() else {
            return None;
        };
        let self_ty = match baml_compiler2_ppir::item_data::method_owner(self.db, function) {
            Some(baml_compiler2_ppir::item_data::MethodOwner::Impl(impl_loc)) => {
                crate::impls::interned_ty(&crate::lower::impl_self_ty(self.db, impl_loc))
            }
            // A recorded interface target pairs with an Impl owner —
            // class-owned methods never carry one, and interface default
            // bodies have no target.
            _ => return None,
        };
        Some((
            InferInterface::new(name.clone(), args.clone(), pins.clone()),
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
        bind_self: bool,
    ) -> (Ty, bool) {
        // A `self`-LESS method binds no receiver whatever the spelling: the
        // receiver named where to find the member, it is not an argument.
        let bound = interface_member.is_method
            && bind_self
            && self.interface_member_takes_self(&interface_member);
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
                    let bounds = crate::lower::function_generic_bounds(self.db, method);
                    instantiation.extend(self.instantiation_args_with_bounds(
                        call,
                        &own_params,
                        Some(&method_name),
                        &bounds,
                        crate::lower::TypePosition::Existential,
                    ));
                    let instantiation = self.write_call_type_args(call, &instantiation, own_offset);
                    self.register_call_bounds(method, &instantiation, call);
                    self.record_runtime_dependent_arguments(call, signature, bound);
                    return (function_value_ty(signature, &instantiation), bound);
                }
                crate::method_resolution::PendingOwnGenerics::External { function, prefix } => {
                    let external = function
                        .external
                        .clone()
                        .expect("mounted interface method has an external descriptor");
                    let own_offset = prefix.len();
                    let bounds = external_bounds_map(&external);
                    let mut instantiation = prefix;
                    instantiation.extend(self.instantiation_args_with_bounds(
                        call,
                        &function.generic_params,
                        Some(&function.name),
                        &bounds,
                        crate::lower::TypePosition::Existential,
                    ));
                    let instantiation = self.write_call_type_args(call, &instantiation, own_offset);
                    self.register_external_call_bounds(&external, &instantiation, call);
                    self.record_external_runtime_dependent_arguments(
                        call,
                        &function,
                        bound,
                        &instantiation,
                    );
                    self.result.call_plans.entry(call).or_default().target =
                        Some(external.target.clone());
                    return (
                        crate::method_resolution::instantiate_external_signature(
                            &function,
                            &instantiation,
                        ),
                        bound,
                    );
                }
            }
        }
        if let crate::method_resolution::MemberDeclarer::ExternalMethod(external) =
            &interface_member.declarer
        {
            self.result.call_plans.entry(call).or_default().target = Some(external.target.clone());
        }
        (interface_member.ty, bound)
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
        if let Some(baml_compiler2_hir::contributions::Definition::Function(function)) =
            self.lower.resolve_value(&segments)
        {
            let signature = function_signature(self.db, function);
            // The desugar targets are single-`<T>`-generic by contract;
            // anything else is stdlib drift this tier must not paper over.
            if signature.generic_params.len() != 1 {
                return None;
            }
            return Some(function_value_ty(signature, &[target]));
        }
        let function = self.lower.resolve_exported_value(&segments)?;
        (function.generic_params.len() == 1)
            .then(|| crate::method_resolution::instantiate_external_signature(&function, &[target]))
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
        if let Some(baml_compiler2_hir::contributions::Definition::Function(function)) =
            self.lower.resolve_value(&segments)
        {
            let signature = function_signature(self.db, function);
            let [param] = signature.params.as_slice() else {
                return None;
            };
            return Some((
                crate::impls::interned_ty(&param.ty),
                crate::impls::interned_ty(&signature.throws),
            ));
        }
        let function = self.lower.resolve_exported_value(&segments)?;
        let [param] = function.params.as_slice() else {
            return None;
        };
        Some((
            Ty::from_plain(&param.ty),
            Ty::from_plain(&function.callable_throws),
        ))
    }

    /// The instantiated `string.from` callee backing the `to_string`
    /// operator-style fallback - a class STATIC (`baml.String.from<T>`),
    /// resolved through the same static-class correspondence written
    /// `string.from(..)` calls use, its `T` pinned to the receiver.
    fn string_from_callee(&mut self, target: Ty) -> Option<Ty> {
        if let Some((class, _)) =
            self.static_class_for(std::slice::from_ref(&baml_type::Name::new("string")))
        {
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
            return Some(function_value_ty(signature, &[target]));
        }
        let qtn = baml_type::TypeName::new(
            baml_type::Name::new("baml"),
            Vec::new(),
            baml_type::Name::new("String"),
        );
        let crate::package_interface::ExportedType::Class { methods, .. } =
            crate::package_interface::mounted_type_row(self.db, &qtn)?
        else {
            return None;
        };
        let method = methods
            .iter()
            .find(|method| method.name.as_str() == "from")?;
        let function =
            crate::package_interface::resolved_exported_function(method, Vec::new(), Vec::new());
        (function.generic_params.len() == 1)
            .then(|| crate::method_resolution::instantiate_external_signature(&function, &[target]))
    }

    /// Generic methods follow the same realization rule as free functions:
    /// a bare value needs either explicit type arguments or enough contextual
    /// function type information to determine them. Direct method calls do not
    /// reach this path (their call site owns inference), and `GenericApply`
    /// already carries an explicit specialization.
    fn report_unspecialized_generic_method_value(
        &mut self,
        body: &ExprBody,
        expr: ExprId,
        expected: &Expectation,
        inferred: &Ty,
    ) {
        if matches!(body.exprs[expr], Expr::GenericApply { .. }) {
            return;
        }
        let Some(resolution) = self.result.member_resolutions.get(&expr).cloned() else {
            return;
        };
        let reference = body.display_expr(expr);
        let had_context = expected.only_has_type().is_some() || self.optional_call_callee_depth > 0;
        let source_method = match resolution {
            MemberResolution::BoundMethod { func, .. }
            | MemberResolution::InterfaceConcreteMethod { func, .. } => Some((func, true)),
            MemberResolution::UnboundMethod { func, .. } => Some((func, false)),
            MemberResolution::InterfaceVirtualMethod { interface, method } => {
                baml_compiler2_ppir::item_data::interface_data(self.db, interface)
                    .methods
                    .iter()
                    .copied()
                    .find(|func| {
                        baml_compiler2_ppir::item_data::function_data(self.db, *func).name == method
                    })
                    .map(|func| (func, true))
            }
            MemberResolution::External(external)
                if !matches!(
                    external.target,
                    crate::callable::ExternalCallTarget::Free { .. }
                ) =>
            {
                if external.user_generic_params().next().is_some() {
                    let generic_params: Vec<_> = external
                        .user_generic_params()
                        .map(|(param, _)| param.name().clone())
                        .collect();
                    let specialization_example_is_safe = external
                        .user_generic_params()
                        .all(|(_, bounds)| bounds.is_empty());
                    let specialization_syntax_available = matches!(body.exprs[expr], Expr::Path(_));
                    self.pending_diags
                        .push(PendingDiag::GenericFunctionValueNotSpecialized {
                            expr,
                            name: external.display_name().clone(),
                            reference,
                            inference_evidence: vec![inferred.clone()],
                            specialization_args: None,
                            unconditional: !had_context,
                            had_expected_type: had_context,
                            generic_params,
                            binding_name: initializer_binding_name(body, expr),
                            function_shape: None,
                            annotation_ty: None,
                            specialization_example_is_safe,
                            specialization_syntax_available,
                        });
                }
                return;
            }
            _ => None,
        };
        let Some((method, receiver_is_bound)) = source_method else {
            return;
        };
        let data = baml_compiler2_ppir::item_data::function_data(self.db, method);
        if data.generic_params.is_empty() {
            return;
        }
        let signature = function_signature(self.db, method);
        let user_params = function_user_generic_params(self.db, method, signature);
        let generic_params = user_params
            .iter()
            .map(|param| param.name().clone())
            .collect();
        let specialization_example_is_safe = data
            .generic_params
            .iter()
            .all(|param| param.bounds.is_empty());
        let specialization_syntax_available = matches!(body.exprs[expr], Expr::Path(_));
        let has_phantom_param = user_params
            .iter()
            .any(|param| !function_signature_mentions_param(signature, param));
        self.pending_diags
            .push(PendingDiag::GenericFunctionValueNotSpecialized {
                expr,
                name: data.name.clone(),
                reference,
                inference_evidence: vec![inferred.clone()],
                specialization_args: None,
                unconditional: !had_context || has_phantom_param,
                had_expected_type: had_context,
                generic_params,
                binding_name: initializer_binding_name(body, expr),
                function_shape: (!has_phantom_param).then(|| {
                    generic_function_value_shape(signature, user_params, receiver_is_bound, false)
                }),
                annotation_ty: (specialization_example_is_safe && !has_phantom_param)
                    .then(|| inferred.clone()),
                specialization_example_is_safe,
                specialization_syntax_available,
            });
    }

    /// The one home for value-position path typing (rust-analyzer's
    /// `infer/path.rs` shape): a local/parameter root followed by field
    /// accesses, or a package-level FUNCTION as a first-class value (`let c:
    /// (x: int) -> int throws never = inc;`), instantiated with fresh
    /// variables per generic param. A contextual function type may resolve
    /// those variables; without one, a generic function must be explicitly
    /// specialized before it can become a value.
    /// Constants and enum variants join as later slices land.
    fn resolve_value_path(
        &mut self,
        body: &ExprBody,
        expr: ExprId,
        segments: &[baml_type::Name],
        expected: &Expectation,
    ) -> Ty {
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
        let top_level_let_qualified_item = self.qualified_path_root_is_top_level_let(segments)
            && (matches!(
                self.lower.resolve_value(segments),
                Some(Definition::Function(_))
            ) || self.lower.resolve_exported_value(segments).is_some());
        if self.path_resolves_locally(expr) && !top_level_let_qualified_item {
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
            let had_context =
                expected.only_has_type().is_some() || self.optional_call_callee_depth > 0;
            let signature = function_signature(self.db, function);
            let instantiation: Vec<Ty> = signature
                .generic_params
                .iter()
                .map(|param| self.fresh_generic_arg(param))
                .collect();
            let data = baml_compiler2_ppir::item_data::function_data(self.db, function);
            if data.generic_params.is_empty() {
                // Synthetic callback-effect parameters are inference-only;
                // they do not make an otherwise non-generic function value
                // require explicit specialization.
            } else {
                let user_params = function_user_generic_params(self.db, function, signature);
                let generic_params = user_params
                    .iter()
                    .map(|param| param.name().clone())
                    .collect();
                let specialization_example_is_safe = data
                    .generic_params
                    .iter()
                    .all(|param| param.bounds.is_empty());
                let has_phantom_param = user_params
                    .iter()
                    .any(|param| !function_signature_mentions_param(signature, param));
                let inference_evidence = user_params
                    .iter()
                    .filter_map(|param| instantiation.get(param.index() as usize).cloned())
                    .collect();
                self.pending_diags
                    .push(PendingDiag::GenericFunctionValueNotSpecialized {
                        expr,
                        name: data.name.clone(),
                        reference: segments
                            .iter()
                            .map(baml_type::Name::as_str)
                            .collect::<Vec<_>>()
                            .join("."),
                        inference_evidence,
                        specialization_args: Some(
                            user_params
                                .iter()
                                .filter_map(|param| {
                                    instantiation.get(param.index() as usize).cloned()
                                })
                                .collect(),
                        ),
                        unconditional: !had_context,
                        had_expected_type: had_context,
                        generic_params,
                        binding_name: initializer_binding_name(body, expr),
                        function_shape: (!has_phantom_param).then(|| {
                            generic_function_value_shape(signature, user_params, false, false)
                        }),
                        annotation_ty: (specialization_example_is_safe && !has_phantom_param)
                            .then(|| function_value_ty(signature, &instantiation)),
                        specialization_example_is_safe,
                        specialization_syntax_available: true,
                    });
            }
            self.write_member_resolution(expr, MemberResolution::Free { func: function });
            return function_value_ty(signature, &instantiation);
        }
        if let Some(function) = self.lower.resolve_exported_value(segments) {
            let had_context =
                expected.only_has_type().is_some() || self.optional_call_callee_depth > 0;
            let external = function
                .external
                .clone()
                .expect("mounted free function carries an external descriptor");
            let instantiation: Vec<Ty> = function
                .generic_params
                .iter()
                .map(|param| self.fresh_generic_arg(param))
                .collect();
            if external.user_generic_params().next().is_some() {
                let user_params: Vec<_> = external
                    .user_generic_params()
                    .map(|(param, _)| param.clone())
                    .collect();
                let generic_params = user_params
                    .iter()
                    .map(|param| param.name().clone())
                    .collect();
                let specialization_example_is_safe = external
                    .user_generic_params()
                    .all(|(_, bounds)| bounds.is_empty());
                let shape_ty =
                    external_generic_function_value_ty(&function, &user_params, false, false);
                let has_phantom_param = user_params
                    .iter()
                    .any(|param| !ty_mentions_param(&shape_ty, param));
                let inference_evidence = external
                    .user_generic_params()
                    .filter_map(|(param, _)| instantiation.get(param.index() as usize).cloned())
                    .collect();
                self.pending_diags
                    .push(PendingDiag::GenericFunctionValueNotSpecialized {
                        expr,
                        name: function.name.clone(),
                        reference: segments
                            .iter()
                            .map(baml_type::Name::as_str)
                            .collect::<Vec<_>>()
                            .join("."),
                        inference_evidence,
                        specialization_args: Some(
                            user_params
                                .iter()
                                .filter_map(|param| {
                                    instantiation.get(param.index() as usize).cloned()
                                })
                                .collect(),
                        ),
                        unconditional: !had_context,
                        had_expected_type: had_context,
                        generic_params,
                        binding_name: initializer_binding_name(body, expr),
                        function_shape: (!has_phantom_param)
                            .then(|| rendered_plain(&shape_ty).to_string()),
                        annotation_ty: (specialization_example_is_safe && !has_phantom_param).then(
                            || {
                                crate::method_resolution::instantiate_external_signature(
                                    &function,
                                    &instantiation,
                                )
                            },
                        ),
                        specialization_example_is_safe,
                        specialization_syntax_available: true,
                    });
            }
            self.write_member_resolution(expr, MemberResolution::External(external));
            return crate::method_resolution::instantiate_external_signature(
                &function,
                &instantiation,
            );
        }
        // Session submissions persist root bindings as top-level `let`s. A
        // let has no declaration signature, so recover its value type from the
        // initializer's durable inference result and feed that through the same
        // member walk used for lexical locals. Function and exported-value
        // resolution deliberately run first so a root binding named `json`,
        // `reflect`, or `type` cannot shadow those package paths. A single
        // segment still reaches this value tier.
        if let Some(root) = segments.first()
            && let Some(Definition::Let(let_binding)) =
                self.lower.resolve_value(std::slice::from_ref(root))
        {
            if self.body_owner_id == Some(BodyOwnerId::Let(let_binding))
                || let_owner_is_in_flight(self.db, let_binding)
            {
                if self.member_probe_depth == 0 {
                    self.pending_diags
                        .push(PendingDiag::TopLevelLetCycle { expr });
                }
                return Ty::error();
            }
            let inference = infer_body(self.db, BodyOwnerId::Let(let_binding));
            let body = baml_compiler2_hir::body::let_body(self.db, let_binding);
            if let baml_compiler2_hir::body::LetBody::Expr(body) = body.as_ref()
                && let Some(root) = body.root_expr
                && let Some(root_ty) = inference.type_of_expr.get(&root).cloned()
            {
                // The initializer's own type is the EXPRESSION type, so a
                // fresh literal arrives unwidened - `let n = 5` would bind `5`
                // and `n.to_string()` would be E0007 on a type with no
                // members. A top-level let is a binding site like any other
                // (`let` in a body applies this before recording the
                // binding), and a session cannot annotate one to opt out, so
                // the widening is unconditional here.
                let root_ty = self.widen_fresh(&Ty::from_plain(&root_ty));
                let (ty, steps) = self.walk_path_members(expr, root_ty, &segments[1..]);
                self.write_resolved_path(expr, steps);
                return ty;
            }
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
            && self.lower.resolve_exported_value(segments).is_none()
        {
            // When a proper prefix resolves (`baml.media.Image.missing`
            // has the valid type `baml.media.Image`), the segment AFTER
            // the longest valid prefix is what failed - report it alone,
            // not the whole dotted path (TIR's first-invalid-segment
            // rule). A path with no valid prefix reports in full.
            let failed = (1..segments.len()).rev().find_map(|cut| {
                let prefix = &segments[..cut];
                (self.lower.resolve_type_definition(prefix).is_some()
                    || self
                        .lower
                        .resolve_exported_type_definition(prefix)
                        .is_some()
                    || self.lower.resolve_value(prefix).is_some()
                    || self.lower.resolve_exported_value(prefix).is_some())
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
            if let Some(shorthand_name) = self.property_shorthand_values.get(&expr).cloned() {
                let locals = self.local_binding_names_at(expr);
                let suggestions =
                    crate::diagnostics::similar_name_suggestions(&shorthand_name, locals.iter());
                self.pending_diags.push(PendingDiag::UnresolvedShorthand {
                    expr,
                    name: shorthand_name,
                    suggestions,
                });
            } else {
                self.pending_diags
                    .push(PendingDiag::UnresolvedName { expr, name });
            }
        }
        Ty::error()
    }

    fn own_instantiation_with_bounds(
        &mut self,
        own: OwnArgs,
        params: &[baml_type::ParamTy],
        callee: &baml_type::Name,
        bounds: &FxHashMap<baml_type::ParamTy, Vec<baml_type::Interface>>,
        position: crate::lower::TypePosition,
    ) -> Vec<Ty> {
        match own {
            OwnArgs::Call(call) => {
                self.instantiation_args_with_bounds(call, params, Some(callee), bounds, position)
            }
            OwnArgs::Fresh => params
                .iter()
                .map(|param| self.fresh_generic_arg(param))
                .collect(),
        }
    }

    /// TIER: an ITEM PROJECTION - the `(Self type, interface, item)` triple,
    /// with either half optionally left to inference. The spellings differ
    /// only in which halves are written:
    ///
    /// | spelling            | `qself`  | `qualifier` |
    /// |---------------------|----------|-------------|
    /// | `Interface.item`    | inferred | written     |
    /// | `(Base as I).item`  | written  | written     |
    ///
    /// `Self` is slot 0 of an interface method's generic frame
    /// (`lower::interface_frame`). A written `qself` PINS it; an inferred one
    /// leaves a fresh variable whose implements-bound rides along through
    /// `function_generic_bounds` and is discharged by the call's arguments.
    /// That one slot is the entire difference between the spellings - which
    /// is why they are one tier and not two.
    ///
    /// A turbofish binds the slots AFTER `Self`: the method's own generics
    /// and the interface's, never `Self` itself, which has its own written
    /// position in the qualified spelling and is inferred in the other.
    ///
    /// The receiver is never bound: `self` stays the written first argument,
    /// the UFCS shape `Type.method(recv, ..)` already uses.
    fn item_projection_value(
        &mut self,
        interface: baml_compiler2_hir::loc::InterfaceLoc<'db>,
        written: Option<&WrittenQualifier<'_>>,
        member: &baml_type::Name,
        own: OwnArgs,
        anchor: ExprId,
        record_at: Option<ExprId>,
    ) -> Option<Ty> {
        let qself = written.map(|written| &written.qself);
        let realized = written.map(|written| written.realized);
        let method = self.interface_method_loc(interface, member)?;
        let signature = function_signature(self.db, method);
        let bounds = crate::lower::function_generic_bounds(self.db, method);
        let Some((self_param, after_self)) = signature.generic_params.split_first() else {
            unreachable!("an interface method's generic frame always opens with `Self`")
        };
        debug_assert_eq!(
            self_param.name().as_str(),
            "Self",
            "interface method frame must open with the `Self` slot"
        );

        // The frame is `[Self] ++ interface generics ++ the method's own
        // generics` (`lower::interface_frame`). A written qualifier realizes
        // the generics group and it is PINNED from it: `Conv<int>` and
        // `Conv<string>` are different interfaces a type may implement both
        // of, so leaving their slots to inference would let two calls in one
        // body unify against the same hole. Associated types are not slots -
        // signature references to them are projections over `Self`, reduced
        // once `Self` is known.
        let interface_data = baml_compiler2_ppir::item_data::interface_data(self.db, interface);
        let pinned = interface_data.generic_params.len();
        // `lower::function_generic_frame` builds an interface method's frame
        // from this same `interface_data`, appending the method's own generics
        // after the two groups, so the frame is always at least this long.
        debug_assert!(
            pinned <= after_self.len(),
            "interface method frame is shorter than the interface it was built from"
        );
        // The clamp is an explicitly-marked release-mode net for that
        // invariant, never a resolution strategy: `debug_assert` above fails
        // the tests if it is ever load-bearing.
        let (frame_params, own_params) = after_self.split_at(pinned.min(after_self.len()));

        let mut instantiation = Vec::with_capacity(signature.generic_params.len());
        let self_slot = match qself {
            Some(written) => written.clone(),
            None => self.fresh_generic_arg(self_param),
        };
        // Guards on the `Self` slot, deferred until inference resolves it
        // (the inferred spelling only pins `Self` through the arguments):
        // an ERASED `Self` — an interface-existential or a union — obeys the
        // one-`Self` object-safety rule when the method takes a receiver
        // (dispatch derives the concrete type from the value), and is
        // rejected outright when it does not (type-keyed dispatch has no
        // value to consult, and an erased type names no single impl).
        // An UNRESOLVED `Self` is a hard error: rustc's E0790, whose fix is
        // the fully-qualified spelling.
        let takes_self = signature
            .params
            .first()
            .is_some_and(|param| param.name.as_str() == "self");
        let iface_ref = InferInterface::new(
            crate::interfaces::interface_loc_qtn(self.db, interface)
                .unwrap_or_else(|| unreachable!("a resolved interface item has a source QTN")),
            // The WRITTEN arguments: `Conv<int>` and `Conv<string>` are
            // different interfaces, so a message naming a bare `Conv` would
            // not say which one the reference meant. Empty for the inferred
            // spelling, which wrote none. Associated types stay empty — they
            // are determined rather than written, and naming them would
            // over-specify the message.
            realized.map_or_else(Box::default, |realized| {
                realized.generics.iter().map(Ty::from_plain).collect()
            }),
            Box::new([]),
        );
        self.pending_diags
            .push(PendingDiag::ItemProjectionSelfSlot {
                expr: anchor,
                var: self_slot.clone(),
                interface: iface_ref,
                member: member.clone(),
                takes_self,
                value_position: matches!(own, OwnArgs::Fresh),
            });
        if qself.is_none() {
            self.pending_diags.push(PendingDiag::UninferredCtorParam {
                expr: anchor,
                var: self_slot.clone(),
                name: baml_type::Name::new("Self"),
            });
        }
        instantiation.push(self_slot);
        for (index, param) in frame_params.iter().enumerate() {
            // Realized generics first, then realized associated types by name;
            // an unrealized slot (the `Interface.item` spelling, which has no
            // subject to realize against) stays a fresh variable the call's
            // arguments solve.
            // The pinned group IS the declared generics — associated types
            // are not frame slots, so the index maps 1:1; a realization that
            // comes up short leaves the slot fresh.
            let realized_arg =
                realized.and_then(|realized| realized.generics.get(index).map(Ty::from_plain));
            instantiation.push(match realized_arg {
                Some(arg) => arg,
                None => {
                    let fresh = self.fresh_generic_arg(param);
                    // Same discipline as the `Self` slot: a frame slot the
                    // call leaves unsolved is a hard error, never an Error
                    // type reaching emission.
                    self.pending_diags.push(PendingDiag::UninferredCtorParam {
                        expr: anchor,
                        var: fresh.clone(),
                        name: param.name().clone(),
                    });
                    fresh
                }
            });
        }
        let own_args = self.own_instantiation_with_bounds(
            own,
            own_params,
            member,
            &bounds,
            crate::lower::TypePosition::Existential,
        );
        if matches!(own, OwnArgs::Fresh) {
            // Same discipline as the `Self` and pinned slots: a fresh own
            // generic nothing solves is a hard error, never an Error type
            // reaching emission. This lane is reachable from a CALL too —
            // a consumed type-arg channel (`Bin<int>.pick()`, and equally
            // the turbofish spelling `Bin.pick<int>()`, which the channel
            // cannot distinguish from it) hands the written args to the
            // CLASS frame and leaves the member's own generics with no
            // written source at all.
            for (param, arg) in own_params.iter().zip(&own_args) {
                // Synthetic effect params are elaboration's, never spelled,
                // and legitimately default when unconstrained.
                if baml_type::is_synthetic_effect_param(param.name()) {
                    continue;
                }
                self.pending_diags.push(PendingDiag::UninferredCtorParam {
                    expr: anchor,
                    var: arg.clone(),
                    name: param.name().clone(),
                });
            }
        }
        instantiation.extend(own_args);
        match own {
            OwnArgs::Call(call) => {
                instantiation = self.write_call_type_args(call, &instantiation, 0);
                self.record_runtime_dependent_arguments(call, signature, false);
            }
            OwnArgs::Fresh => {
                // A VALUE reference has no call site, but MIR still needs the
                // (eventually solved) frame to resolve the callable — record
                // it as a plan keyed by the reference expression itself, the
                // same channel a call's instantiation rides. Writeback
                // grounds it once inference finishes.
                let plan = self.result.call_plans.entry(anchor).or_default();
                plan.type_args.clone_from(&instantiation);
                plan.own_offset = 0;
            }
        }
        self.register_call_bounds(method, &instantiation, anchor);
        if let Some(record_at) = record_at {
            // The slot is what is statically known - interface plus member -
            // recorded uniformly for every spelling and for default and
            // required methods alike. Resolving it to a concrete impl is
            // downstream's job: the VM keys on the receiver's runtime type
            // and caches the target. A written `Self` narrows WHICH interface
            // slot, never who answers it.
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

    /// The `(Base as I).item` spelling: both halves written. Validates them
    /// and hands the same tier its pinned `Self`.
    ///
    /// This is the only spelling that reaches two otherwise-unreachable
    /// members - one declared by several implemented interfaces (the bare
    /// `base.item` is E0121-ambiguous), and one whose `Self` appears in a
    /// parameter (the existential `base.as<I>.item` is object-safety
    /// rejected, and a written `Self` removes the need for a receiver).
    fn qualified_path_value(
        &mut self,
        expr: ExprId,
        member: &baml_type::Name,
        own: OwnArgs,
        record_at: Option<ExprId>,
    ) -> Ty {
        let Some(anchors) = self.type_refs.qualified_path_anchors.get(&expr).copied() else {
            // The parser builds this node only with both halves present, so a
            // missing anchor is a malformed body, already reported upstream.
            return Ty::error();
        };
        let member = member.clone();

        let lower = |this: &mut Self, type_ref, position| {
            let (ty, diagnostics) = this.lower_body_type_ref_at(type_ref, position);
            this.queue_body_lowering_diagnostics(diagnostics);
            this.reject_expr_position_holes(&ty, expr)
        };
        let qself = lower(self, anchors.qself, crate::lower::TypePosition::Existential);
        // The qualifier is a CONSTRAINT HEAD, not an existential: its
        // associated types are determined by `Self` and need not be written,
        // exactly as in a bound (`T extends I`). The existential position's
        // pin-them-all demand would reject `(int as I)` for any interface
        // with an associated type.
        let interface = lower(
            self,
            anchors.interface,
            crate::lower::TypePosition::ConstraintHead,
        );
        if qself.has_error() || interface.has_error() {
            return Ty::error();
        }
        // The interface-view gate is a STRUCTURE demand, as it is for
        // `.as<I>`: an alias naming an interface answers as the interface.
        let interface = self.expand_alias_ty(&interface);
        let Some(qualifier) = InferInterface::of_ty(&interface) else {
            self.pending_diags.push(PendingDiag::QualifierNotInterface {
                expr,
                target: interface,
            });
            return Ty::error();
        };
        let qself = self.structurally_resolve(&qself);
        // Determination runs the impl matcher and the canonical algebra,
        // neither of which admits an unresolved inference variable. Both
        // halves are WRITTEN types, ground at lowering — the only way a
        // variable survives to here is a written hole (`(Wrapper<_> as I).m`)
        // that `reject_expr_position_holes` already reported and replaced with
        // a fresh var. So this bail never introduces a silent `Ty::error()`:
        // the diagnostic invariant is upheld by construction, and the assert
        // pins that reading.
        if qself.has_infer() || interface.has_infer() {
            debug_assert!(
                self.pending_diags.iter().any(
                    |diag| matches!(diag, PendingDiag::ExprPositionHole { expr: at } if *at == expr)
                ),
                "an inference variable in a written qualifier must come from a reported hole"
            );
            return Ty::error();
        }

        // Which half is at fault, from the SHARED determination road: the
        // qualifier must be an interface `qself` implements, and it must
        // declare `member` DIRECTLY (`requires` is a bound, not inheritance).
        // Written generic arguments are load-bearing here - `Bar<int>` and
        // `Bar<string>` are different interfaces and a type may implement
        // both - while associated types need not be written, being uniquely
        // determined once `Self` is known.
        let qself_plain = self.materialize_ty(&qself);
        let interface_plain = self.materialize_ty(&interface);
        let (determination, diagnostics) = crate::interfaces::determine_member_interface_with_facts(
            self.db,
            &self.facts,
            &qself_plain,
            Some(interface_plain),
            &member,
            crate::interfaces::MemberNamespace::Value,
        );
        for error in diagnostics {
            self.pending_diags.push(PendingDiag::AnnotWf {
                type_ref: anchors.interface,
                error,
            });
        }
        let unresolved_member = |this: &mut Self| {
            if this.member_probe_depth == 0 {
                this.pending_diags.push(PendingDiag::UnresolvedMember {
                    expr,
                    base: interface.clone(),
                    member: member.clone(),
                });
            }
            Ty::error()
        };
        let realized = match determination {
            crate::interfaces::Determination::Determined(realized) => realized,
            crate::interfaces::Determination::SubjectDoesNotImplementQualifier { .. } => {
                if self.member_probe_depth == 0 {
                    self.pending_diags
                        .push(PendingDiag::QualifierNotImplemented {
                            expr,
                            value: qself,
                            interface,
                        });
                }
                return Ty::error();
            }
            crate::interfaces::Determination::Undeclared { .. }
            | crate::interfaces::Determination::Ambiguous(_) => return unresolved_member(self),
            // Already-poisoned inputs were reported where they were lowered.
            crate::interfaces::Determination::InvalidBase
            | crate::interfaces::Determination::Poisoned => return Ty::error(),
        };

        // A MOUNTED interface has no source method item to instantiate, the
        // same limit the `Interface.item` spelling has; and determination
        // proved the member exists in the VALUE namespace, so a miss below it
        // is a FIELD, which has no static spelling - reading one needs a
        // receiver to read it from.
        let Some(interface_loc) = self.interface_loc_for(&qualifier.name) else {
            return unresolved_member(self);
        };
        self.item_projection_value(
            interface_loc,
            Some(&WrittenQualifier {
                qself,
                realized: &realized,
            }),
            &member,
            own,
            expr,
            record_at,
        )
        .unwrap_or_else(|| unresolved_member(self))
    }

    /// The source `InterfaceLoc` a qualified type name denotes, if any.
    fn interface_loc_for(
        &self,
        qtn: &baml_type::QualifiedTypeName,
    ) -> Option<baml_compiler2_hir::loc::InterfaceLoc<'db>> {
        match self.facts.definition_of(qtn) {
            Some(baml_compiler2_hir::contributions::Definition::Interface(loc)) => Some(loc),
            _ => None,
        }
    }

    /// The interface's method item named `member`. Required and default
    /// methods alike - they are one item kind.
    fn interface_method_loc(
        &self,
        interface: baml_compiler2_hir::loc::InterfaceLoc<'db>,
        member: &baml_type::Name,
    ) -> Option<baml_compiler2_hir::loc::FunctionLoc<'db>> {
        baml_compiler2_ppir::item_data::interface_data(self.db, interface)
            .methods
            .iter()
            .copied()
            .find(|&method| {
                baml_compiler2_ppir::item_data::function_data(self.db, method).name == *member
            })
    }

    /// The `Interface.item` spelling: the interface is written, `Self` is
    /// inferred from the call's arguments. Bounds register at the
    /// instantiation, whatever the spelling.
    fn interface_static_value(
        &mut self,
        prefix: &[baml_type::Name],
        member: &baml_type::Name,
        own: OwnArgs,
        anchor: ExprId,
        record_at: Option<ExprId>,
    ) -> Option<Ty> {
        let (interface, _) = self.interface_static_method(prefix, member)?;
        self.item_projection_value(interface, None, member, own, anchor, record_at)
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
        // Preserve the ordinary source-backed fast path. Runtime compilation
        // has no stdlib source classes, so only it falls through to the
        // serialized external surface below.
        if let Some(source) = self.source_class_static_value(prefix, member, own, anchor, record_at)
        {
            return Some(source);
        }
        let exported_class = self
            .lower
            .resolve_exported_type_definition(prefix)
            .map(|exported| (exported, None))
            .or_else(|| {
                self.static_external_class_for(prefix)
                    .map(|(exported, args)| (exported, Some(args)))
            });
        if let Some((exported, pinned)) = exported_class
            && let crate::package_interface::ExportedType::Class {
                methods,
                generic_params,
                generic_param_bounds,
                ..
            } = exported.as_ref()
            && let Some(method) = methods.iter().find(|method| &method.name == member)
        {
            let function = crate::package_interface::resolved_exported_function(
                method,
                generic_params.clone(),
                generic_param_bounds.clone(),
            );
            let external = function
                .external
                .clone()
                .expect("mounted static method carries an external descriptor");
            let bounds = external_bounds_map(&external);
            let frame: Vec<baml_type::ParamTy> = external
                .owner_generic_params
                .iter()
                .chain(&external.generic_params)
                .cloned()
                .collect();
            let (instantiation, own_offset) = match (own, pinned) {
                (OwnArgs::Call(call), Some(owner_args)) => {
                    let own_args = self.instantiation_args_with_bounds(
                        call,
                        &external.generic_params,
                        Some(member),
                        &bounds,
                        external_type_position(&external.target),
                    );
                    let own_offset = owner_args.len();
                    let mut instantiation = owner_args;
                    instantiation.extend(own_args);
                    (instantiation, own_offset)
                }
                (OwnArgs::Call(call), None) => {
                    let instantiation = self.instantiation_args_with_bounds(
                        call,
                        &frame,
                        Some(member),
                        &bounds,
                        external_type_position(&external.target),
                    );
                    (instantiation, 0)
                }
                (OwnArgs::Fresh, Some(mut owner_args)) => {
                    owner_args.extend(
                        external
                            .generic_params
                            .iter()
                            .map(|param| self.fresh_generic_arg(param)),
                    );
                    let offset = owner_args.len() - external.generic_params.len();
                    (owner_args, offset)
                }
                (OwnArgs::Fresh, None) => (
                    frame
                        .iter()
                        .map(|param| self.fresh_generic_arg(param))
                        .collect(),
                    0,
                ),
            };
            let instantiation = if let OwnArgs::Call(call) = own {
                let instantiation = self.write_call_type_args(call, &instantiation, own_offset);
                self.register_external_call_bounds(&external, &instantiation, anchor);
                self.record_external_runtime_dependent_arguments(
                    call,
                    &function,
                    false,
                    &instantiation,
                );
                self.result.call_plans.entry(call).or_default().target =
                    Some(external.target.clone());
                instantiation
            } else {
                instantiation
            };
            if let Some(record_at) = record_at {
                self.write_member_resolution(record_at, MemberResolution::External(external));
            }
            return Some(crate::method_resolution::instantiate_external_signature(
                &function,
                &instantiation,
            ));
        }
        // An implements-block method is not a class-inherent export. Resolve
        // mounted UFCS (`app.Widget.describe(widget)`) through the same impl
        // registry as a bound member, but retain `self` in the callable type.
        if let Some(exported) = self.lower.resolve_exported_type_definition(prefix)
            && let crate::package_interface::ExportedType::Class {
                qtn,
                generic_params,
                ..
            } = exported.as_ref()
        {
            let class_args: Vec<Ty> = generic_params
                .iter()
                .map(|param| self.fresh_generic_arg(param))
                .collect();
            let receiver = Ty::intern(InferTy::Class(
                qtn.clone(),
                class_args.into(),
                TyAttr::default(),
            ));
            if let crate::method_resolution::InterfaceMemberLookup::Found(interface_member) =
                crate::method_resolution::lookup_interface_member(
                    self.db,
                    &self.facts,
                    &receiver,
                    member,
                )
                && interface_member.is_method
            {
                let resolution = self.declarer_resolution(&interface_member.declarer, member);
                let fn_ty = match own {
                    OwnArgs::Call(call) => {
                        self.interface_member_callee(interface_member, call, false)
                            .0
                    }
                    OwnArgs::Fresh => self.interface_member_unbound_value(interface_member),
                };
                if let Some(record_at) = record_at
                    && let Some(resolution) = resolution
                {
                    self.write_member_resolution(record_at, resolution);
                }
                return Some(fn_ty);
            }
        }
        // TIER: a type-qualified implements-block member on a SOURCE class -
        // the bare spelling of the `(C as I).item` projection with the
        // interface INFERRED.
        self.class_impl_static_value(prefix, member, own, anchor, record_at)
    }

    /// The impl tier of [`Self::class_static_value`]: `C.item` /
    /// `C<args>.item` where `item` lives in an implements block (in-class or
    /// free alike - the block spelling is metadata, not semantics). Mirrors
    /// [`Self::qualified_path_value`] with the qualifier inferred: the
    /// determination must be UNIQUE - two declaring interfaces need the
    /// `(C as I).item` spelling (E0121's rule) - and the resolved member
    /// types exactly as the qualified spelling would, so self-less statics
    /// dispatch type-keyed and UFCS methods keep `self` as the written
    /// first argument.
    ///
    /// The receiver must be GROUND before determination runs (the impl
    /// matcher admits no inference variables), so the class arguments come
    /// only from the spelling: an alias expansion's pinned args, the hoisted
    /// receiver args (`Bin<int>.build(2)` - BEP-039 moves `<int>` onto the
    /// call channel), or an empty frame. A generic class with no written
    /// arguments does not reach this tier - which interface declares the
    /// member could depend on the very arguments inference has not solved.
    fn class_impl_static_value(
        &mut self,
        prefix: &[baml_type::Name],
        member: &baml_type::Name,
        own: OwnArgs,
        anchor: ExprId,
        record_at: Option<ExprId>,
    ) -> Option<Ty> {
        let (class, pinned) = self.static_class_for(prefix)?;
        let frame = crate::lower::class_generic_frame(self.db, class);
        // The class arguments and whether the call's written type-arg channel
        // was consumed for them (the hoisted-receiver-args spelling).
        let (args, channel_consumed) = match pinned {
            Some(args) => (args, false),
            None if frame.is_empty() => (Vec::new(), false),
            None => {
                let OwnArgs::Call(call) = own else {
                    return None;
                };
                let written = self.type_refs.expr_type_args.get(&call)?.clone();
                // The whole prefix must be written and static: a partial or
                // runtime instantiation cannot ground the receiver here.
                if written.len() != frame.len()
                    || written
                        .iter()
                        .any(|slot| matches!(slot, BodyTypeArgRef::Runtime { .. }))
                {
                    return None;
                }
                // Lowered WITHOUT call-plan slot recording: these args live
                // inside the `Self` template, and the interface-item road
                // reads recorded slots as the method's OWN suffix.
                let args: Vec<Ty> = written
                    .iter()
                    .map(|slot| {
                        let BodyTypeArgRef::Static(type_ref) = slot else {
                            unreachable!("runtime slots were rejected above");
                        };
                        let (lowered, diagnostics) = self.lower_body_type_ref_at(
                            *type_ref,
                            crate::lower::TypePosition::Existential,
                        );
                        self.queue_body_lowering_diagnostics(diagnostics);
                        self.reject_expr_position_holes(&lowered, anchor)
                    })
                    .collect();
                (args, true)
            }
        };
        let qself =
            crate::lower::class_ty(crate::lower::class_qualified_name(self.db, class), args);
        if qself.has_infer() || qself.has_error() {
            return None;
        }
        let (determination, _) = crate::interfaces::determine_member_interface_with_facts(
            self.db,
            &self.facts,
            &rendered_plain(&qself),
            None,
            member,
            crate::interfaces::MemberNamespace::Value,
        );
        let realized = match determination {
            crate::interfaces::Determination::Determined(realized) => realized,
            crate::interfaces::Determination::Ambiguous(candidates) => {
                if self.member_probe_depth == 0 {
                    self.pending_diags.push(PendingDiag::AmbiguousMember {
                        expr: anchor,
                        base: qself,
                        member: member.clone(),
                        sources: candidates
                            .iter()
                            .map(|iface| {
                                InferInterface::new(
                                    iface.name.clone(),
                                    iface.generics.iter().map(Ty::from_plain).collect(),
                                    iface
                                        .associated_types
                                        .iter()
                                        .map(|(name, ty)| (name.clone(), Ty::from_plain(ty)))
                                        .collect(),
                                )
                            })
                            .collect(),
                        is_field: false,
                    });
                }
                return Some(Ty::error());
            }
            crate::interfaces::Determination::Undeclared { .. }
            | crate::interfaces::Determination::SubjectDoesNotImplementQualifier { .. }
            | crate::interfaces::Determination::InvalidBase
            | crate::interfaces::Determination::Poisoned => return None,
        };
        let interface_loc = self.interface_loc_for(&realized.name)?;
        // A consumed channel holds the CLASS args, so the member's own
        // generics (if any) instantiate fresh instead of re-reading it.
        let own = if channel_consumed {
            OwnArgs::Fresh
        } else {
            own
        };
        self.item_projection_value(
            interface_loc,
            Some(&WrittenQualifier {
                qself,
                realized: &realized,
            }),
            member,
            own,
            anchor,
            record_at,
        )
    }

    fn source_class_static_value(
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
        let bounds = crate::lower::function_generic_bounds(self.db, method);
        let position = if self.is_reflect_package_get_function(class, method) {
            crate::lower::TypePosition::ExtractionContract
        } else {
            crate::lower::TypePosition::Existential
        };
        // An unbound/static class method supplies the whole declared frame at
        // the call site: owner params first, then function params (TIR's
        // `UnboundMethod` rule). An alias-qualified class already pins the
        // owner prefix, so only the method suffix remains writable.
        let (instantiation, own_offset) = match pinned {
            Some(owner_args) => {
                let own_params = &signature.generic_params[frame.len()..];
                let mut instantiation = owner_args;
                instantiation.extend(
                    self.own_instantiation_with_bounds(own, own_params, member, &bounds, position),
                );
                (instantiation, frame.len())
            }
            None => (
                self.own_instantiation_with_bounds(
                    own,
                    &signature.generic_params,
                    member,
                    &bounds,
                    position,
                ),
                0,
            ),
        };
        if let OwnArgs::Call(call) = own {
            let instantiation = self.write_call_type_args(call, &instantiation, own_offset);
            self.register_call_bounds(method, &instantiation, anchor);
            self.record_runtime_dependent_arguments(call, signature, false);
            if let Some(record_at) = record_at {
                self.write_member_resolution(
                    record_at,
                    MemberResolution::UnboundMethod {
                        class,
                        func: method,
                    },
                );
            }
            return Some(function_value_ty(signature, &instantiation));
        }
        self.register_call_bounds(method, &instantiation, anchor);
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

    /// Source-less counterpart of [`Self::static_class_for`] for primitive or
    /// alias qualifiers. Fully qualified external classes resolve in the
    /// ordinary exported-type tier above; this bridge handles spellings such
    /// as `string.from` whose methods live on `baml.String`.
    fn static_external_class_for(
        &self,
        prefix: &[baml_type::Name],
    ) -> Option<(Box<crate::package_interface::ExportedType>, Vec<Ty>)> {
        if prefix
            .first()
            .is_some_and(|name| self.scoped_type_param(name).is_some())
        {
            return None;
        }
        let ty = self.static_qualifier_ty(prefix)?;
        let (qtn, args) = crate::method_resolution::external_class_for_type(&self.facts, &ty, 8)?;
        let exported = crate::package_interface::mounted_type_row(self.db, &qtn)?.clone();
        Some((Box::new(exported), args))
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
        let lowered = self.lower_scoped_type_path(segments);
        let baml_type::LoweringTy::EnumVariant(qtn, variant, _) = &lowered else {
            return None;
        };
        let ty = crate::impls::interned_ty(&crate::lower::reject_holes(&lowered));
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
        } else if let Some(record_at) = record_at
            && matches!(
                crate::package_interface::mounted_type_row(self.db, qtn),
                Some(crate::package_interface::ExportedType::Enum { .. })
            )
        {
            self.write_member_resolution(
                record_at,
                MemberResolution::ExternalVariant {
                    enum_name: qtn.clone(),
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
        if let OwnArgs::Call(call) = own
            && self
                .type_refs
                .expr_type_args
                .get(&call)
                .is_some_and(|slots| {
                    slots
                        .iter()
                        .any(|slot| matches!(slot, BodyTypeArgRef::Runtime { .. }))
                })
        {
            return None;
        }
        let written = self.lower_scoped_type_path(prefix);
        let target = if !written.contains_error() {
            crate::impls::interned_ty(&crate::lower::reject_holes(&written))
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
        if prefix
            .first()
            .is_some_and(|name| self.scoped_type_param(name).is_some())
        {
            return None;
        }
        if let Some(Definition::Class(class)) = self.lower.resolve_type_definition(prefix) {
            return Some((class, None));
        }
        let ty = self.static_qualifier_ty(prefix)?;
        crate::method_resolution::receiver_class(&self.facts, &ty, 8)
            .map(|(class, args)| (class, Some(args)))
    }

    /// Anything else a static qualifier can denote: a primitive or media
    /// KEYWORD (annotation-grammar tokens, not paths), or an ALIAS (chains
    /// included). It becomes the TYPE it names, and the S11 receiver-class
    /// correspondence maps type to class, the same table instance receivers
    /// use. rust-analyzer expands aliases at lowering so every consumer sees
    /// the target; our lazy-alias design expands at the demand point.
    fn static_qualifier_ty(&self, prefix: &[baml_type::Name]) -> Option<Ty> {
        let ty = match prefix {
            [single] => match single.as_str() {
                "int" => Ty::intern(InferTy::Int {
                    attr: baml_type::TyAttr::default(),
                }),
                "bigint" => Ty::intern(InferTy::Bigint {
                    attr: baml_type::TyAttr::default(),
                }),
                "float" => Ty::intern(InferTy::Float {
                    attr: baml_type::TyAttr::default(),
                }),
                "string" => Ty::intern(InferTy::String {
                    attr: baml_type::TyAttr::default(),
                }),
                "bool" => Ty::intern(InferTy::Bool {
                    attr: baml_type::TyAttr::default(),
                }),
                "uint8array" => Ty::intern(InferTy::Uint8Array {
                    attr: baml_type::TyAttr::default(),
                }),
                "image" => Ty::intern(InferTy::Media(
                    baml_type::MediaKind::Image,
                    baml_type::TyAttr::default(),
                )),
                "audio" => Ty::intern(InferTy::Media(
                    baml_type::MediaKind::Audio,
                    baml_type::TyAttr::default(),
                )),
                "video" => Ty::intern(InferTy::Media(
                    baml_type::MediaKind::Video,
                    baml_type::TyAttr::default(),
                )),
                "pdf" => Ty::intern(InferTy::Media(
                    baml_type::MediaKind::Pdf,
                    baml_type::TyAttr::default(),
                )),
                _ => crate::impls::interned_ty(&crate::lower::reject_holes(
                    &self.lower_scoped_type_path(prefix),
                )),
            },
            _ => crate::impls::interned_ty(&crate::lower::reject_holes(
                &self.lower_scoped_type_path(prefix),
            )),
        };
        if ty.has_error() {
            return None;
        }
        Some(ty)
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
        if prefix
            .first()
            .is_some_and(|name| self.scoped_type_param(name).is_some())
        {
            return None;
        }
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
    /// function type flowing down, or become fresh inference variables that
    /// later uses can constrain. An omitted `throws` is inferred from the
    /// lambda's body.
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
            .and_then(|ty| self.callback_root_fn(&ty))
            // A union with one concrete callback arm expects that function
            // here just as much as an immediate callback does: the other arms
            // provide no lambda signature. Without this the lambda's params
            // remain unconstrained and its throws cannot flow through the
            // callback slot's synthetic effect.
            .and_then(|ty| match ty.kind() {
                InferTy::Function {
                    params,
                    ret,
                    throws,
                    ..
                } => Some((params.clone(), ret.clone(), throws.clone())),
                _ => None,
            });

        let expected_arity = expected_fn.as_ref().and_then(|(expected_params, _, _)| {
            (expected_params.len() != def.params.len()).then_some(expected_params.len())
        });
        if let Some(expected) = expected_arity {
            self.pending_diags.push(PendingDiag::ArgCountMismatch {
                expr,
                expected,
                got: def.params.len(),
            });
        }

        let param_tys: Vec<Ty> = def
            .params
            .iter()
            .enumerate()
            .map(|(index, param)| {
                let annotated = signature
                    .as_ref()
                    .and_then(|sig| sig.params.get(index).copied().flatten());
                match annotated {
                    Some(type_ref) => self.lower_body_annotation(type_ref),
                    None => match expected_fn
                        .as_ref()
                        .and_then(|(params, _, _)| params.get(index))
                    {
                        Some(expected) => expected.ty.clone(),
                        None if expected_fn.is_some() => Ty::error(),
                        None => self.untyped_lambda_parameter_ty(expr, index, param.name.clone()),
                    },
                }
            })
            .collect();

        let annotated_ret = signature
            .as_ref()
            .and_then(|sig| sig.return_type)
            .map(|type_ref| self.lower_body_annotation(type_ref));
        // Signature deduction from expectation (rustc's
        // `check_supplied_sig_against_expectation`, which eq-unifies the
        // supplied closure signature against the expected one and ignores
        // failures; rust-analyzer's closure-sig deduction is the same): a WRITTEN
        // annotation commits into the expected function type's corresponding
        // slot while that slot is still an unsolved inference variable. This
        // is what keeps a generic call's type argument "known unambiguously
        // at the call site" (TYPE_SYSTEM.md) when the lambda spells it out —
        // without the commit, the callee's own param (e.g. `Array.map`'s `U`)
        // waits for the finish fixpoint while the walk continues, and a later
        // member access on a receiver derived from it inspects a still
        // unsolved var and silently finalizes `Error`, miscompiling
        // downstream. A slot that already resolved structurally is left to
        // the ordinary argument check (and unify mismatches are ignored here
        // for the same reason: the arg check re-judges and reports them).
        // `throws` is deliberately excluded — the effect channel owns it.
        if let Some((exp_params, exp_ret, _)) = &expected_fn {
            for (index, ty) in param_tys.iter().enumerate() {
                let written = signature
                    .as_ref()
                    .and_then(|sig| sig.params.get(index).copied().flatten())
                    .is_some();
                if !written || ty.has_error() {
                    continue;
                }
                let Some(expected_param) = exp_params.get(index) else {
                    continue;
                };
                let expected = self.structurally_resolve(&expected_param.ty);
                if expected.has_infer() {
                    let _ = self.table.unify(ty, &expected);
                }
            }
            if let Some(ret) = &annotated_ret
                && !ret.has_error()
            {
                let expected = self.structurally_resolve(exp_ret);
                if expected.has_infer() {
                    let _ = self.table.unify(ret, &expected);
                }
            }
        }
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
                let saved_defer_loop_floors = std::mem::take(&mut self.defer_loop_floors);
                let saved_loop_depth = std::mem::replace(&mut self.loop_depth, 0);
                self.return_frames.push(ReturnFrame {
                    expected: ret_expectation.clone(),
                    candidates: Vec::new(),
                });
                let body_ty = match &ret_expectation {
                    // A void lambda DISCARDS its body's tail value, the
                    // same statement semantics void functions get (test
                    // bodies are synthesized `() -> void` lambdas).
                    Some(ret) if is_unit(ret) => {
                        self.infer_expr(body, lambda_body, &Expectation::None)
                    }
                    Some(ret) if !ret.has_error() => self.check_expr(body, lambda_body, ret),
                    _ => self.infer_expr(body, lambda_body, &Expectation::None),
                };
                let mut return_frame = self.return_frames.pop().expect("pushed above");
                return_frame.candidates.push(body_ty);
                let joined = self.join_return_candidates(&return_frame.candidates);
                let inferred_ret = self.widen_fresh(&joined);
                let ret_ty = match ret_expectation {
                    Some(ret) if is_unit(&ret) => ret,
                    Some(ret) if !ret.has_error() => {
                        if ret.has_infer() {
                            self.sub(&inferred_ret, &ret);
                        }
                        ret
                    }
                    _ => inferred_ret,
                };
                self.loop_depth = saved_loop_depth;
                self.defer_loop_floors = saved_defer_loop_floors;
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
                        && !matches!(throws.kind(), InferTy::Unknown { .. })
                })
        } else {
            None
        };
        let written_throws = written_throws.or(contextual_throws);
        // A WRITTEN closed clause is the lambda's contract: its body's
        // contributions check against it exactly as a function's do
        // (open contributions judge at finalize).
        if let Some(declared) = &written_throws {
            // The annotation funnel already instantiated any written `_`
            // member as a fresh effect variable, so a lambda clause is never
            // PARTIAL here — the variable itself carries the openness, and
            // `sub` below binds contributions into it (the pre-split
            // `throws_clause_parts` probe on the instantiated clause was
            // vacuously closed).
            if !declared.has_error() {
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

        let params: Box<[baml_type::interned::InferFunctionParamTy]> = def
            .params
            .iter()
            .zip(&param_tys)
            .map(|(param, ty)| baml_type::interned::InferFunctionParamTy {
                name: Some(param.name.clone()),
                ty: ty.clone(),
                mode: if param.default.is_some() {
                    baml_type::FunctionParamMode::Optional
                } else {
                    baml_type::FunctionParamMode::Required
                },
            })
            .collect();
        let ty = Ty::intern(InferTy::Function {
            params,
            ret: ret_ty,
            throws: throws_ty,
            attr: TyAttr::default(),
        });
        if expected_arity.is_some() {
            Ty::error()
        } else {
            ty
        }
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
        let runtime_params = self.runtime_call_params(at);
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
                // Declared bounds are plain; the obligation machinery's
                // vocabulary is interned — ingest once per bound.
                let bound = InferInterface::from_constraint(&bound);
                let bound = &bound;
                if runtime_params.is_empty() && self.scoped_type_bindings.is_empty() {
                    let interface = baml_type::interned::InferInterface::new(
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
                    continue;
                }
                let runtime_slot_dependent = runtime_params
                    .iter()
                    .any(|runtime| runtime == &param || interface_mentions_param(bound, runtime));
                let argument = substitute_static_call_params(
                    &Ty::intern(InferTy::TypeVar(param.clone(), TyAttr::default())),
                    instantiation,
                    &runtime_params,
                );
                let bound =
                    substitute_static_interface_params(bound, instantiation, &runtime_params);
                let scoped_binding_dependent = self.scoped_type_bindings.iter().any(|binding| {
                    ty_mentions_param(&argument, &binding.parameter)
                        || interface_mentions_param(&bound, &binding.parameter)
                });
                if runtime_slot_dependent || scoped_binding_dependent {
                    self.result
                        .call_plans
                        .entry(at)
                        .or_default()
                        .deferred_checks
                        .push(RuntimeCheck::Bound { argument, bound });
                    continue;
                }
                self.register_obligation(obligations::Obligation::Implements {
                    ty: arg.clone(),
                    interface: bound,
                    at,
                    not_concrete_rejects: (param.index() as usize) >= own_start,
                });
            }
        }
    }

    fn register_external_call_bounds(
        &mut self,
        external: &crate::callable::ExternalCallable,
        instantiation: &[Ty],
        at: ExprId,
    ) {
        let runtime_params = self.runtime_call_params(at);
        let own_start = external.owner_generic_params.len();
        let params = external
            .owner_generic_params
            .iter()
            .zip(&external.owner_generic_param_bounds)
            .chain(
                external
                    .generic_params
                    .iter()
                    .zip(&external.generic_param_bounds),
            );
        for (param, bounds) in params {
            let Some(arg) = instantiation.get(param.index() as usize) else {
                continue;
            };
            for bound in bounds {
                let bound = InferInterface::from_constraint(bound);
                let runtime_slot_dependent = runtime_params
                    .iter()
                    .any(|runtime| runtime == param || interface_mentions_param(&bound, runtime));
                let argument = substitute_static_call_params(
                    &Ty::intern(InferTy::TypeVar(param.clone(), TyAttr::default())),
                    instantiation,
                    &runtime_params,
                );
                let bound =
                    substitute_static_interface_params(&bound, instantiation, &runtime_params);
                let scoped_binding_dependent = self.scoped_type_bindings.iter().any(|binding| {
                    ty_mentions_param(&argument, &binding.parameter)
                        || interface_mentions_param(&bound, &binding.parameter)
                });
                if runtime_slot_dependent || scoped_binding_dependent {
                    self.result
                        .call_plans
                        .entry(at)
                        .or_default()
                        .deferred_checks
                        .push(RuntimeCheck::Bound { argument, bound });
                    continue;
                }
                self.register_obligation(obligations::Obligation::Implements {
                    ty: arg.clone(),
                    interface: bound,
                    at,
                    not_concrete_rejects: (param.index() as usize) >= own_start,
                });
            }
        }
    }

    fn record_external_runtime_dependent_arguments(
        &mut self,
        call: ExprId,
        function: &crate::package_interface::ResolvedFunction,
        bound_receiver: bool,
        instantiation: &[Ty],
    ) {
        let runtime_params = self.runtime_call_params(call);
        if runtime_params.is_empty() {
            return;
        }
        self.report_runtime_type_escape(
            call,
            &Ty::from_plain(&function.return_type),
            RuntimeTypeEscape::Value,
        );
        self.report_runtime_type_escape(
            call,
            &Ty::from_plain(&function.callable_throws),
            RuntimeTypeEscape::Error,
        );
        let mut dependent = FxHashMap::default();
        for (param_index, param) in function
            .params
            .iter()
            .skip(usize::from(bound_receiver))
            .enumerate()
        {
            let template = Ty::from_plain(&param.ty);
            if runtime_params
                .iter()
                .any(|runtime| ty_mentions_param(&template, runtime))
            {
                dependent.insert(
                    param_index,
                    substitute_static_call_params(&template, instantiation, &runtime_params),
                );
            }
        }
        if !dependent.is_empty() {
            self.runtime_dependent_call_params.insert(call, dependent);
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
                // Declared bounds are plain; the obligation machinery's
                // vocabulary is interned — ingest once per bound.
                let bound = InferInterface::from_constraint(&bound);
                let bound = &bound;
                let interface = baml_type::interned::InferInterface::new(
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
        self.instantiation_args_with_bounds(
            site,
            generic_params,
            callee,
            &FxHashMap::default(),
            crate::lower::TypePosition::Existential,
        )
    }

    /// The call-site instantiation road with the callee's declared bounds and
    /// the one context-sensitive type position supplied explicitly. Written
    /// slots align only with user-writable params; synthetic effect params
    /// always receive fresh effect variables.
    fn instantiation_args_with_bounds(
        &mut self,
        site: ExprId,
        generic_params: &[baml_type::ParamTy],
        callee: Option<&baml_type::Name>,
        bounds: &FxHashMap<baml_type::ParamTy, Vec<baml_type::Interface>>,
        position: crate::lower::TypePosition,
    ) -> Vec<Ty> {
        let written = self
            .type_refs
            .expr_type_args
            .get(&site)
            .cloned()
            .unwrap_or_default();
        // Explicit turbofish counts against the WRITABLE params (synthetic
        // effect params are elaboration's, never spelled).
        if let Some(callee) = callee
            && !written.is_empty()
        {
            let expected = generic_params
                .iter()
                .filter(|param| !baml_type::is_synthetic_effect_param(param.name()))
                .count();
            if written.len() != expected {
                self.pending_diags.push(PendingDiag::WrongTypeArgArity {
                    expr: site,
                    callee: callee.clone(),
                    expected,
                    got: written.len(),
                });
            }
        }
        let mut written = written.iter();
        let mut slots = Vec::new();
        let mut instantiation = Vec::with_capacity(generic_params.len());
        for param in generic_params {
            if baml_type::is_synthetic_effect_param(param.name()) {
                instantiation.push(self.fresh_generic_arg(param));
                continue;
            }
            let Some(slot) = written.next() else {
                instantiation.push(self.fresh_generic_arg(param));
                continue;
            };
            match slot {
                BodyTypeArgRef::Static(type_ref) => {
                    let computed = self.computed_generic_argument_name(*type_ref, site);
                    let (lowered, diagnostics) = self.lower_body_type_ref_at(*type_ref, position);
                    let runtime_bindings = self
                        .result
                        .type_ref_bindings
                        .get(&self.type_refs.raw_id(*type_ref))
                        .cloned()
                        .unwrap_or_default();
                    if let Some(name) = computed {
                        self.specialize_computed_generic_diagnostic(
                            diagnostics,
                            *type_ref,
                            site,
                            &name,
                        );
                    } else {
                        self.queue_body_lowering_diagnostics(diagnostics);
                    }
                    let ty = self.reject_expr_position_holes(&lowered, site);
                    slots.push(CallTypeArgPlan::Static {
                        ty: ty.clone(),
                        emission_ty: ty.clone(),
                        runtime_bindings,
                    });
                    instantiation.push(ty);
                }
                BodyTypeArgRef::Runtime { operand } => {
                    let occurrence_ty = bounds
                        .get(param)
                        .and_then(|bounds| bounds.first())
                        .map(interface_occurrence_ty)
                        .unwrap_or_else(|| {
                            Ty::intern(InferTy::Unknown {
                                attr: TyAttr::default(),
                            })
                        });
                    slots.push(CallTypeArgPlan::Runtime {
                        operand: *operand,
                        occurrence_ty: occurrence_ty.clone(),
                        parameter: param.clone(),
                    });
                    instantiation.push(occurrence_ty);
                }
            }
        }
        if !slots.is_empty() {
            self.result.call_plans.entry(site).or_default().slots = slots;
        }
        instantiation
    }

    fn runtime_call_params(&self, call: ExprId) -> Vec<baml_type::ParamTy> {
        self.result
            .call_plans
            .get(&call)
            .into_iter()
            .flat_map(|plan| &plan.slots)
            .filter_map(|slot| match slot {
                CallTypeArgPlan::Runtime { parameter, .. } => Some(parameter.clone()),
                CallTypeArgPlan::Static { .. } => None,
            })
            .collect()
    }

    /// BEP-066 ruling (A): an inline `unreflect(value)` type argument is legal
    /// only while the runtime type stays out of the expression's published
    /// type. The parameter is rigid for this call alone — the call site
    /// publishes `occurrence_ty` in its place — so a published type that still
    /// mentions the parameter would be typed by a substitution the value does
    /// not actually satisfy afterwards, and every later dispatch re-derives
    /// the receiver's arguments from that published type.
    ///
    /// The exception, and the reason this is an occurs-check on the published
    /// type rather than a ban on the spelling, is a type that IS the parameter
    /// (`parse<T>(..) -> T`): occurrence-substitution then types a VALUE, the
    /// runtime tag rides on the value itself, and nothing static claims more
    /// than `unknown`. That is the supported dynamic path and stays legal.
    /// One position deeper — `Wrapper<T>`, `T[]`, `T?`, a constructed `C<T>` —
    /// the occurrence substitutes into a type CONSTRUCTOR, and the published
    /// type starts asserting something about the value.
    ///
    /// A call publishes two types, and the rule reads the same in both: the
    /// result the caller binds, and the [`RuntimeTypeEscape::Error`] a
    /// `throws` clause hands to the caller's handler — which is just as
    /// visible after the call returns, and just as often written `Boom<T>`.
    fn report_runtime_type_escape(
        &mut self,
        call: ExprId,
        published: &Ty,
        escape: RuntimeTypeEscape,
    ) {
        if escape == RuntimeTypeEscape::Value {
            // The `?.` check runs later, at the chain boundary, where the
            // callee's signature is long out of reach — so the one fact it
            // needs is recorded here: does this call's RESULT name the
            // parameter at all? A result that never mentions it (`-> bool`,
            // `-> Wrapper<unknown>`) publishes nothing about the runtime type,
            // and wrapping nothing in `| null` is still nothing.
            self.runtime_slots_named_by_result.extend(
                self.escaping_carriers(call, |parameter| ty_mentions_param(published, parameter)),
            );
        }
        let escaping = self.escaping_carriers(call, |parameter| {
            runtime_param_escapes(published, parameter)
        });
        self.report_escaping_carriers(call, escaping, escape);
    }

    /// A class literal publishes its instantiated class type directly. Every
    /// inline runtime slot is therefore embedded in that result, including
    /// carriers nested inside a nominal argument.
    fn report_object_runtime_type_escape(&mut self, object: ExprId) {
        let escaping = self.escaping_carriers(object, |_| true);
        self.report_escaping_carriers(object, escaping, RuntimeTypeEscape::Value);
    }

    /// The carrier expressions of `call`'s inline `unreflect(...)` slots whose
    /// parameter `escapes`.
    fn escaping_carriers(
        &self,
        call: ExprId,
        escapes: impl Fn(&baml_type::ParamTy) -> bool,
    ) -> Vec<ExprId> {
        self.result
            .call_plans
            .get(&call)
            .into_iter()
            .flat_map(|plan| &plan.slots)
            .flat_map(|slot| match slot {
                CallTypeArgPlan::Runtime {
                    operand, parameter, ..
                } => escapes(parameter)
                    .then_some(*operand)
                    .into_iter()
                    .collect::<Vec<_>>(),
                CallTypeArgPlan::Static {
                    runtime_bindings, ..
                } => runtime_bindings
                    .iter()
                    .filter(|binding| escapes(&binding.parameter))
                    .filter_map(|binding| binding.operand)
                    .collect(),
            })
            .collect()
    }

    fn report_escaping_carriers(
        &mut self,
        enclosing: ExprId,
        carriers: Vec<ExprId>,
        escape: RuntimeTypeEscape,
    ) {
        for carrier in carriers {
            // A callee can be typed more than once (the interface probe
            // re-runs the member road), and a slot can escape through more
            // than one published type; the slot is reported once.
            if self.reported_runtime_escapes.insert(carrier) {
                self.pending_diags
                    .push(PendingDiag::RuntimeTypeMustBeNamed {
                        carrier,
                        enclosing,
                        escape,
                    });
            }
        }
    }

    /// The `?.` arm of the same rule, reported at the chain BOUNDARY because
    /// that is where the wrapper appears: a short-circuiting chain republishes
    /// its tail's result as `T | null`, so a call whose bare `-> T` result was
    /// legal on its own stops being legal once `?.` wraps it. This is the
    /// spelling half of the declared `-> T?` refusal — both publish `unknown?`
    /// and both now say the same thing.
    ///
    /// It is still the published type that decides, never the punctuation: a
    /// tail whose result never mentions the parameter (`-> bool`, or the
    /// declared-erased `-> Wrapper<unknown>`) publishes nothing about the
    /// runtime type, so the chain's `| null` has nothing to wrap and the call
    /// stays legal. [`Self::report_runtime_type_escape`] recorded that fact
    /// while the callee's signature was still in hand.
    ///
    /// Only the chain's TAIL is affected: it is the expression whose value the
    /// chain republishes. A call in argument position, or one whose result is
    /// consumed further along the chain (`a?.b.m<unreflect(t)>().field`),
    /// publishes its own result unchanged and keeps whatever verdict its
    /// signature earned.
    fn report_chain_null_escape(&mut self, body: &ExprBody, tail: ExprId) {
        if !matches!(
            body.exprs[tail],
            Expr::Call { .. } | Expr::OptionalCall { .. }
        ) {
            return;
        }
        let escaping = self.escaping_carriers(tail, |_| true);
        let escaping: Vec<ExprId> = escaping
            .into_iter()
            .filter(|carrier| self.runtime_slots_named_by_result.contains(carrier))
            .collect();
        self.report_escaping_carriers(tail, escaping, RuntimeTypeEscape::Value);
    }

    fn record_runtime_dependent_arguments(
        &mut self,
        call: ExprId,
        signature: &crate::lower::FunctionSignature,
        bound_receiver: bool,
    ) {
        if !self.result.call_plans.get(&call).is_some_and(|plan| {
            plan.slots.iter().any(|slot| match slot {
                CallTypeArgPlan::Runtime { .. } => true,
                CallTypeArgPlan::Static {
                    runtime_bindings, ..
                } => !runtime_bindings.is_empty(),
            })
        }) {
            return;
        }
        self.report_runtime_type_escape(
            call,
            &crate::impls::interned_ty(&signature.ret),
            RuntimeTypeEscape::Value,
        );
        // `signature.throws` is the DECLARED clause when the author wrote one
        // and the inferred effect otherwise (S12). Both are published to the
        // caller, so both are checked — the note is worded for a clause the
        // author may never have spelled.
        self.report_runtime_type_escape(
            call,
            &crate::impls::interned_ty(&signature.throws),
            RuntimeTypeEscape::Error,
        );
        if let Some(instantiation) = self
            .result
            .call_plans
            .get(&call)
            .map(|plan| plan.type_args.clone())
        {
            let instantiated_ret =
                substitute_params(&crate::impls::interned_ty(&signature.ret), &instantiation);
            let instantiated_throws = substitute_params(
                &crate::impls::interned_ty(&signature.throws),
                &instantiation,
            );
            self.report_runtime_type_escape(call, &instantiated_ret, RuntimeTypeEscape::Value);
            self.report_runtime_type_escape(call, &instantiated_throws, RuntimeTypeEscape::Error);
        }
        let runtime_params = self.runtime_call_params(call);
        if runtime_params.is_empty() {
            return;
        }
        let Some(instantiation) = self
            .result
            .call_plans
            .get(&call)
            .map(|plan| plan.type_args.clone())
        else {
            return;
        };
        let mut dependent = FxHashMap::default();
        for (param_index, param) in signature
            .params
            .iter()
            .skip(usize::from(bound_receiver))
            .enumerate()
        {
            let param_ty = crate::impls::interned_ty(&param.ty);
            if runtime_params
                .iter()
                .any(|runtime| ty_mentions_param(&param_ty, runtime))
            {
                dependent.insert(
                    param_index,
                    substitute_static_call_params(&param_ty, &instantiation, &runtime_params),
                );
            }
        }
        if !dependent.is_empty() {
            self.runtime_dependent_call_params.insert(call, dependent);
        }
    }

    fn is_reflect_package_get_function(
        &self,
        class: baml_compiler2_hir::loc::ClassLoc<'db>,
        function: baml_compiler2_hir::loc::FunctionLoc<'db>,
    ) -> bool {
        let qtn = crate::lower::class_qualified_name(self.db, class);
        qtn.package().as_str() == "reflect"
            && qtn.namespace().is_empty()
            && qtn.name().as_str() == "Package"
            && baml_compiler2_ppir::item_data::function_data(self.db, function)
                .name
                .as_str()
                == "get_function"
    }

    fn computed_generic_argument_name(
        &self,
        type_ref: BodyTypeRefId,
        site: ExprId,
    ) -> Option<baml_type::Name> {
        use baml_compiler2_hir::type_ref::TypeRefKind;
        let written = &self.type_refs.store[self.type_refs.raw_id(type_ref)];
        let TypeRefKind::Path {
            segments,
            generic_args,
            associated_type_bindings,
        } = &written.kind
        else {
            return None;
        };
        if !written.attrs.is_empty()
            || segments.len() != 1
            || !generic_args.is_empty()
            || !associated_type_bindings.is_empty()
        {
            return None;
        }
        let name = &segments[0];
        let is_value = self.local_binding_names_at(site).contains(name)
            || self.lower.resolve_value(segments).is_some();
        let is_type = self.lower.resolve_type_definition(segments).is_some()
            || self.scoped_type_param(name).is_some()
            || self
                .lower
                .generic_params()
                .iter()
                .any(|param| param.name() == name);
        (is_value && !is_type).then(|| name.clone())
    }

    /// Replace a generic unresolved-type finding for a value-shaped slot with
    /// M-1's targeted whole-slot diagnostic. Any other findings from the same
    /// lowering operation retain their original anchors.
    fn specialize_computed_generic_diagnostic(
        &mut self,
        diagnostics: Vec<crate::lower::LoweringDiag>,
        type_ref: BodyTypeRefId,
        site: ExprId,
        name: &baml_type::Name,
    ) {
        let mut replaced = false;
        for lowering in diagnostics {
            if lowering.type_ref == self.type_refs.raw_id(type_ref)
                && matches!(
                    &lowering.kind,
                    crate::lower::LoweringDiagKind::Unresolved { name: unresolved, .. }
                        if unresolved == name
                )
            {
                replaced = true;
            } else {
                self.pending_diags.push(PendingDiag::BodyAnnot {
                    type_ref: self.type_refs.diagnostic_id(lowering.type_ref),
                    kind: lowering.kind,
                });
            }
        }
        if replaced {
            self.pending_diags
                .push(PendingDiag::ComputedGenericArgumentRequiresUnreflect {
                    expr: site,
                    name: name.clone(),
                });
        }
    }

    /// A fresh variable for one generic param at a use site: synthetic
    /// effect params get EFFECT variables (unconstrained defaults to
    /// `never`, not Error - S12's defaulting rule).
    fn fresh_generic_arg(&mut self, param: &baml_type::ParamTy) -> Ty {
        if baml_type::is_synthetic_effect_param(param.name()) {
            self.table.new_var_ty_of(unify::VarPolicy::Effect)
        } else {
            self.table.new_var_ty()
        }
    }

    /// Shared completeness check for local and mounted class constructors.
    /// A valid spread has the exact class type and therefore supplies every
    /// slot; without one, each omitted slot must admit its `null` initializer.
    fn report_missing_required_object_fields(
        &mut self,
        object: ExprId,
        class_name: &baml_type::QualifiedTypeName,
        field_types: &[(baml_type::Name, baml_type::Ty)],
        instantiation: &[Ty],
        fields: &[ObjectExprField],
        has_spread: bool,
    ) {
        if has_spread {
            return;
        }

        let mut missing = Vec::new();
        for (name, field_ty) in field_types {
            if fields.iter().any(|field| field.name == *name) {
                continue;
            }
            // Declaration types are plain; the engine ingests them here (the
            // total direction) to substitute and resolve.
            let field_ty = substitute_params(&crate::impls::interned_ty(field_ty), instantiation);
            let resolved = self.structurally_resolve(&field_ty);
            // An error sentinel means the declaration's rule is unknown, not
            // that the field is non-nullable. Its source diagnostic is the
            // actionable error; continue checking independently valid slots.
            if !resolved.has_error() && !type_admits_null(&resolved) {
                missing.push(name.clone());
            }
        }
        if !missing.is_empty() {
            self.pending_diags
                .push(PendingDiag::MissingRequiredObjectFields {
                    object,
                    class_name: class_name.clone(),
                    field_names: missing,
                });
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
        fields: &[ObjectExprField],
        spreads: &[baml_compiler2_ast::SpreadField],
    ) -> Ty {
        if type_name
            .0
            .first()
            .is_none_or(|name| self.scoped_type_param(name).is_none())
            && let Some(exported) = self.lower.resolve_exported_type_definition(&type_name.0)
            && let crate::package_interface::ExportedType::Class {
                qtn,
                fields: exported_fields,
                generic_params,
                generic_param_bounds,
                ..
            } = exported.as_ref()
        {
            return self.infer_exported_object(
                body,
                object,
                qtn.clone(),
                exported_fields,
                generic_params,
                generic_param_bounds,
                fields,
                spreads,
            );
        }
        let definition = type_name
            .0
            .first()
            .is_none_or(|name| self.scoped_type_param(name).is_none())
            .then(|| self.lower.resolve_type_definition(&type_name.0))
            .flatten();
        let Some(baml_compiler2_hir::contributions::Definition::Class(class)) = definition else {
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
            for field in fields {
                self.infer_expr(body, field.value, &Expectation::None);
            }
            for spread in spreads {
                self.infer_expr(body, spread.expr, &Expectation::None);
            }
            return Ty::error();
        };
        let db = self.db;
        let class_name = crate::lower::class_qualified_name(db, class);
        if baml_type::type_kind::is_type_kind_class(&class_name) {
            for field in fields {
                self.infer_expr(body, field.value, &Expectation::None);
            }
            for spread in spreads {
                self.infer_expr(body, spread.expr, &Expectation::None);
            }
            self.pending_diags
                .push(PendingDiag::CannotConstructReflectionKind {
                    expr: object,
                    class_name,
                });
            return Ty::error();
        }
        if let Some(companion) = baml_type::type_kind::builtin_companion_of(&class_name) {
            for field in fields {
                self.infer_expr(body, field.value, &Expectation::None);
            }
            for spread in spreads {
                self.infer_expr(body, spread.expr, &Expectation::None);
            }
            self.pending_diags
                .push(PendingDiag::CannotConstructBuiltinCompanion {
                    expr: object,
                    class_name,
                    companion,
                });
            return Ty::error();
        }
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
            let mentioned = field_types.iter().any(|(_, field_ty)| {
                ty_mentions_param(&crate::impls::interned_ty(field_ty), param)
            });
            if mentioned && arg.has_infer() {
                self.pending_diags.push(PendingDiag::UninferredCtorParam {
                    expr: object,
                    var: arg.clone(),
                    name: baml_type::Name::new(param.name().as_str()),
                });
            }
        }
        for field in fields {
            let name = &field.name;
            let value = field.value;
            match field_types.iter().find(|(field, _)| field == name) {
                Some((_, field_ty)) => {
                    let field_ty =
                        substitute_params(&crate::impls::interned_ty(field_ty), &instantiation);
                    self.check_expr(body, value, &field_ty);
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
                                let field_ty = substitute_params(
                                    &crate::impls::interned_ty(field_ty),
                                    &instantiation,
                                );
                                self.check_expr(body, value, &field_ty);
                            }
                            None => {
                                self.infer_expr(body, value, &Expectation::None);
                            }
                        }
                        continue;
                    }
                    self.infer_expr(body, value, &Expectation::None);
                    let shorthand = field.syntax == PropertySyntax::Shorthand;
                    let class_name = crate::lower::qualify_def(
                        db,
                        baml_compiler2_hir::contributions::Definition::Class(class),
                        &baml_compiler2_ppir::item_data::class_data(db, class).name,
                    );
                    self.pending_diags.push(PendingDiag::UnknownObjectField {
                        object,
                        value,
                        class_name,
                        declared: field_types.iter().map(|(field, _)| field.clone()).collect(),
                        name: name.clone(),
                        shorthand,
                    });
                }
            }
        }
        self.report_missing_required_object_fields(
            object,
            &class_name,
            &field_types,
            &instantiation,
            fields,
            !spreads.is_empty(),
        );
        let short = type_name.0.last().expect("type paths are never empty");
        let object_ty = Ty::intern(InferTy::Class(
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
        self.report_object_runtime_type_escape(object);
        object_ty
    }

    #[allow(clippy::too_many_arguments)]
    fn infer_exported_object(
        &mut self,
        body: &ExprBody,
        object: ExprId,
        class_name: baml_type::QualifiedTypeName,
        exported_fields: &[(
            baml_type::Name,
            baml_type::Ty,
            crate::package_interface::ExportedFieldAttrs,
        )],
        generic_params: &[baml_type::ParamTy],
        generic_param_bounds: &[Vec<baml_type::Interface>],
        fields: &[ObjectExprField],
        spreads: &[baml_compiler2_ast::SpreadField],
    ) -> Ty {
        if baml_type::type_kind::is_type_kind_class(&class_name) {
            for field in fields {
                self.infer_expr(body, field.value, &Expectation::None);
            }
            for spread in spreads {
                self.infer_expr(body, spread.expr, &Expectation::None);
            }
            self.pending_diags
                .push(PendingDiag::CannotConstructReflectionKind {
                    expr: object,
                    class_name,
                });
            return Ty::error();
        }
        if let Some(companion) = baml_type::type_kind::builtin_companion_of(&class_name) {
            for field in fields {
                self.infer_expr(body, field.value, &Expectation::None);
            }
            for spread in spreads {
                self.infer_expr(body, spread.expr, &Expectation::None);
            }
            self.pending_diags
                .push(PendingDiag::CannotConstructBuiltinCompanion {
                    expr: object,
                    class_name,
                    companion,
                });
            return Ty::error();
        }
        let mut instantiation = self.instantiation_args(object, generic_params, None);
        instantiation.truncate(generic_params.len());
        while instantiation.len() < generic_params.len() {
            instantiation.push(self.table.new_var_ty());
        }
        for (index, param) in generic_params.iter().enumerate() {
            let Some(arg) = instantiation.get(param.index() as usize) else {
                continue;
            };
            for bound in generic_param_bounds.get(index).into_iter().flatten() {
                let bound = InferInterface::from_constraint(bound);
                let interface = InferInterface::new(
                    bound.name.clone(),
                    bound
                        .generics
                        .iter()
                        .map(|ty| substitute_params(ty, &instantiation))
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                    bound
                        .associated_types
                        .iter()
                        .map(|(name, ty)| (name.clone(), substitute_params(ty, &instantiation)))
                        .collect(),
                );
                self.register_obligation(obligations::Obligation::Implements {
                    ty: arg.clone(),
                    interface,
                    at: object,
                    not_concrete_rejects: true,
                });
            }
        }
        let field_types: Vec<(baml_type::Name, Ty)> = exported_fields
            .iter()
            .map(|(name, ty, _)| (name.clone(), Ty::from_plain(ty)))
            .collect();
        for (slot, arg) in instantiation.iter().enumerate() {
            let Some(param) = generic_params.get(slot) else {
                continue;
            };
            if arg.has_infer()
                && field_types
                    .iter()
                    .any(|(_, field_ty)| ty_mentions_param(field_ty, param))
            {
                self.pending_diags.push(PendingDiag::UninferredCtorParam {
                    expr: object,
                    var: arg.clone(),
                    name: param.name().clone(),
                });
            }
        }
        for field in fields {
            let name = &field.name;
            let value = field.value;
            if let Some((_, field_ty)) = field_types.iter().find(|(field, _)| field == name) {
                self.check_expr(body, value, &substitute_params(field_ty, &instantiation));
            } else {
                self.infer_expr(body, value, &Expectation::None);
                let shorthand = field.syntax == PropertySyntax::Shorthand;
                self.pending_diags.push(PendingDiag::UnknownObjectField {
                    object,
                    value,
                    class_name: class_name.clone(),
                    declared: field_types.iter().map(|(field, _)| field.clone()).collect(),
                    name: name.clone(),
                    shorthand,
                });
            }
        }
        let plain_field_types: Vec<(baml_type::Name, baml_type::Ty)> = exported_fields
            .iter()
            .map(|(name, ty, _)| (name.clone(), ty.clone()))
            .collect();
        self.report_missing_required_object_fields(
            object,
            &class_name,
            &plain_field_types,
            &instantiation,
            fields,
            !spreads.is_empty(),
        );
        let object_ty = Ty::intern(InferTy::Class(
            class_name,
            instantiation.into_boxed_slice(),
            TyAttr::default(),
        ));
        for spread in spreads {
            self.check_expr(body, spread.expr, &object_ty);
        }
        self.report_object_runtime_type_escape(object);
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

    fn report_bare_output_format_reference(
        &mut self,
        expr: ExprId,
        class: baml_compiler2_hir::loc::ClassLoc<'db>,
        function: baml_compiler2_hir::loc::FunctionLoc<'db>,
    ) {
        let class_data = baml_compiler2_ppir::item_data::class_data(self.db, class);
        let function_data = baml_compiler2_ppir::item_data::function_data(self.db, function);
        let package = baml_compiler2_hir::file_package::file_package(self.db, class.file(self.db));
        let is_output_format = function_data.name.as_str() == "output_format"
            && package.package.as_str() == "ai"
            && match class_data.name.as_str() {
                "Context" => package.namespace_path.is_empty(),
                "SpecCtx" => package
                    .namespace_path
                    .iter()
                    .map(baml_type::Name::as_str)
                    .eq(["internal"]),
                _ => false,
            };
        if is_output_format {
            self.pending_diags
                .push(PendingDiag::BareOutputFormatReference { expr });
        }
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
    ) -> (Ty, Option<MemberResolution<'db, Ty>>) {
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
        if let InferTy::Union(members, _) = resolved.kind() {
            let members = members.to_vec();
            return self.union_member_access(at, &resolved, &members, member);
        }
        if let InferTy::Class(qtn, args, _) = resolved.kind()
            && let Some(baml_compiler2_hir::contributions::Definition::Class(class)) =
                self.facts.definition_of(qtn)
            && let Some((_, field_ty)) = crate::lower::class_field_types(self.db, class)
                .iter()
                .find(|(field, _)| field == member)
        {
            return (
                substitute_params(&crate::impls::interned_ty(field_ty), args),
                Some(MemberResolution::Field {
                    class,
                    field: member.clone(),
                }),
            );
        }
        if let InferTy::Class(qtn, args, _) = resolved.kind()
            && let Some(crate::package_interface::ExportedType::Class { fields, .. }) =
                crate::package_interface::mounted_type_row(self.db, qtn)
            && let Some((_, field_ty, _)) = fields.iter().find(|(field, ..)| field == member)
        {
            return (
                substitute_params(&Ty::from_plain(field_ty), args),
                Some(MemberResolution::ExternalField {
                    class: qtn.clone(),
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
            match candidate.source {
                crate::method_resolution::MethodCandidateSource::Source { method, class } => {
                    if self.reject_selfless_inherent_method(method, member, at) {
                        return (Ty::error(), None);
                    }
                    if self.member_probe_depth == 0 {
                        self.report_bare_output_format_reference(at, class, method);
                    }
                    let signature = function_signature(self.db, method);
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
                    self.register_call_bounds(method, &instantiation, at);
                    return (
                        bind_receiver(function_value_ty(signature, &instantiation)),
                        Some(MemberResolution::BoundMethod {
                            class,
                            func: method,
                        }),
                    );
                }
                crate::method_resolution::MethodCandidateSource::External(function) => {
                    let external = function
                        .external
                        .clone()
                        .expect("mounted method carries an external descriptor");
                    let mut instantiation = candidate.class_args;
                    instantiation.extend(
                        function
                            .generic_params
                            .iter()
                            .map(|param| self.fresh_generic_arg(param)),
                    );
                    self.register_external_call_bounds(&external, &instantiation, at);
                    let ty = crate::method_resolution::instantiate_external_signature(
                        &function,
                        &instantiation,
                    );
                    return (
                        if external.takes_self {
                            bind_receiver(ty)
                        } else {
                            ty
                        },
                        Some(MemberResolution::External(external)),
                    );
                }
            }
        }
        match crate::method_resolution::lookup_interface_member(
            self.db,
            &self.facts,
            &resolved,
            member,
        ) {
            crate::method_resolution::InterfaceMemberLookup::Found(interface_member) => {
                if self.reject_selfless_instance_member(&interface_member, member, at) {
                    return (Ty::error(), None);
                }
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
                    Ty::intern(InferTy::Unknown {
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
    ) -> (Ty, Option<MemberResolution<'db, Ty>>) {
        match crate::method_resolution::lookup_union_member(
            self.db,
            &self.facts,
            union_ty,
            members,
            member,
        ) {
            crate::method_resolution::UnionMemberLookup::Found(interface_member) => {
                if self.reject_selfless_instance_member(&interface_member, member, at) {
                    return (Ty::error(), None);
                }
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
    ) -> Option<MemberResolution<'db, Ty>> {
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
            MemberDeclarer::ImplMethod {
                block,
                func,
                frame_type_args,
                from_interface_default,
            } => Some(MemberResolution::InterfaceConcreteMethod {
                impl_block: *block,
                func: *func,
                frame_type_args: frame_type_args.clone(),
                from_interface_default: *from_interface_default,
            }),
            MemberDeclarer::ImplField { .. } => None,
            MemberDeclarer::ExternalMethod(callable) => {
                Some(MemberResolution::External(callable.clone()))
            }
            MemberDeclarer::ExternalVirtualField {
                interface,
                realized,
                field_index,
            } => Some(MemberResolution::ExternalInterfaceVirtualField {
                interface: interface.clone(),
                view: realized.existential(),
                field_index: *field_index,
                field: member.clone(),
            }),
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
    fn qualified_interface_display(&self, iface: &baml_type::interned::InferInterface) -> String {
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
                .map(|arg| rendered_plain(arg).render_user_facing())
                .collect::<Vec<_>>()
                .join(", ");
            format!("{base}<{args}>")
        }
    }

    /// An interface member in VALUE position: no turbofish, so a default
    /// method's own generics instantiate fresh; methods bind their
    /// receiver. Shared by field access and the `default.` value road.
    /// Whether a resolved interface METHOD takes a `self` receiver. A
    /// `self`-less method is an associated function, not an instance member:
    /// it is reachable through the type-qualified spellings only (Rust's
    /// E0599 "associated function, not method" split), because the receiver
    /// spellings' lowering passes the receiver as `self` — which a
    /// `self`-less callee has no slot for.
    fn interface_member_takes_self(
        &self,
        member: &crate::method_resolution::InterfaceMember<'db>,
    ) -> bool {
        use crate::method_resolution::MemberDeclarer;
        match &member.declarer {
            MemberDeclarer::VirtualMethod { method, .. }
            | MemberDeclarer::ImplMethod { func: method, .. } => {
                function_signature(self.db, *method)
                    .params
                    .first()
                    .is_some_and(|param| param.name.as_str() == "self")
            }
            MemberDeclarer::ExternalMethod(callable) => callable.takes_self,
            // Fields have no receiver-vs-associated split.
            MemberDeclarer::VirtualField { .. }
            | MemberDeclarer::ImplField { .. }
            | MemberDeclarer::ExternalVirtualField { .. } => true,
        }
    }

    /// Reject a `self`-less interface method reached through a VALUE receiver
    /// (`f.make(5)`, `f.as<I>.make()`, union receivers): returns `true` and
    /// reports when the member is such a method. See
    /// [`Self::interface_member_takes_self`] for why this cannot be allowed
    /// to slide (the receiver would be passed into the first real parameter).
    /// A class-INHERENT static (`function make(seed: int) -> Widget`) reached
    /// through a value receiver. Rejected for the same reason its interface
    /// twin is: the receiver contributes nothing the call needs, and reading
    /// it as one is what allowed it to be smuggled into the first real
    /// parameter. `Widget.make(..)` is the spelling.
    fn reject_selfless_inherent_method(
        &mut self,
        method: baml_compiler2_hir::loc::FunctionLoc<'db>,
        member_name: &baml_type::Name,
        at: ExprId,
    ) -> bool {
        let takes_self = function_signature(self.db, method)
            .params
            .first()
            .is_some_and(|param| param.name.as_str() == "self");
        if takes_self {
            return false;
        }
        if self.member_probe_depth == 0 {
            // An in-class `implements` method resolves as an ordinary class
            // candidate, so this road — not the member-lookup one — is where a
            // CONCRETE receiver's interface static lands. Name its interface
            // when it has one; `None` is reserved for genuinely inherent
            // statics, whose spelling is the owning type.
            let interface_name = self.declaring_interface_of_method(method);
            self.pending_diags
                .push(PendingDiag::SelflessInstanceMember {
                    expr: at,
                    interface_name,
                    member: member_name.clone(),
                });
        }
        true
    }

    /// The interface a resolved member was declared by, for the diagnostic
    /// that names it. Both METHOD declarers can answer: a symbolic receiver
    /// carries the interface directly, and a concrete one carries the impl's
    /// method, whose interface target is recorded (an adopted default body
    /// is owned by the interface itself). The FIELD declarers cannot occur
    /// here — the only caller has already required `member.is_method`.
    fn member_declaring_interface(
        &self,
        declarer: &crate::method_resolution::MemberDeclarer<'db>,
    ) -> Option<baml_type::Name> {
        use crate::method_resolution::MemberDeclarer;

        match declarer {
            MemberDeclarer::VirtualMethod { interface, .. } => {
                Some(self.interface_short_name(*interface))
            }
            MemberDeclarer::ImplMethod { func, .. } => self.declaring_interface_of_method(*func),
            // A mounted method's descriptor is source-less: it records
            // `takes_self` but not the interface that declared it, so there is
            // no name to give and the message stays the general one.
            MemberDeclarer::ExternalMethod(_) => None,
            // FIELD declarers cannot reach here — the only caller has already
            // required `member.is_method`.
            MemberDeclarer::VirtualField { .. }
            | MemberDeclarer::ImplField { .. }
            | MemberDeclarer::ExternalVirtualField { .. } => None,
        }
    }

    fn interface_short_name(
        &self,
        interface: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    ) -> baml_type::Name {
        baml_compiler2_ppir::item_data::interface_data(self.db, interface)
            .name
            .clone()
    }

    /// The interface a METHOD belongs to, from the method alone: an inherited
    /// default body is owned by the interface, and a method written inside an
    /// `implements` block records that block's interface target. `None` for a
    /// genuinely inherent method, which no interface declares.
    ///
    /// Keyed on the method rather than the resolution because BOTH `self`-less
    /// rejections need the answer and reach it differently — a member lookup
    /// hands back a declarer, while an in-class `implements` method resolves
    /// as an ordinary class candidate with no declarer at all.
    fn declaring_interface_of_method(
        &self,
        func: baml_compiler2_hir::loc::FunctionLoc<'db>,
    ) -> Option<baml_type::Name> {
        if let Some(baml_compiler2_ppir::item_data::MethodOwner::Interface(interface)) =
            baml_compiler2_ppir::item_data::method_owner(self.db, func)
        {
            return Some(self.interface_short_name(interface));
        }
        let target =
            baml_compiler2_ppir::item_data::method_interface_target(self.db, func).as_ref()?;
        let target_ty = self.lower.lower_type_ref_at(
            &target.type_refs,
            target.target,
            crate::lower::TypePosition::ConstraintHead,
        );
        match &target_ty {
            baml_type::LoweringTy::Interface(name, ..) => Some(name.name().clone()),
            _ => None,
        }
    }

    /// A `self`-less member reached through a VALUE receiver. Rejected
    /// whatever the receiver's kind — concrete, existential, or union: the
    /// value contributes nothing the call needs, and reading it as a receiver
    /// is what let it be smuggled into the first real parameter. The
    /// type-qualified spellings are the only roads, so the receiver itself is
    /// not a parameter here.
    fn reject_selfless_instance_member(
        &mut self,
        member: &crate::method_resolution::InterfaceMember<'db>,
        member_name: &baml_type::Name,
        at: ExprId,
    ) -> bool {
        if !member.is_method || self.interface_member_takes_self(member) {
            return false;
        }
        if self.member_probe_depth == 0 {
            let interface_name = self.member_declaring_interface(&member.declarer);
            self.pending_diags
                .push(PendingDiag::SelflessInstanceMember {
                    expr: at,
                    interface_name,
                    member: member_name.clone(),
                });
        }
        true
    }

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
                crate::method_resolution::PendingOwnGenerics::External { function, prefix } => {
                    let mut instantiation = prefix;
                    instantiation.extend(
                        function
                            .generic_params
                            .iter()
                            .map(|param| self.fresh_generic_arg(param)),
                    );
                    bind_receiver(crate::method_resolution::instantiate_external_signature(
                        &function,
                        &instantiation,
                    ))
                }
            };
        }
        // Same rule as the call road: a `self`-less method binds nothing.
        if interface_member.is_method && self.interface_member_takes_self(&interface_member) {
            return bind_receiver(interface_member.ty);
        }
        interface_member.ty
    }

    /// UFCS/value form of an interface-provided method. Unlike ordinary
    /// member values this preserves the explicit `self` parameter.
    fn interface_member_unbound_value(
        &mut self,
        interface_member: crate::method_resolution::InterfaceMember<'db>,
    ) -> Ty {
        if let Some(pending) = interface_member.pending_own {
            return match pending {
                crate::method_resolution::PendingOwnGenerics::Source { method, prefix } => {
                    let signature = function_signature(self.db, method);
                    let mut instantiation = prefix;
                    instantiation.extend(
                        signature.generic_params[instantiation.len()..]
                            .iter()
                            .map(|param| self.fresh_generic_arg(param)),
                    );
                    function_value_ty(signature, &instantiation)
                }
                crate::method_resolution::PendingOwnGenerics::External { function, prefix } => {
                    let mut instantiation = prefix;
                    instantiation.extend(
                        function
                            .generic_params
                            .iter()
                            .map(|param| self.fresh_generic_arg(param)),
                    );
                    crate::method_resolution::instantiate_external_signature(
                        &function,
                        &instantiation,
                    )
                }
            };
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
        while let InferTy::TypeAlias(qtn, _) = resolved.kind() {
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

    /// The sole concrete function type at a callback root: `ty` itself, or
    /// the unique function member nested through unions and type aliases.
    ///
    /// A lambda literal can use that signature even when the other union arms
    /// are non-callable values (`string | Callback`, for example). Multiple
    /// function arms remain ambiguous and provide no context. The traversal is
    /// fuel-bounded because recursive aliases are valid types but cannot offer
    /// a finite unique callback shape.
    fn callback_root_fn(&mut self, ty: &Ty) -> Option<Ty> {
        fn collect(
            this: &mut InferenceContext<'_>,
            ty: &Ty,
            callback: &mut Option<Ty>,
            fuel: u8,
        ) -> bool {
            if fuel == 0 {
                return false;
            }
            let expanded = this.expand_alias_ty(ty);
            match expanded.kind() {
                InferTy::Function { .. } => {
                    if callback.is_some() {
                        return false;
                    }
                    *callback = Some(expanded);
                    true
                }
                InferTy::Union(members, _) => {
                    let members = members.to_vec();
                    members
                        .iter()
                        .all(|member| collect(this, member, callback, fuel.saturating_sub(1)))
                }
                InferTy::TypeAlias(..) => false,
                _ => true,
            }
        }

        let mut callback = None;
        collect(self, ty, &mut callback, 16)
            .then_some(callback)
            .flatten()
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
            InferTy::List(element, _) => Some(element.clone()),
            InferTy::Union(members, _) => {
                let mut lists = members.iter().filter_map(|member| match member.kind() {
                    InferTy::List(element, _) => Some(element.clone()),
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
            InferTy::Map { key, value, .. } => Some((key.clone(), value.clone())),
            InferTy::Union(members, _) => {
                let mut maps = members.iter().filter_map(|member| match member.kind() {
                    InferTy::Map { key, value, .. } => Some((key.clone(), value.clone())),
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

    fn qualified_path_root_is_top_level_let(&self, segments: &[baml_type::Name]) -> bool {
        segments.len() > 1
            && segments.first().is_some_and(|root| {
                matches!(
                    self.lower.resolve_value(std::slice::from_ref(root)),
                    Some(Definition::Let(_))
                )
            })
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
    fn lower_body_annotation(&mut self, type_ref: BodyTypeRefId) -> Ty {
        if let Some(cached) = self.annotation_cache.get(&type_ref) {
            return cached.clone();
        }
        let (lowered, diagnostics) =
            self.lower_body_type_ref_at(type_ref, crate::lower::TypePosition::Existential);
        self.queue_body_lowering_diagnostics(diagnostics);
        // Written-type well-formedness (rustc's wfcheck at body
        // annotations): generic arguments must satisfy their heads'
        // declared bounds. Hole-carrying annotations skip - their holes
        // solve first and the instantiation sites judge them.
        if !lowered.contains_hole() {
            let env = self.wf_scope_env.get_or_init(|| match self.body_owner {
                Some(function) => crate::lower::function_generic_bounds(self.db, function)
                    .into_iter()
                    .collect(),
                None => rustc_hash::FxHashMap::default(),
            });
            for error in crate::interfaces::type_generic_bound_errors(self.db, env, &lowered) {
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
    fn instantiate_holes(&mut self, ty: &baml_type::LoweringTy, at: HoleAnchor) -> Ty {
        self.ingest_lowered(ty, IngestHoles::Anchored(at))
    }

    /// The lowering→inference ingestion: intern the closed structure, and
    /// let the policy decide what each `_` hole becomes. Both policies mint
    /// a fresh table variable — they differ in whether the hole is legal
    /// (annotation positions record it in `hole_vars` for unsolved-hole
    /// reporting) or immediately diagnosed (expression positions, an
    /// unconditional E0147).
    fn ingest_lowered(&mut self, ty: &baml_type::LoweringTy, policy: IngestHoles) -> Ty {
        if let baml_type::LoweringTy::Infer { .. } = ty {
            return match policy {
                IngestHoles::Anchored(at) => {
                    let var_ty = self.table.new_var_ty();
                    if let InferTy::InferVar { var, .. } = var_ty.kind() {
                        self.hole_vars.push((*var, at));
                    }
                    var_ty
                }
                IngestHoles::ExprPosition(expr) => {
                    self.pending_diags
                        .push(PendingDiag::ExprPositionHole { expr });
                    self.table.new_var_ty()
                }
            };
        }
        match baml_type::Ty::try_from(ty) {
            // No holes below: the interned ingestion is total.
            Ok(closed) => Ty::from_plain(&closed),
            Err(_) => self.ingest_lowered_slow(ty, policy),
        }
    }

    /// The hole-carrying slow path of [`Self::ingest_lowered`]: rebuild the
    /// recursive shapes interned with each child re-ingested. Leaves never
    /// reach here (a leaf is closed, so the fast path takes it).
    fn ingest_lowered_slow(&mut self, ty: &baml_type::LoweringTy, policy: IngestHoles) -> Ty {
        use baml_type::LoweringTy as LoweringTyShape;
        let kind = match ty {
            LoweringTyShape::List(inner, attr) => {
                InferTy::List(self.ingest_lowered(inner, policy), attr.clone())
            }
            LoweringTyShape::Map { key, value, attr } => InferTy::Map {
                key: self.ingest_lowered(key, policy),
                value: self.ingest_lowered(value, policy),
                attr: attr.clone(),
            },
            LoweringTyShape::Union(members, attr) => InferTy::Union(
                members
                    .iter()
                    .map(|member| self.ingest_lowered(member, policy))
                    .collect(),
                attr.clone(),
            ),
            LoweringTyShape::Class(name, args, attr) => InferTy::Class(
                name.clone(),
                args.iter()
                    .map(|arg| self.ingest_lowered(arg, policy))
                    .collect(),
                attr.clone(),
            ),
            LoweringTyShape::Interface(name, args, pins, attr) => InferTy::Interface(
                name.clone(),
                args.iter()
                    .map(|arg| self.ingest_lowered(arg, policy))
                    .collect(),
                pins.iter()
                    .map(|(name, ty)| (name.clone(), self.ingest_lowered(ty, policy)))
                    .collect(),
                attr.clone(),
            ),
            LoweringTyShape::Function {
                params,
                ret,
                throws,
                attr,
            } => InferTy::Function {
                params: params
                    .iter()
                    .map(|param| baml_type::interned::InferFunctionParamTy {
                        name: param.name.clone(),
                        ty: self.ingest_lowered(&param.ty, policy),
                        mode: param.mode,
                    })
                    .collect(),
                ret: self.ingest_lowered(ret, policy),
                throws: self.ingest_lowered(throws, policy),
                attr: attr.clone(),
            },
            LoweringTyShape::Future(value, error, attr) => InferTy::Future(
                self.ingest_lowered(value, policy),
                self.ingest_lowered(error, policy),
                attr.clone(),
            ),
            LoweringTyShape::AssociatedTypeProjection {
                base,
                interface,
                member,
                attr,
            } => InferTy::AssociatedTypeProjection {
                base: self.ingest_lowered(base, policy),
                interface: baml_type::interned::InferInterface::new(
                    interface.name.clone(),
                    interface
                        .generics
                        .iter()
                        .map(|arg| self.ingest_lowered(arg, policy))
                        .collect(),
                    interface
                        .associated_types
                        .iter()
                        .map(|(name, ty)| (name.clone(), self.ingest_lowered(ty, policy)))
                        .collect(),
                ),
                member: member.clone(),
                attr: attr.clone(),
            },
            _ => unreachable!("closed leaves take the ingest fast path"),
        };
        Ty::intern(kind)
    }

    /// [`Self::instantiate_holes`] for EXPRESSION-position type arguments
    /// (turbofish, generic-apply values, upcast targets): a written `_`
    /// there is a hard E0147 outright - TIR's rule; expression positions
    /// have no annotation slot for inference to fill (`is Show<_>` /
    /// `.as<Show<_>>` are ascriptions with no local source). The hole
    /// still instantiates as a fresh var so inference proceeds for
    /// RECOVERY, but the diagnostic is unconditional and immediate,
    /// never dependent on whether the var happens to solve.
    fn reject_expr_position_holes(&mut self, ty: &baml_type::LoweringTy, at: ExprId) -> Ty {
        self.ingest_lowered(ty, IngestHoles::ExprPosition(at))
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
            if matches!(finalized.kind(), InferTy::Never { .. }) {
                continue;
            }
            let resolved = self.table.resolve_completely(&finalized);
            let canonical = self.matrix_scrut(&resolved);
            match canonical.kind() {
                InferTy::Union(members, _) => {
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
                // CONTEXT - `baml.errors.Context` (the AST field's
                // "stack trace" name understates it; TIR resolves the
                // class). Lookup-gated, fail-safe to Error.
                let context_ty = match self.facts.definition_of(&baml_type::TypeName::new(
                    baml_type::Name::new("baml"),
                    vec![baml_type::Name::new("errors")],
                    baml_type::Name::new("Context"),
                )) {
                    Some(baml_compiler2_hir::contributions::Definition::Class(_)) => {
                        Ty::intern(InferTy::Class(
                            baml_type::TypeName::new(
                                baml_type::Name::new("baml"),
                                vec![baml_type::Name::new("errors")],
                                baml_type::Name::new("Context"),
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
                        &Ty::intern(InferTy::Unknown {
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
                    if self.provable_subtype(fact, &claim) {
                        may.push(fact.clone());
                        definite.push(fact.clone());
                    } else if self.provable_subtype(&claim, fact) {
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
                    if matches!(claim.kind(), InferTy::Unknown { .. }) {
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
            InferTy::Class(qtn, _, _) if qtn.is_panic_type() => Some(expanded.clone()),
            InferTy::Union(members, _) => {
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

    /// The non-`baml.panics.*` component of a thrown type - what a `throw`
    /// actually contributes to the channel.
    ///
    /// The complement of [`Self::panic_subset`]: that one keeps the half a
    /// catch arm may always trap, this one keeps the half a `throws` clause
    /// must account for. `None` means the type was panics all the way down.
    fn non_panic_subset(&mut self, ty: &Ty) -> Option<Ty> {
        let expanded = self.expand_alias_ty(ty);
        match expanded.kind() {
            InferTy::Class(qtn, _, _) if qtn.is_panic_type() => None,
            InferTy::Union(members, _) => {
                let members = members.to_vec();
                let rest: Vec<Ty> = members
                    .iter()
                    .filter_map(|member| self.non_panic_subset(member))
                    .collect();
                if rest.is_empty() {
                    None
                } else {
                    Some(self.union_of(&rest))
                }
            }
            _ => Some(expanded.clone()),
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
            InferTy::Null { .. } | InferTy::Unknown { .. } => true,
            InferTy::Union(members, _) => members
                .iter()
                .any(|member| matches!(member.kind(), InferTy::Null { .. })),
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
                        InferTy::Null { .. } => true,
                        InferTy::Union(members, _) => members
                            .iter()
                            .any(|member| matches!(member.kind(), InferTy::Null { .. })),
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
        if matches!(ty.kind(), InferTy::Never { .. }) || ty.has_error() {
            return;
        }
        // Panics are RAISED, not thrown: catchable at runtime, but never part
        // of a `throws` contract. `throw baml.panics.X` therefore contributes
        // nothing to the channel - it re-raises, exactly as `baml.sys.panic`
        // does, and the emitted `ThrowIfPanic` guard already carries the
        // matching runtime behaviour past wildcard arms. A throw whose type
        // mixes panics with ordinary errors contributes only the ordinary
        // half. Untouched when no panic is present, so every other throw
        // records exactly the type it always did.
        let ty = if self.panic_subset(ty).is_some() {
            match self.non_panic_subset(ty) {
                Some(rest) => rest,
                None => return,
            }
        } else {
            ty.clone()
        };
        // Thrown literals KEEP their literal types (no widening): catch
        // arms match on literal error codes, and the canonical union at
        // the channel is the generation site. The RUNTIME boundary
        // widens (the provider's conversion): `reflect.signature` on a
        // `throw "negative"` lambda reconstructs `string`.
        let contribution = ty;
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
    fn write_member_resolution(&mut self, expr: ExprId, resolution: MemberResolution<'db, Ty>) {
        self.result.member_resolutions.insert(expr, resolution);
    }

    /// Records a call's solved instantiation vector (raw; ground after
    /// writeback) with the owner-prefix split. The plan's two halves have
    /// two writers keyed by the same call id - type args at the
    /// instantiation site, bindings in `check_call_args` - the r-a shape
    /// of separate tables written where each decision is made, co-located
    /// in one struct.
    fn write_call_type_args(
        &mut self,
        call: ExprId,
        type_args: &[Ty],
        own_offset: usize,
    ) -> Vec<Ty> {
        let explicit = self.type_refs.expr_type_args.contains_key(&call);
        if !self.result.call_plans.get(&call).is_some_and(|plan| {
            plan.slots
                .iter()
                .any(|slot| matches!(slot, CallTypeArgPlan::Runtime { .. }))
        }) {
            let type_args = type_args.to_vec();
            let plan = self.result.call_plans.entry(call).or_default();
            plan.type_args.clone_from(&type_args);
            plan.own_offset = own_offset;
            plan.explicit = explicit;
            return type_args;
        }
        let runtime_params = self.runtime_call_params(call);
        let mut type_args = type_args.to_vec();
        let runtime_occurrences: Vec<(baml_type::ParamTy, Ty)> = self
            .result
            .call_plans
            .get(&call)
            .into_iter()
            .flat_map(|plan| &plan.slots)
            .filter_map(|slot| match slot {
                CallTypeArgPlan::Runtime {
                    occurrence_ty,
                    parameter,
                    ..
                } => Some((
                    parameter.clone(),
                    substitute_static_call_params(occurrence_ty, &type_args, &runtime_params),
                )),
                CallTypeArgPlan::Static { .. } => None,
            })
            .collect();
        for (parameter, occurrence_ty) in &runtime_occurrences {
            if let Some(slot) = type_args.get_mut(parameter.index() as usize) {
                *slot = occurrence_ty.clone();
            }
        }
        let plan = self.result.call_plans.entry(call).or_default();
        for slot in &mut plan.slots {
            if let CallTypeArgPlan::Runtime {
                occurrence_ty,
                parameter,
                ..
            } = slot
                && let Some((_, specialized)) = runtime_occurrences
                    .iter()
                    .find(|(candidate, _)| candidate == parameter)
            {
                *occurrence_ty = specialized.clone();
            }
        }
        plan.type_args.clone_from(&type_args);
        plan.own_offset = own_offset;
        plan.explicit = explicit;
        type_args
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
    ) -> (Ty, Vec<ResolvedPathSegment<'db, Ty>>) {
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

    fn write_resolved_path(&mut self, expr: ExprId, steps: Vec<ResolvedPathSegment<'db, Ty>>) {
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

    fn untyped_empty_container_ty(
        &mut self,
        expr: ExprId,
        make_container: impl FnOnce(Ty) -> Ty,
    ) -> Ty {
        let slot = self.table.new_var_ty_of(unify::VarPolicy::ContainerSlot);
        let InferTy::InferVar { var, .. } = slot.kind() else {
            unreachable!("a fresh establishment variable must be an inference type");
        };
        let var = *var;
        let containing_type = make_container(slot);
        self.infer_var_origin_order.push(var);
        self.infer_var_origins.insert(
            var,
            InferVarOrigin::TypeMustBeKnown {
                location: crate::diagnostics::DiagnosticLocation::Expr(expr),
                containing_type: containing_type.clone(),
            },
        );
        containing_type
    }

    fn untyped_lambda_parameter_ty(
        &mut self,
        lambda: ExprId,
        parameter_index: usize,
        name: baml_type::Name,
    ) -> Ty {
        let ty = self.table.new_var_ty_of(unify::VarPolicy::LambdaParam);
        let InferTy::InferVar { var, .. } = ty.kind() else {
            unreachable!("a fresh lambda parameter variable must be an inference type");
        };
        let var = *var;
        self.infer_var_origin_order.push(var);
        self.infer_var_origins.insert(
            var,
            InferVarOrigin::LambdaParameter {
                lambda,
                parameter_index,
                name,
            },
        );
        ty
    }

    fn take_unresolved_infer_diagnostics(
        &mut self,
    ) -> Vec<(
        crate::diagnostics::DiagnosticLocation,
        crate::diagnostics::TirTypeError,
    )> {
        let mut diagnostics = Vec::new();
        for var in std::mem::take(&mut self.infer_var_origin_order) {
            let Some(origin) = self.infer_var_origins.remove(&var) else {
                continue;
            };
            let Some(root) = self.table.unsolved_root_var(var) else {
                continue;
            };
            let (location, error) = match origin {
                InferVarOrigin::TypeMustBeKnown {
                    location,
                    containing_type,
                } => {
                    let full_type = self.table.resolve_completely(&containing_type);
                    if full_type.has_error() {
                        continue;
                    }
                    (
                        location,
                        crate::diagnostics::TirTypeError::TypeMustBeKnown {
                            full_type: rendered_plain(&full_type),
                        },
                    )
                }
                InferVarOrigin::LambdaParameter {
                    lambda,
                    parameter_index,
                    name,
                } => (
                    crate::diagnostics::DiagnosticLocation::LambdaParameter(
                        lambda,
                        parameter_index,
                    ),
                    crate::diagnostics::TirTypeError::CannotInferLambdaParamType {
                        param_name: name,
                    },
                ),
            };
            if !self.diagnosed_infer_vars.insert(root) {
                continue;
            }
            diagnostics.push((location, error));
        }
        diagnostics
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
            if !self.cached_subtype(&judged_actual, &judged_expected) {
                self.result
                    .type_mismatches
                    .entry(anchor)
                    .or_insert((expected, actual));
            }
        }
        // BAML's only defaulting rule: an unconstrained EFFECT is `never`
        // (a value variable erases to Error instead - ruling 2).
        self.table.default_unsolved_effects_to_never();
        let unresolved_infer_diagnostics = self.take_unresolved_infer_diagnostics();
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
                    .map(|(_, ty)| self.finalize_ty(ty).into_ty())
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
        // is extraneous, anchored at the body root (the clause itself
        // lives in the signature store). An imprecise `unknown` contract
        // is an error; other extraneous members remain warnings.
        if let Some(declared) = self.declared_throws.clone()
            && !self.declared_throws_open
            && !declared.has_error()
            && let Some(root) = self.body_root
        {
            let is_open_contract = crate::lower::is_open_throws_contract(self.db, &declared);
            // Coverage compares WIDENED facts (TIR's fact grain: a thrown
            // `"boom"` covers a declared `string`) while the report keeps
            // the declared spelling.
            // An open union needs its preserved written surface: finalizing
            // `unknown | SomeError` canonicalizes it to semantic `unknown`
            // and erases `SomeError` before coverage can report it. Other
            // contracts still need finalization so projections and solved
            // variables compare against the effective facts correctly.
            let declared_for_coverage = if is_open_contract {
                rendered_plain(&declared)
            } else {
                self.plain_finalized(&declared)
            };
            let declared_facts =
                crate::package_interface::flatten_ty_to_facts(&declared_for_coverage);
            let effective: std::collections::BTreeSet<baml_type::Ty> = self.throws_channels[0]
                .clone()
                .iter()
                .flat_map(|(_, ty)| {
                    crate::throw_facts::flatten_declared_ty_to_facts(&self.plain_finalized(ty))
                })
                .collect();
            let extraneous: Vec<baml_type::Ty> = declared_facts
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
                .cloned()
                .collect();
            if is_open_contract {
                let throws_unknown = effective
                    .iter()
                    .any(|ty| crate::lower::is_open_throws_contract(self.db, &Ty::from_plain(ty)));
                if !throws_unknown {
                    let inferred_types = effective
                        .iter()
                        .map(baml_type::Ty::render_user_facing)
                        .collect();
                    self.pending_diags
                        .push(PendingDiag::ImpreciseUnknownThrows {
                            at: root,
                            inferred_types,
                        });
                } else {
                    // The `unknown` member is meaningful, but any other
                    // uncovered members remain ordinary E0097 warnings. Do
                    // not report an alias that expands to `unknown` as an
                    // extraneous member merely because coverage compares the
                    // alias's written surface with the resolved thrown type.
                    let mut extra_types: Vec<String> = extraneous
                        .iter()
                        .filter(|ty| {
                            !crate::lower::is_open_throws_contract(self.db, &Ty::from_plain(ty))
                        })
                        .map(baml_type::Ty::render_user_facing)
                        .collect();
                    extra_types.sort();
                    if !extra_types.is_empty() {
                        self.pending_diags.push(PendingDiag::ExtraneousThrows {
                            at: root,
                            extra_types,
                        });
                    }
                }
            } else if !extraneous.is_empty() {
                let mut extra_types: Vec<String> = extraneous
                    .iter()
                    .map(baml_type::Ty::render_user_facing)
                    .collect();
                extra_types.sort();
                if !extra_types.is_empty() {
                    self.pending_diags.push(PendingDiag::ExtraneousThrows {
                        at: root,
                        extra_types,
                    });
                }
            }
        }
        let mut result = std::mem::take(&mut self.result);
        result.throws = throws;
        for ty in result
            .type_of_expr
            .values_mut()
            .chain(result.type_of_pat.values_mut())
        {
            *ty = self.finalize_ty(ty).into_ty();
        }
        // Truthiness decisions deferred past the fixpoint (B-1563): a
        // condition still carrying an inference variable at check time
        // decides here, on its FINAL type, so `if (identity(0))` records
        // the same coercion `if (0)` does.
        self.decide_deferred_conditions(&mut result);
        // Provisional checks re-judge now that their expectations solved:
        // a definite failure joins the mismatch table (first writer per
        // expr wins - a direct mismatch is the better message).
        for (expr, expected, actual) in std::mem::take(&mut self.provisional_checks) {
            let expected = self.finalize_ty(&expected).into_ty();
            let actual = self.finalize_ty(&actual).into_ty();
            if expected.has_error() || actual.has_error() || self.cached_subtype(&actual, &expected)
            {
                continue;
            }
            result
                .type_mismatches
                .entry(expr)
                .or_insert((expected, actual));
        }
        for (expected, actual) in result.type_mismatches.values_mut() {
            *expected = self.finalize_ty(expected).into_ty();
            *actual = self.finalize_ty(actual).into_ty();
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
            for (location, error) in unresolved_infer_diagnostics {
                diags.push(TirDiagnostic {
                    error,
                    severity: DiagnosticSeverity::Error,
                    primary: location,
                    related: Vec::new(),
                });
            }
            for (&expr, (expected, actual)) in &result.type_mismatches {
                // rustc's tainted_by_errors discipline: a mismatch whose
                // operand IS the error sentinel is a CASCADE of a reported
                // failure. Error LEAVES inside a structural head are a
                // different case - `list<!e>` against `map<...>` is a real
                // head mismatch (unsolved container vars finalize their
                // elements to the sentinel without erasing the finding).
                let tainted = |ty: &Ty| {
                    ty.has_error()
                        && matches!(ty.kind(), InferTy::Error { .. } | InferTy::Unknown { .. })
                };
                if tainted(expected) || tainted(actual) {
                    continue;
                }
                // A check can fail MID-INFERENCE on still-open variables
                // that later resolution satisfies; only a mismatch that
                // HOLDS in the finalized world reports.
                if self.cached_subtype(actual, expected) {
                    continue;
                }
                // The for-desugar's iterability failure reads as its own
                // message (TIR's NotIterable), not a raw interface mismatch.
                let error = match expected.kind() {
                    InferTy::Interface(qtn, _, _, _)
                        if qtn.package().as_str() == "baml"
                            && qtn.namespace().len() == 1
                            && qtn.namespace()[0].as_str() == "iter"
                            && qtn.name().as_str() == "Iterable" =>
                    {
                        TirTypeError::NotIterable {
                            ty: self.materialize_ty(actual),
                        }
                    }
                    // BEP-044 wf3 #G18: a value that ALMOST implements an
                    // expected interface through a blanket `implements`
                    // rule - the implementor shape matches but a generic
                    // bound fails - names the unsatisfied bound (rustc's
                    // obligation-cause refinement of a fulfillment error)
                    // rather than a bare mismatch.
                    InferTy::Interface(..) => {
                        match self.first_failing_blanket_bound(actual, expected) {
                            Some(bound) => TirTypeError::BlanketBoundNotSatisfied {
                                value_type: self.materialize_ty(actual),
                                bound,
                            },
                            None => TirTypeError::TypeMismatch {
                                expected: self.materialize_ty(expected),
                                got: self.materialize_ty(actual),
                            },
                        }
                    }
                    _ => TirTypeError::TypeMismatch {
                        expected: self.materialize_ty(expected),
                        got: self.materialize_ty(actual),
                    },
                };
                diags.push(TirDiagnostic {
                    error,
                    severity: DiagnosticSeverity::Error,
                    primary: DiagnosticLocation::Expr(expr),
                    related: Vec::new(),
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
                    HoleAnchor::TypeRef(type_ref) => DiagnosticLocation::BodyTypeRef(type_ref),
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
                            scrutinee_type: self.plain_finalized(&scrutinee),
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
                    } => (
                        crate::diagnostics::removed_reflect_spelling(&name)
                            .unwrap_or(TirTypeError::UnresolvedType { name, suggestions }),
                        expr,
                    ),
                    PendingDiag::PositionalAfterNamed { expr } => {
                        (TirTypeError::PositionalArgumentAfterNamed, expr)
                    }
                    PendingDiag::DuplicateNamedArg { expr, name } => {
                        (TirTypeError::DuplicateNamedArgument { name }, expr)
                    }
                    PendingDiag::UnresolvedName { expr, name } => (
                        crate::diagnostics::removed_reflect_spelling(&name)
                            .unwrap_or(TirTypeError::UnresolvedName { name }),
                        expr,
                    ),
                    PendingDiag::TopLevelLetCycle { expr } => (TirTypeError::CannotInferType, expr),
                    PendingDiag::UnresolvedMember { expr, base, member } => (
                        TirTypeError::UnresolvedMember {
                            base_type: self.plain_finalized(&base),
                            member,
                        },
                        expr,
                    ),
                    PendingDiag::AnnotWf { type_ref, error } => {
                        diags.push(TirDiagnostic {
                            error,
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::BodyTypeRef(type_ref),
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
                        let receiver =
                            baml_type::Name::new(self.plain_finalized(&base).render_user_facing());
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
                                self.plain_finalized(&base).render_user_facing(),
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
                            union: self.plain_finalized(&base),
                            member,
                        },
                        expr,
                    ),
                    PendingDiag::BoundedArgNotConcrete { expr, arg, bound } => (
                        TirTypeError::BoundedTypeArgNotConcrete {
                            arg: self.plain_finalized(&arg),
                            bound: Box::new([self.materialize_interface(&bound)]),
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
                    PendingDiag::QualifierNotInterface { expr, target } => (
                        TirTypeError::InvalidInterfaceUpcastTarget {
                            target: self.plain_finalized(&target),
                        },
                        expr,
                    ),
                    PendingDiag::QualifierNotImplemented {
                        expr,
                        value,
                        interface,
                    } => (
                        TirTypeError::TypeDoesNotImplementInterface {
                            value_type: self.plain_finalized(&value),
                            interface: self.plain_finalized(&interface),
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
                    PendingDiag::GenericFunctionValueNotSpecialized {
                        expr,
                        name,
                        reference,
                        inference_evidence,
                        specialization_args,
                        unconditional,
                        had_expected_type,
                        generic_params,
                        binding_name,
                        function_shape,
                        annotation_ty,
                        specialization_example_is_safe,
                        specialization_syntax_available,
                    } => {
                        let has_unresolved_user_arg = inference_evidence
                            .iter()
                            .any(|arg| self.finalize_ty(arg).has_error());
                        if !unconditional && !has_unresolved_user_arg {
                            continue;
                        }
                        let specialization_example =
                            if specialization_example_is_safe && specialization_syntax_available {
                                let mut args = Vec::with_capacity(generic_params.len());
                                if let Some(specialization_args) = specialization_args {
                                    for arg in &specialization_args {
                                        let finalized = self.finalize_ty(arg);
                                        args.push(
                                            rendered_plain(&diagnostic_example_ty(&finalized))
                                                .to_string(),
                                        );
                                    }
                                } else {
                                    args.resize(generic_params.len(), "int".to_string());
                                }
                                Some(args.join(", "))
                            } else {
                                None
                            };
                        let annotation_example = annotation_ty.map(|ty| {
                            let finalized = self.finalize_ty(&ty);
                            rendered_plain(&diagnostic_example_ty(&finalized)).to_string()
                        });
                        (
                            TirTypeError::GenericFunctionValueNotSpecialized {
                                name,
                                reference,
                                had_expected_type,
                                generic_params,
                                binding_name,
                                function_shape,
                                annotation_example,
                                specialization_example,
                                specialization_syntax_available,
                            },
                            expr,
                        )
                    }
                    PendingDiag::ComputedGenericArgumentRequiresUnreflect { expr, name } => (
                        TirTypeError::ComputedGenericArgumentRequiresUnreflect { name },
                        expr,
                    ),
                    PendingDiag::MountedPackageCallUnsupported { expr, path } => {
                        (TirTypeError::MountedPackageCallUnsupported { path }, expr)
                    }
                    PendingDiag::RuntimeTypeArgumentOnStreamingCall { expr, callee } => (
                        TirTypeError::RuntimeTypeArgumentOnStreamingCall {
                            callee_name: callee,
                        },
                        expr,
                    ),
                    PendingDiag::RuntimeTypeArgumentOnIndirectCall { expr } => {
                        (TirTypeError::RuntimeTypeArgumentOnIndirectCall, expr)
                    }
                    PendingDiag::RuntimeTypeMustBeNamed {
                        carrier,
                        enclosing,
                        escape,
                    } => {
                        diags.push(TirDiagnostic {
                            error: TirTypeError::RuntimeTypeMustBeNamed { escape },
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::UnreflectArg { carrier, enclosing },
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::CannotConstructReflectionKind { expr, class_name } => (
                        TirTypeError::CannotConstructReflectionKind { class_name },
                        expr,
                    ),
                    PendingDiag::CannotConstructBuiltinCompanion {
                        expr,
                        class_name,
                        companion,
                    } => (
                        TirTypeError::CannotConstructBuiltinCompanion {
                            class_name,
                            companion,
                        },
                        expr,
                    ),
                    PendingDiag::NotCallable { expr, ty } => (
                        TirTypeError::NotCallable {
                            ty: self.plain_finalized(&ty),
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
                        let lhs = self.plain_finalized(&lhs);
                        let rhs = rhs.map(|ty| self.plain_finalized(&ty));
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
                            ty: self.plain_finalized(&ty),
                        },
                        expr,
                    ),
                    PendingDiag::BareOutputFormatReference { expr } => {
                        (TirTypeError::OutputFormatNotCalled, expr)
                    }
                    PendingDiag::ConditionAlwaysConst {
                        expr,
                        ty,
                        always_true,
                    } => {
                        diags.push(TirDiagnostic {
                            error: TirTypeError::ConditionAlwaysConstant {
                                ty: self.plain_finalized(&ty),
                                always_true,
                            },
                            severity: DiagnosticSeverity::Warning,
                            primary: DiagnosticLocation::Expr(expr),
                            related: Vec::new(),
                        });
                        continue;
                    }
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
                    PendingDiag::SelflessInstanceMember {
                        expr,
                        interface_name,
                        member,
                    } => (
                        TirTypeError::SelflessInstanceMember {
                            interface_name,
                            method_name: member,
                        },
                        expr,
                    ),
                    PendingDiag::ItemProjectionSelfSlot {
                        expr,
                        var,
                        interface,
                        member,
                        takes_self,
                        value_position,
                    } => {
                        let slot = self.finalize_ty(&var);
                        // `never` names no implementor (it satisfies any bound
                        // vacuously by subtyping, which is exactly why it must
                        // be caught here), and a LITERAL type is not an
                        // implementor either — impls attach to its base, so a
                        // `Self`-returning method would produce a base-typed
                        // value inhabiting the literal type.
                        if matches!(slot.kind(), InferTy::Never { .. } | InferTy::Literal(..)) {
                            diags.push(TirDiagnostic {
                                error: TirTypeError::SelflessMethodNeedsConcreteSelf {
                                    interface_name: baml_type::Name::new(
                                        self.qualified_interface_display(&interface),
                                    ),
                                    method_name: member,
                                    self_ty: self.materialize_ty(&slot),
                                },
                                severity: DiagnosticSeverity::Error,
                                primary: DiagnosticLocation::Expr(expr),
                                related: Vec::new(),
                            });
                            continue;
                        }
                        if !matches!(slot.kind(), InferTy::Interface(..) | InferTy::Union(..)) {
                            continue;
                        }
                        if !takes_self {
                            (
                                TirTypeError::SelflessMethodNeedsConcreteSelf {
                                    interface_name: baml_type::Name::new(
                                        self.qualified_interface_display(&interface),
                                    ),
                                    method_name: member,
                                    self_ty: self.materialize_ty(&slot),
                                },
                                expr,
                            )
                        } else if let Some(position) =
                            crate::method_resolution::declared_method_self_restriction(
                                self.db,
                                &self.facts,
                                &interface,
                                &member,
                            )
                        {
                            (
                                TirTypeError::InvalidSelfCallThroughInterface {
                                    interface_name: baml_type::Name::new(
                                        self.qualified_interface_display(&interface),
                                    ),
                                    method_name: member,
                                    position,
                                },
                                expr,
                            )
                        } else if value_position {
                            (
                                TirTypeError::ErasedSelfMethodValue {
                                    interface_name: baml_type::Name::new(
                                        self.qualified_interface_display(&interface),
                                    ),
                                    method_name: member,
                                    self_ty: self.materialize_ty(&slot),
                                },
                                expr,
                            )
                        } else {
                            continue;
                        }
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
                        let got = self.plain_finalized(&got);
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
                                expected_input: self.plain_finalized(&expected_input),
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
                    PendingDiag::ImpreciseUnknownThrows { at, inferred_types } => {
                        diags.push(TirDiagnostic {
                            error: TirTypeError::ImpreciseUnknownThrows { inferred_types },
                            severity: DiagnosticSeverity::Error,
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
                    PendingDiag::ReturnTypeMismatch {
                        stmt,
                        expr,
                        expected,
                        actual,
                    } => {
                        let expected = self.finalize_ty(&expected);
                        let actual = self.finalize_ty(&actual);
                        if expected.has_error()
                            || actual.has_error()
                            || self.cached_subtype(&actual, &expected)
                        {
                            continue;
                        }
                        let primary = match (stmt, expr) {
                            (Some(stmt), _) => DiagnosticLocation::Stmt(stmt),
                            (None, Some(expr)) => DiagnosticLocation::Expr(expr),
                            (None, None) => unreachable!("one anchor is always set"),
                        };
                        diags.push(TirDiagnostic {
                            error: TirTypeError::TypeMismatch {
                                expected: expected.to_plain(),
                                got: actual.to_plain(),
                            },
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
                                lhs: self.plain_finalized(&lhs),
                                rhs: self.plain_finalized(&rhs),
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
                                lhs: self.plain_finalized(&lhs),
                                rhs: self.plain_finalized(&rhs),
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
                                ty: self.plain_finalized(&ty),
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
                    PendingDiag::MissingRequiredObjectFields {
                        object,
                        class_name,
                        field_names,
                    } => {
                        diags.push(TirDiagnostic {
                            error: TirTypeError::MissingRequiredClassFields {
                                class_name,
                                field_names,
                            },
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::Expr(object),
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
                            || self.cached_subtype(&extra, &declared)
                        {
                            continue;
                        }
                        // A synthetic-effect-param extra traces to a
                        // CALLBACK parameter: the humanized wording names
                        // it (TIR's CallbackThrowsContractViolation).
                        if let InferTy::TypeVar(param, _) = extra.kind()
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
                            .or_insert((declared.clone().into_ty(), extra.clone().into_ty()));
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
                                expected: self.plain_finalized(&expected),
                                got: self.plain_finalized(&found),
                            },
                            severity: DiagnosticSeverity::Error,
                            primary: DiagnosticLocation::Pat(pat),
                            related: Vec::new(),
                        });
                        continue;
                    }
                    PendingDiag::UnresolvedPatternName { pat, name } => {
                        diags.push(TirDiagnostic {
                            error: crate::diagnostics::removed_reflect_spelling(&name)
                                .unwrap_or(TirTypeError::UnresolvedName { name }),
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
                                first_type: self.plain_finalized(&first),
                                other_type: self.plain_finalized(&other),
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
                            primary: DiagnosticLocation::BodyTypeRef(type_ref),
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
                    PendingDiag::LetElseMustDiverge { expr, got } => (
                        TirTypeError::LetElseMustDiverge {
                            got: self.plain_finalized(&got),
                        },
                        expr,
                    ),
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
                | DiagnosticLocation::LambdaParameter(id, _)
                | DiagnosticLocation::ObjectFieldName(_, id) => (0u8, u32::from(id.into_raw())),
                DiagnosticLocation::Stmt(id) => (1, u32::from(id.into_raw())),
                DiagnosticLocation::TypeAnnot(id) => (2, u32::from(id.into_raw())),
                DiagnosticLocation::Pat(id) => (4, u32::from(id.into_raw())),
                DiagnosticLocation::BodyTypeRef(id) => {
                    (5, u32::from(self.type_refs.raw_id(id).into_raw()))
                }
                DiagnosticLocation::UnreflectArg { carrier, .. } => {
                    (6, u32::from(carrier.into_raw()))
                }
                DiagnosticLocation::Span(range) => (3, u32::from(range.start())),
            });
            diags.dedup();
            result.diagnostics = diags;
        }
        // The writeback pass covers every recorded table (rustc's
        // `resolve_type_vars_in_body`): the virtual-field VIEW and the
        // path ladders' per-segment types carry types.
        for resolution in result.member_resolutions.values_mut() {
            finalize_member_resolution(&mut self, resolution);
        }
        for path in result.path_resolutions.values_mut() {
            for segment in &mut path.segments {
                segment.ty = self.finalize_ty(&segment.ty).into_ty();
                if let Some(resolution) = &mut segment.resolution {
                    finalize_member_resolution(&mut self, resolution);
                }
            }
        }
        for plan in result.call_plans.values_mut() {
            for ty in &mut plan.type_args {
                *ty = self.finalize_ty(ty).into_ty();
            }
            for slot in &mut plan.slots {
                match slot {
                    CallTypeArgPlan::Static {
                        ty, emission_ty, ..
                    } => {
                        *ty = self.finalize_ty(ty).into_ty();
                        *emission_ty = self.finalize_emission_ty(emission_ty).into_ty();
                    }
                    CallTypeArgPlan::Runtime { occurrence_ty, .. } => {
                        *occurrence_ty = self.finalize_ty(occurrence_ty).into_ty();
                    }
                }
            }
            for check in &mut plan.deferred_checks {
                match check {
                    RuntimeCheck::Argument { expected, .. } => {
                        *expected = self.finalize_ty(expected).into_ty();
                    }
                    RuntimeCheck::Bound { argument, bound } => {
                        *argument = self.finalize_ty(argument).into_ty();
                        *bound = InferInterface::new(
                            bound.name.clone(),
                            bound
                                .generics
                                .iter()
                                .map(|ty| self.finalize_ty(ty).into_ty())
                                .collect(),
                            bound
                                .associated_types
                                .iter()
                                .map(|(name, ty)| (name.clone(), self.finalize_ty(ty).into_ty()))
                                .collect(),
                        );
                    }
                }
            }
        }
        for check in &mut result.runtime_checks {
            match check {
                RuntimeCheck::Argument { expected, .. } => {
                    *expected = self.finalize_ty(expected).into_ty();
                }
                RuntimeCheck::Bound { argument, bound } => {
                    *argument = self.finalize_ty(argument).into_ty();
                    *bound = InferInterface::new(
                        bound.name.clone(),
                        bound
                            .generics
                            .iter()
                            .map(|ty| self.finalize_ty(ty).into_ty())
                            .collect(),
                        bound
                            .associated_types
                            .iter()
                            .map(|(name, ty)| (name.clone(), self.finalize_ty(ty).into_ty()))
                            .collect(),
                    );
                }
            }
        }
        for adjustments in result.expr_adjustments.values_mut() {
            for adjustment in adjustments.iter_mut() {
                adjustment.target = self.finalize_ty(&adjustment.target).into_ty();
            }
        }
        self.materialize_result(result)
    }

    /// [`finalize_ty`](Self::finalize_ty) + the total plain exit: the
    /// one-step form for diagnostic payloads and other plain-vocabulary
    /// consumers inside `finish`.
    fn plain_finalized(&mut self, ty: &Ty) -> baml_type::Ty {
        self.finalize_ty(ty).to_plain()
    }

    /// One recorded type, finalized: solved variables substituted,
    /// survivors erased to the local Error sentinel, unions
    /// re-canonicalized (skipped for error-carrying types - the canonical
    /// algebra is Error-tolerant and would collapse them arbitrarily).
    /// The [`ClosedTy`] return IS the finalize guarantee: nothing leaves
    /// this function still carrying a variable.
    fn finalize_ty(&mut self, ty: &Ty) -> ClosedTy {
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

    /// Finalize a written type for runtime emission. This performs the same
    /// substitution, inference cleanup, and mandatory projection reduction as
    /// semantic finalization, but deliberately leaves every union node in its
    /// written order instead of applying union set algebra.
    fn finalize_emission_ty(&mut self, ty: &Ty) -> ClosedTy {
        let resolved = self.table.resolve_completely(ty);
        let erased = erase_infer(&resolved);
        self.reduce_projections(&erased, PROJECTION_FINALIZE_FUEL)
    }

    /// Post-substitution projection normalization (rustc's
    /// instantiate-then-normalize; rust-analyzer normalizes projections at
    /// the result boundary the same way): every projection the oracle can
    /// determine reduces, so results and renders show what the type IS -
    /// `(IntStore as Store).Item` finalizes as `int`. Targeted rather than
    /// full canonicalization, which would also expand nominal aliases;
    /// renders keep those by design.
    fn reduce_projections(&self, ty: &ClosedTy, fuel: u32) -> ClosedTy {
        if fuel == 0 || !ty.has_projection() {
            return ty.clone();
        }
        let rebuilt = ty.map_children(|child| self.reduce_projections(child, fuel));
        // Node-local normalization (rustc's lazy normalize): a ground
        // projection reduces through the oracle. The closed descent means
        // the plain image is total; destructuring it hands the oracle its
        // native vocabulary in one step.
        if let InferTy::AssociatedTypeProjection { .. } = rebuilt.kind()
            && let baml_type::Ty::AssociatedTypeProjection {
                base,
                interface,
                member,
                ..
            } = rebuilt.to_plain()
            && let baml_type::normalize::ProjectionStep::Reduced(step) =
                baml_type::normalize::TypeContext::project(
                    &self.facts,
                    &base,
                    &interface,
                    &member,
                    fuel,
                )
        {
            return self.reduce_projections(&ClosedTy::from_plain(&step), fuel - 1);
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
        let iterable = baml_type::interned::InferInterface::new(
            baml_type::TypeName::new(
                baml_type::Name::new("baml"),
                vec![baml_type::Name::new("iter")],
                baml_type::Name::new("Iterable"),
            ),
            Box::new([]),
            Box::new([]),
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
        let existential = iterable.existential();
        let projection = Ty::intern(InferTy::AssociatedTypeProjection {
            base: collection.clone(),
            interface: iterable,
            member: baml_type::Name::new("Item"),
            attr: baml_type::TyAttr::default(),
        });
        let reduced = self.structurally_resolve(&projection);
        if reduced.has_projection() && !reduced.has_infer() {
            // Ground and irreducible. Two legitimate outcomes, split by
            // the SAME verdict the finalize filter applies to the
            // obligation's mismatch (one spelling, one verdict - B-1576):
            // a collection that fails `Iterable` reports E0006, so its
            // element is the DIAGNOSED error sentinel and consumers
            // suppress cascades (rustc's guaranteed-error discipline). A
            // collection that satisfies the bound keeps the projection AS
            // the element - rustc's rigid `<T as IntoIterator>::Item`,
            // which `lower_to_runtime` carries for per-receiver dispatch;
            // erasing it to an error rejected legal generic and union
            // collections without any diagnostic (the shipped abort).
            // Judge only a CLOSED collection (an open one defers to later
            // resolution, as before), and canonicalize only under that
            // gate: the semantic join is meaningless on open input, and
            // running it first was a latent ICE.
            let collection = self.table.resolve_completely(collection);
            if let Ok(collection) = ClosedTy::try_from(&collection) {
                let canonical = self.canonicalize_unions(&collection);
                if !self.cached_subtype(&canonical, &existential) {
                    return Ty::error();
                }
            }
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
    fn canonicalize_unions(&self, ty: &ClosedTy) -> ClosedTy {
        match ty.kind() {
            InferTy::Union(..) => {
                let mut members: Vec<ClosedTy> = Vec::new();
                ty.for_each_child(|member| members.push(self.canonicalize_unions(member)));
                // Error-carrying unions clean up STRUCTURALLY only:
                // the canonical algebra's equivalence treats the Error
                // sentinel as bidirectionally compatible (checking's
                // cascade suppression), so a container member like
                // `!error[]` would MERGE into `int[]` and vanish.
                // rustc's discipline is the opposite for identity -
                // `InferTy::Error` equals only itself in canonical
                // forms; compat lives in the relate layer alone.
                // Until the shared algebra splits those roles (S16,
                // when TIR stops consuming it), flatten/dedup/collapse
                // here and skip absorption.
                if members.iter().any(|member| member.has_error()) {
                    let raw: Vec<Ty> = members
                        .iter()
                        .map(|member| member.as_ty().clone())
                        .collect();
                    return ClosedTy::try_from(syntactic_union(&raw)).unwrap_or_else(|_| {
                        unreachable!("a syntactic union of closed members is closed")
                    });
                }
                let joined = canonical_union_interned(&members, &self.facts);
                match joined.kind() {
                    InferTy::Union(members, attr) => {
                        let (mut ordered, nulls): (Vec<Ty>, Vec<Ty>) = members
                            .iter()
                            .cloned()
                            .partition(|member| !matches!(member.kind(), InferTy::Null { .. }));
                        ordered.extend(nulls);
                        ClosedTy::try_from(Ty::intern(InferTy::Union(ordered.into(), attr.clone())))
                            .unwrap_or_else(|_| {
                                unreachable!(
                                    "reordering a closed union's members preserves closedness"
                                )
                            })
                    }
                    _ => joined,
                }
            }
            _ => ty.map_children(|child| self.canonicalize_unions(child)),
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
            .filter(|ty| !matches!(ty.kind(), InferTy::Unknown { .. }))
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
                    .all(|upper| self.cached_subtype(candidate, upper))
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
            let maximum_of = |candidates: &[Ty]| -> Option<Ty> {
                let subsumes_all = |candidate: &&Ty| {
                    candidates
                        .iter()
                        .all(|lower| self.cached_subtype(lower, candidate))
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
                    .filter(|max| uppers.iter().all(|upper| self.cached_subtype(max, upper)))
                    .or_else(|| maximum_of(&lowers))
            } else {
                maximum_of(&lowers).or_else(|| maximum_of(&widened))
            };
            match maximum {
                Some(maximum)
                    if uppers
                        .iter()
                        .all(|upper| self.cached_subtype(&maximum, upper)) =>
                {
                    maximum
                }
                _ => {
                    // No join: genuinely incompatible demands. A monomorphic
                    // source with no initial type follows first-demand order:
                    // the first ground demand wins so later incompatible
                    // demands report through the provisional re-check at
                    // finalize. Other vars, such as call instantiations, fail
                    // resolution instead.
                    if self
                        .table
                        .unsolved_policy(var)
                        .is_some_and(unify::VarPolicy::first_demand_commits)
                    {
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
        let mut resolved = if let InferTy::InferVar { var, .. } = resolved.kind() {
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
        if resolved.has_projection()
            && let Ok(closed) = ClosedTy::try_from(&resolved)
        {
            // One spelling, one verdict: reduce over the canonical form.
            // Forcing can ground a syntactic union `union_of` deferred
            // while a member carried a variable, and the oracle reads the
            // spelling it is given - a member-identical union like
            // `list<int> | list<int>` must collapse before a projection
            // over it can reduce (B-1576).
            let canonical = self.canonicalize_unions(&closed);
            let reduced = self.reduce_projections(&canonical, PROJECTION_FINALIZE_FUEL);
            return self.expand_alias_ty(&reduced);
        }
        // WEAK aliases normalize here too (rustc's `Alias::Weak` in
        // `structurally_resolve_type`): a structure consumer never
        // sees the nominal wrapper, so no consumer can forget to
        // expand. Recorded types keep the written name - this is the
        // demanded STRUCTURE, not the render.
        if matches!(resolved.kind(), InferTy::TypeAlias(..)) {
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
        let param_tys = self.param_tys.clone();
        for (index, param_ty) in param_tys.iter().enumerate() {
            let resolved = self.table.resolve_completely(param_ty);
            let Some(callback) = self.callback_root_fn(&resolved) else {
                continue;
            };
            let InferTy::Function { throws, .. } = callback.kind() else {
                unreachable!("callback_root_fn returns a function type")
            };
            if matches!(throws.kind(), InferTy::TypeVar(p, _) if p == effect) {
                return data.params.get(index).map(|param| param.name.clone());
            }
        }
        None
    }

    /// Every local value name in the expression's lexical scope and its
    /// ancestors - the near-match candidate pool for shorthand suggestions.
    fn local_binding_names_at(&self, expr: ExprId) -> Vec<baml_type::Name> {
        let mut names = Vec::new();
        let Some(scope) = self
            .metadata_key(expr)
            .and_then(|key| self.index.expression_scope(key))
        else {
            return names;
        };
        for ancestor in self.index.ancestor_scopes(scope) {
            let bindings = &self.index.scope_bindings[ancestor.index() as usize];
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
            &rendered_plain(actual),
            &rendered_plain(expected),
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
                        aliases
                            .entry(qtn)
                            .or_insert_with(|| crate::lower::type_alias_value(self.db, *loc));
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
                if !self.cached_subtype(&actual, &expected)
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
    if let InferTy::InferVar { var, attr } = ty.kind() {
        return Ty::intern(InferTy::TypeVar(
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
    if let InferTy::InferVar { var, .. } = ty.kind() {
        out.push(*var);
    }
    baml_type::interned::for_each_child(ty.kind(), |child| collect_infer_vars(child, out));
}

/// Whether `source` and `target` share a head CONSTRUCTOR - the gate
/// for recursing into a union target's single structured constituent
/// (only pairs plain unification could relate member-wise).
fn same_head_constructor(source: &Ty, target: &Ty) -> bool {
    match (source.kind(), target.kind()) {
        (InferTy::List(..), InferTy::List(..))
        | (InferTy::Map { .. }, InferTy::Map { .. })
        | (InferTy::Future(..), InferTy::Future(..)) => true,
        (InferTy::Class(a, a_args, _), InferTy::Class(b, b_args, _)) => {
            a == b && a_args.len() == b_args.len()
        }
        (InferTy::Interface(a, a_args, _, _), InferTy::Interface(b, b_args, _, _)) => {
            a == b && a_args.len() == b_args.len()
        }
        (
            InferTy::Function {
                params: a_params, ..
            },
            InferTy::Function {
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
/// for constructor inference slots, and for `impl_facts`' poisoned-header
/// gate).
pub(crate) fn ty_mentions_param(ty: &Ty, param: &baml_type::ParamTy) -> bool {
    fn walk(ty: &Ty, param: &baml_type::ParamTy, found: &mut bool) {
        if *found {
            return;
        }
        if matches!(ty.kind(), InferTy::TypeVar(p, _) if p == param) {
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

/// The user-written portion of a function's flattened generic frame. Owner
/// parameters precede it and compiler-created callback-effect parameters
/// follow it, so neither group should make a function value require explicit
/// specialization.
fn function_user_generic_params<'a, 'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    function: baml_compiler2_hir::loc::FunctionLoc<'db>,
    signature: &'a crate::lower::FunctionSignature,
) -> &'a [baml_type::ParamTy] {
    let data = baml_compiler2_ppir::item_data::elaborated_function_data(db, function);
    let end = signature
        .generic_params
        .len()
        .checked_sub(data.synthetic_effect_params.len())
        .expect("synthetic effect parameters are a suffix of the generic frame");
    let start = end
        .checked_sub(data.user_generic_params.len())
        .expect("user parameters precede synthetic effects in the generic frame");
    &signature.generic_params[start..end]
}

fn function_signature_mentions_param(
    signature: &crate::lower::FunctionSignature,
    param: &baml_type::ParamTy,
) -> bool {
    signature.params.iter().any(|function_param| {
        ty_mentions_param(&crate::impls::interned_ty(&function_param.ty), param)
    }) || ty_mentions_param(&crate::impls::interned_ty(&signature.ret), param)
        || ty_mentions_param(&crate::impls::interned_ty(&signature.throws), param)
}

fn external_bounds_map(
    external: &crate::callable::ExternalCallable,
) -> FxHashMap<baml_type::ParamTy, Vec<baml_type::Interface>> {
    external
        .owner_generic_params
        .iter()
        .zip(&external.owner_generic_param_bounds)
        .chain(
            external
                .generic_params
                .iter()
                .zip(&external.generic_param_bounds),
        )
        .map(|(param, bounds)| (param.clone(), bounds.clone()))
        .collect()
}

fn external_type_position(
    target: &crate::callable::ExternalCallTarget,
) -> crate::lower::TypePosition {
    match target {
        crate::callable::ExternalCallTarget::Method {
            package,
            namespace,
            class,
            name,
        } if package.as_str() == "reflect"
            && namespace.is_empty()
            && class.as_str() == "Package"
            && name.as_str() == "get_function" =>
        {
            crate::lower::TypePosition::ExtractionContract
        }
        _ => crate::lower::TypePosition::Existential,
    }
}

fn external_target_path(target: &crate::callable::ExternalCallTarget) -> baml_type::Name {
    let path = match target {
        crate::callable::ExternalCallTarget::Free {
            package,
            namespace,
            name,
        } => std::iter::once(package)
            .chain(namespace)
            .chain(std::iter::once(name))
            .map(baml_type::Name::as_str)
            .collect::<Vec<_>>()
            .join("."),
        crate::callable::ExternalCallTarget::Method {
            package,
            namespace,
            class,
            name,
        } => std::iter::once(package)
            .chain(namespace)
            .chain([class, name])
            .map(baml_type::Name::as_str)
            .collect::<Vec<_>>()
            .join("."),
        crate::callable::ExternalCallTarget::Interface { interface, method } => {
            format!("{}.{}", interface.render_dotted(true), method)
        }
    };
    baml_type::Name::new(path)
}

fn interface_mentions_param(interface: &InferInterface, param: &baml_type::ParamTy) -> bool {
    interface
        .generics
        .iter()
        .chain(interface.associated_types.iter().map(|(_, ty)| ty))
        .any(|ty| ty_mentions_param(ty, param))
}

/// Does a call-scoped runtime parameter survive into a type the call
/// `published` (its result, or the error it can throw) as more than that type
/// itself? See [`InferCtx::report_runtime_type_escape`] for why the bare
/// parameter is the one shape that does not escape.
fn runtime_param_escapes(published: &Ty, param: &baml_type::ParamTy) -> bool {
    if matches!(published.kind(), InferTy::TypeVar(candidate, _) if candidate == param) {
        return false;
    }
    ty_mentions_param(published, param)
}

fn collect_unreflect_type_refs(
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    id: baml_compiler2_hir::type_ref::TypeRefId,
    out: &mut Vec<(baml_compiler2_hir::type_ref::TypeRefId, ExprId)>,
) {
    use baml_compiler2_hir::type_ref::TypeRefKind;
    match &store[id].kind {
        TypeRefKind::Unreflect {
            operand: Some(operand),
        } => out.push((id, *operand)),
        TypeRefKind::Unreflect { operand: None } => {}
        TypeRefKind::Path {
            generic_args,
            associated_type_bindings,
            ..
        } => {
            for child in generic_args {
                collect_unreflect_type_refs(store, *child, out);
            }
            for binding in associated_type_bindings {
                collect_unreflect_type_refs(store, binding.ty, out);
            }
        }
        TypeRefKind::AssociatedTypeProjection {
            base, interface, ..
        } => {
            collect_unreflect_type_refs(store, *base, out);
            if let Some(interface) = interface {
                collect_unreflect_type_refs(store, *interface, out);
            }
        }
        TypeRefKind::Optional { inner } | TypeRefKind::List { inner } => {
            collect_unreflect_type_refs(store, *inner, out);
        }
        TypeRefKind::Map { key, value } => {
            collect_unreflect_type_refs(store, *key, out);
            collect_unreflect_type_refs(store, *value, out);
        }
        TypeRefKind::Union { variants } => {
            for child in variants {
                collect_unreflect_type_refs(store, *child, out);
            }
        }
        TypeRefKind::Function {
            params,
            ret,
            throws,
        } => {
            for param in params {
                collect_unreflect_type_refs(store, param.ty, out);
            }
            collect_unreflect_type_refs(store, *ret, out);
            if let Some(throws) = throws {
                collect_unreflect_type_refs(store, *throws, out);
            }
        }
        _ => {}
    }
}

/// A plain declared constraint as the interned occurrence type a runtime
/// slot carries (plain→interned, the total ingestion direction).
fn interface_occurrence_ty(interface: &baml_type::Interface) -> Ty {
    crate::impls::interned_ty(&interface.to_ty())
}

/// Substitute every solved/static call parameter while deliberately retaining
/// runtime parameters as rigid variables. The resulting template is what the
/// VM specializes after loading runtime type values.
fn substitute_static_call_params(
    ty: &Ty,
    args: &[Ty],
    runtime_params: &[baml_type::ParamTy],
) -> Ty {
    if let InferTy::TypeVar(param, _) = ty.kind() {
        if runtime_params.contains(param) {
            return ty.clone();
        }
        if let Some(replacement) = args.get(param.index() as usize) {
            return replacement.clone();
        }
    }
    if !ty.has_typevar() {
        return ty.clone();
    }
    Ty::intern(
        ty.kind()
            .map_children(|child| substitute_static_call_params(child, args, runtime_params)),
    )
}

fn replace_rigid_param(ty: &Ty, param: &baml_type::ParamTy, replacement: &Ty) -> Ty {
    if matches!(ty.kind(), InferTy::TypeVar(candidate, _) if candidate == param) {
        return replacement.clone();
    }
    if !ty.has_typevar() {
        return ty.clone();
    }
    Ty::intern(
        ty.kind()
            .map_children(|child| replace_rigid_param(child, param, replacement)),
    )
}

fn substitute_static_interface_params(
    interface: &InferInterface,
    args: &[Ty],
    runtime_params: &[baml_type::ParamTy],
) -> InferInterface {
    InferInterface::new(
        interface.name.clone(),
        interface
            .generics
            .iter()
            .map(|ty| substitute_static_call_params(ty, args, runtime_params))
            .collect(),
        interface
            .associated_types
            .iter()
            .map(|(name, ty)| {
                (
                    name.clone(),
                    substitute_static_call_params(ty, args, runtime_params),
                )
            })
            .collect(),
    )
}

fn bind_receiver(fn_ty: Ty) -> Ty {
    let InferTy::Function {
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
    Ty::intern(InferTy::Function {
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
    let params: Box<[baml_type::interned::InferFunctionParamTy]> = signature
        .params
        .iter()
        .map(|param| baml_type::interned::InferFunctionParamTy {
            name: Some(param.name.clone()),
            ty: substitute_params(&crate::impls::interned_ty(&param.ty), instantiation),
            mode: if param.has_default {
                baml_type::FunctionParamMode::Optional
            } else {
                baml_type::FunctionParamMode::Required
            },
        })
        .collect();
    Ty::intern(InferTy::Function {
        params,
        ret: substitute_params(&crate::impls::interned_ty(&signature.ret), instantiation),
        throws: substitute_params(&crate::impls::interned_ty(&signature.throws), instantiation),
        attr: TyAttr::default(),
    })
}

impl<'db> InferenceContext<'db> {
    /// Inference finalize's exit through the
    /// [`baml_type::interned::ClosedTy`] boundary, TOTAL: a finalized
    /// (closed) value materializes directly; a value that arrives still
    /// open is finalized on the spot, so the conversion IS the finalize
    /// boundary and no field enumeration elsewhere has to be exhaustive
    /// for soundness. `finish`'s writeback pass remains the semantic
    /// enumeration - a value it missed arriving open here is drift,
    /// asserted in debug. Boundaries that can legitimately meet an OPEN
    /// type mid-inference must not use this (finalizing there would erase
    /// a variable that could still solve): they construct `ClosedTy`
    /// themselves and pick a disposition on `Err` (defer, suppress, or
    /// rename-for-rendering).
    fn materialize_ty(&mut self, ty: &Ty) -> baml_type::Ty {
        match ClosedTy::try_from(ty) {
            Ok(closed) => closed.to_plain(),
            Err(_) => {
                debug_assert!(
                    false,
                    "a type reached the plain boundary without passing finalize"
                );
                self.plain_finalized(ty)
            }
        }
    }

    /// [`materialize_ty`](Self::materialize_ty)'s satellite twin, for
    /// interface constraints - total field-by-field through the same edge.
    fn materialize_interface(&mut self, iface: &InferInterface) -> baml_type::Interface {
        baml_type::Interface::new(
            iface.name.clone(),
            iface
                .generics
                .iter()
                .map(|ty| self.materialize_ty(ty))
                .collect(),
            iface
                .associated_types
                .iter()
                .map(|(name, ty)| (name.clone(), self.materialize_ty(ty)))
                .collect(),
        )
    }

    /// Materialize the finalized working result into the public plain
    /// artifact - the LAST step of `finish`. Every value conversion routes
    /// through the total boundary edge
    /// ([`materialize_ty`](Self::materialize_ty) /
    /// [`materialize_interface`](Self::materialize_interface)), so no
    /// interned handle escapes inference; every struct converts by
    /// exhaustive match/field-list, so a new type-carrying field cannot be
    /// added without its materialization appearing here.
    fn materialize_result(&mut self, working: WorkingResult<'db>) -> InferenceResult<'db> {
        let InferenceResult {
            type_of_expr,
            type_of_pat,
            throws,
            type_mismatches,
            non_exhaustive_matches,
            diagnostics,
            member_resolutions,
            path_resolutions,
            call_plans,
            type_bindings,
            type_ref_bindings,
            runtime_checks,
            expr_adjustments,
            desugared_callees,
        } = working;
        InferenceResult {
            type_of_expr: type_of_expr
                .into_iter()
                .map(|(expr, ty)| (expr, self.materialize_ty(&ty)))
                .collect(),
            type_of_pat: type_of_pat
                .into_iter()
                .map(|(pat, ty)| (pat, self.materialize_ty(&ty)))
                .collect(),
            throws: self.materialize_ty(&throws),
            type_mismatches: type_mismatches
                .into_iter()
                .map(|(expr, (expected, actual))| {
                    (
                        expr,
                        (self.materialize_ty(&expected), self.materialize_ty(&actual)),
                    )
                })
                .collect(),
            non_exhaustive_matches,
            diagnostics,
            member_resolutions: member_resolutions
                .into_iter()
                .map(|(expr, resolution)| (expr, self.materialize_member_resolution(resolution)))
                .collect(),
            path_resolutions: path_resolutions
                .into_iter()
                .map(|(expr, path)| {
                    (
                        expr,
                        ResolvedPath {
                            segments: path
                                .segments
                                .into_iter()
                                .map(|segment| ResolvedPathSegment {
                                    ty: self.materialize_ty(&segment.ty),
                                    resolution: segment.resolution.map(|resolution| {
                                        self.materialize_member_resolution(resolution)
                                    }),
                                })
                                .collect(),
                        },
                    )
                })
                .collect(),
            call_plans: call_plans
                .into_iter()
                .map(|(expr, plan)| (expr, self.materialize_call_plan(plan)))
                .collect(),
            type_bindings: type_bindings
                .into_iter()
                .map(|(stmt, binding)| (stmt, self.materialize_scoped_binding(binding)))
                .collect(),
            type_ref_bindings: type_ref_bindings
                .into_iter()
                .map(|(type_ref, bindings)| {
                    (
                        type_ref,
                        bindings
                            .into_iter()
                            .map(|binding| self.materialize_scoped_binding(binding))
                            .collect(),
                    )
                })
                .collect(),
            runtime_checks: runtime_checks
                .into_iter()
                .map(|check| self.materialize_runtime_check(check))
                .collect(),
            expr_adjustments: expr_adjustments
                .into_iter()
                .map(|(expr, adjustments)| {
                    (
                        expr,
                        adjustments
                            .into_iter()
                            .map(|adjustment| Adjustment {
                                kind: adjustment.kind,
                                target: self.materialize_ty(&adjustment.target),
                            })
                            .collect(),
                    )
                })
                .collect(),
            desugared_callees,
        }
    }

    fn materialize_member_resolution(
        &mut self,
        resolution: MemberResolution<'db, Ty>,
    ) -> MemberResolution<'db> {
        match resolution {
            MemberResolution::Field { class, field } => MemberResolution::Field { class, field },
            MemberResolution::Variant { enum_loc, variant } => {
                MemberResolution::Variant { enum_loc, variant }
            }
            MemberResolution::Free { func } => MemberResolution::Free { func },
            MemberResolution::BoundMethod { class, func } => {
                MemberResolution::BoundMethod { class, func }
            }
            MemberResolution::UnboundMethod { class, func } => {
                MemberResolution::UnboundMethod { class, func }
            }
            MemberResolution::InterfaceVirtualMethod { interface, method } => {
                MemberResolution::InterfaceVirtualMethod { interface, method }
            }
            MemberResolution::InterfaceConcreteMethod {
                impl_block,
                func,
                frame_type_args,
                from_interface_default,
            } => MemberResolution::InterfaceConcreteMethod {
                impl_block,
                func,
                frame_type_args: frame_type_args
                    .iter()
                    .map(|ty| self.materialize_ty(ty))
                    .collect(),
                from_interface_default,
            },
            MemberResolution::InterfaceVirtualField {
                interface,
                view,
                field_index,
                field,
            } => MemberResolution::InterfaceVirtualField {
                interface,
                view: self.materialize_ty(&view),
                field_index,
                field,
            },
            MemberResolution::External(callable) => MemberResolution::External(callable),
            MemberResolution::ExternalField { class, field } => {
                MemberResolution::ExternalField { class, field }
            }
            MemberResolution::ExternalVariant { enum_name, variant } => {
                MemberResolution::ExternalVariant { enum_name, variant }
            }
            MemberResolution::ExternalInterfaceVirtualField {
                interface,
                view,
                field_index,
                field,
            } => MemberResolution::ExternalInterfaceVirtualField {
                interface,
                view: self.materialize_ty(&view),
                field_index,
                field,
            },
        }
    }

    fn materialize_call_plan(&mut self, plan: CallPlan<Ty, InferInterface>) -> CallPlan {
        let CallPlan {
            bindings,
            type_args,
            own_offset,
            explicit,
            slots,
            deferred_checks,
            runtime_id,
            target,
        } = plan;
        CallPlan {
            bindings,
            type_args: type_args.iter().map(|ty| self.materialize_ty(ty)).collect(),
            own_offset,
            explicit,
            slots: slots
                .into_iter()
                .map(|slot| match slot {
                    CallTypeArgPlan::Static {
                        ty,
                        emission_ty,
                        runtime_bindings,
                    } => CallTypeArgPlan::Static {
                        ty: self.materialize_ty(&ty),
                        emission_ty: self.materialize_ty(&emission_ty),
                        runtime_bindings: runtime_bindings
                            .into_iter()
                            .map(|binding| self.materialize_scoped_binding(binding))
                            .collect(),
                    },
                    CallTypeArgPlan::Runtime {
                        operand,
                        occurrence_ty,
                        parameter,
                    } => CallTypeArgPlan::Runtime {
                        operand,
                        occurrence_ty: self.materialize_ty(&occurrence_ty),
                        parameter,
                    },
                })
                .collect(),
            deferred_checks: deferred_checks
                .into_iter()
                .map(|check| self.materialize_runtime_check(check))
                .collect(),
            runtime_id,
            target,
        }
    }

    fn materialize_runtime_check(
        &mut self,
        check: RuntimeCheck<Ty, InferInterface>,
    ) -> RuntimeCheck {
        match check {
            RuntimeCheck::Argument { arg, expected } => RuntimeCheck::Argument {
                arg,
                expected: self.materialize_ty(&expected),
            },
            RuntimeCheck::Bound { argument, bound } => RuntimeCheck::Bound {
                argument: self.materialize_ty(&argument),
                bound: self.materialize_interface(&bound),
            },
        }
    }

    fn materialize_scoped_binding(&mut self, binding: ScopedTypeBinding<Ty>) -> ScopedTypeBinding {
        let ScopedTypeBinding {
            name,
            parameter,
            operand,
            template_ty,
            occurrence_ty,
        } = binding;
        ScopedTypeBinding {
            name,
            parameter,
            operand,
            template_ty: template_ty.map(|ty| self.materialize_ty(&ty)),
            occurrence_ty: self.materialize_ty(&occurrence_ty),
        }
    }
}

fn generic_function_value_shape(
    signature: &crate::lower::FunctionSignature,
    user_params: &[baml_type::ParamTy],
    receiver_is_bound: bool,
    concrete_example: bool,
) -> String {
    let mut instantiation: Vec<Ty> = signature
        .generic_params
        .iter()
        .map(|param| {
            Ty::intern(InferTy::TypeVar(
                param.clone(),
                baml_type::TyAttr::default(),
            ))
        })
        .collect();
    if concrete_example {
        for param in user_params {
            if let Some(slot) = instantiation.get_mut(param.index() as usize) {
                *slot = Ty::int();
            }
        }
    }
    let ty = function_value_ty(signature, &instantiation);
    let ty = if receiver_is_bound {
        bind_receiver(ty)
    } else {
        ty
    };
    rendered_plain(&ty).to_string()
}

fn external_generic_function_value_ty(
    function: &crate::package_interface::ResolvedFunction,
    user_params: &[baml_type::ParamTy],
    receiver_is_bound: bool,
    concrete_example: bool,
) -> Ty {
    let mut instantiation: Vec<Ty> = function
        .generic_params
        .iter()
        .map(|param| {
            Ty::intern(InferTy::TypeVar(
                param.clone(),
                baml_type::TyAttr::default(),
            ))
        })
        .collect();
    if concrete_example {
        for param in user_params {
            if let Some(slot) = instantiation.get_mut(param.index() as usize) {
                *slot = Ty::int();
            }
        }
    }
    let ty = crate::method_resolution::instantiate_external_signature(function, &instantiation);
    if receiver_is_bound {
        bind_receiver(ty)
    } else {
        ty
    }
}

fn initializer_binding_name(body: &ExprBody, initializer: ExprId) -> Option<baml_type::Name> {
    body.stmts.iter().find_map(|(_, stmt)| {
        let Stmt::Let {
            pattern,
            initializer: Some(candidate),
            ..
        } = stmt
        else {
            return None;
        };
        if *candidate != initializer {
            return None;
        }
        match &body.patterns[*pattern] {
            Pattern::Bind { name, .. } => Some(name.clone()),
            _ => None,
        }
    })
}

/// Turn a finalized, partially inferred type into a concrete diagnostic
/// example without discarding the slots inference did solve. `int` is only a
/// placeholder for still-unknown, unbounded slots; bounded generics never use
/// this example path.
fn diagnostic_example_ty(ty: &Ty) -> Ty {
    match ty.kind() {
        InferTy::Error { .. } | InferTy::InferVar { .. } => Ty::int(),
        kind => Ty::intern(kind.map_children(diagnostic_example_ty)),
    }
}

/// Finalize every type a recorded member resolution carries. Both writeback
/// sites (the expression table and each path-ladder segment) go through this,
/// so a resolution variant that gains a type-carrying field is finalized in
/// one place rather than two that can drift.
fn finalize_member_resolution(
    ctx: &mut InferenceContext<'_>,
    resolution: &mut MemberResolution<'_, Ty>,
) {
    match resolution {
        MemberResolution::InterfaceVirtualField { view, .. }
        | MemberResolution::ExternalInterfaceVirtualField { view, .. } => {
            *view = ctx.finalize_ty(view).into_ty();
        }
        MemberResolution::InterfaceConcreteMethod {
            frame_type_args, ..
        } => {
            for ty in frame_type_args.iter_mut() {
                *ty = ctx.finalize_ty(ty).into_ty();
            }
        }
        MemberResolution::Field { .. }
        | MemberResolution::Variant { .. }
        | MemberResolution::Free { .. }
        | MemberResolution::BoundMethod { .. }
        | MemberResolution::UnboundMethod { .. }
        | MemberResolution::InterfaceVirtualMethod { .. }
        | MemberResolution::External(_)
        | MemberResolution::ExternalField { .. }
        | MemberResolution::ExternalVariant { .. } => {}
    }
}

/// Replaces every `Infer` node (unsolved variable or hole) with the Error
/// sentinel, in place - the finalize half of rulings 2/3.
///
// BUG: this erases UNCONDITIONALLY, but the matching diagnostic only covers
// the `hole_vars` subset (a written `_`, reported as E0147 in `finalize`). An
// unsolved variable is an unsolved variable whether its `_` was written or
// implicit, so every one of them owes a diagnostic; the ones that do not get
// one become an `Error` nothing accounts for, breaking the invariant `Error`
// carries ("a hard error was already reported here, do not cascade").
// `ResolvedAliases::convert` relies on that invariant and panics on an `Error`
// reaching runtime lowering, so MIR launders every `Error` to the top type to
// stay alive - which defuses the guard on every MIR path (see
// `erase_compiler_only_ty`).
//
// The known undiagnosed producer is a generic call made in the scope of a
// `type T = unreflect(expr)` binding, whose slots go unsolved (corpus:
// `ns_runtime_type_binding_generic_calls`). That source is ILLEGAL, not
// well-typed: the call needs type arguments it cannot get. Report every
// unsolved variable and it is rejected at compile time, compilation stops
// before MIR, and the laundering can go.
fn erase_infer(ty: &Ty) -> ClosedTy {
    fn erase(ty: &Ty) -> Ty {
        if !ty.has_infer() {
            return ty.clone();
        }
        if matches!(ty.kind(), InferTy::InferVar { .. }) {
            return Ty::error();
        }
        Ty::intern(ty.kind().map_children(erase))
    }
    // The function's own postcondition, checked in O(1): every variable
    // was just replaced.
    ClosedTy::try_from(erase(ty))
        .unwrap_or_else(|_| unreachable!("erase_infer replaced every inference node"))
}

/// The rename-for-rendering edge for `&self` diagnostic helpers, which
/// have no table access to finalize: a closed value materializes; a live
/// variable renders as the user-denotable top type
/// ([`infer_to_diagnostic_unknown`]).
fn rendered_plain(ty: &Ty) -> baml_type::Ty {
    ClosedTy::try_from(ty)
        .unwrap_or_else(|_| {
            ClosedTy::try_from(infer_to_diagnostic_unknown(ty)).unwrap_or_else(|_| {
                unreachable!("infer_to_diagnostic_unknown closes every variable")
            })
        })
        .to_plain()
}

/// Replaces every unresolved inference node with the user-denotable top type
/// while preserving the surrounding shape for diagnostics.
fn infer_to_diagnostic_unknown(ty: &Ty) -> Ty {
    if !ty.has_infer() {
        return ty.clone();
    }
    if matches!(ty.kind(), InferTy::InferVar { .. }) {
        return Ty::intern(InferTy::Unknown {
            attr: TyAttr::default(),
        });
    }
    Ty::intern(ty.kind().map_children(infer_to_diagnostic_unknown))
}

/// A fresh literal widens to its base primitive at binding sites (the spec's
/// TypeScript-style widening); everything else passes through. Top-level
/// only - container-element widening arrives with the join machinery.
fn widen_fresh_literal(ty: &Ty) -> Ty {
    match ty.kind() {
        InferTy::Literal(literal, Freshness::Fresh, attr) => {
            Ty::intern(literal_base(literal, attr.clone()))
        }
        _ => ty.clone(),
    }
}

/// The base primitive a literal type belongs to.
pub(crate) fn literal_base(literal: &Literal, attr: TyAttr) -> InferTy {
    match literal {
        Literal::Int(_) => InferTy::Int { attr },
        Literal::Bigint(_) => InferTy::Bigint { attr },
        Literal::Float(_) => InferTy::Float { attr },
        Literal::String(_) => InferTy::String { attr },
        Literal::Bool(_) => InferTy::Bool { attr },
    }
}

/// An operand's union alternatives for operator dispatch, literals widened
/// to their bases regardless of freshness (dispatch is by base type; every
/// alternative must support the operator).
fn operand_members(ty: &Ty) -> Vec<Ty> {
    fn widen(ty: &Ty) -> Ty {
        match ty.kind() {
            InferTy::Literal(literal, _, attr) => Ty::intern(literal_base(literal, attr.clone())),
            // A builtin primitive-companion class receiver (`self` inside
            // `class Float`) IS its primitive for dispatch - the single
            // collapse rule (`baml_type::QualifiedTypeName::builtin_primitive`).
            InferTy::Class(qtn, args, attr) if args.is_empty() => {
                use baml_type::PrimitiveType;
                match qtn.builtin_primitive() {
                    Some(PrimitiveType::Int) => Ty::intern(InferTy::Int { attr: attr.clone() }),
                    Some(PrimitiveType::Bigint) => {
                        Ty::intern(InferTy::Bigint { attr: attr.clone() })
                    }
                    Some(PrimitiveType::Float) => Ty::intern(InferTy::Float { attr: attr.clone() }),
                    Some(PrimitiveType::String) => {
                        Ty::intern(InferTy::String { attr: attr.clone() })
                    }
                    Some(PrimitiveType::Bool) => Ty::intern(InferTy::Bool { attr: attr.clone() }),
                    _ => ty.clone(),
                }
            }
            _ => ty.clone(),
        }
    }
    match ty.kind() {
        InferTy::Union(members, _) => members.iter().map(widen).collect(),
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

#[cfg(test)]
mod runtime_param_escape_tests {
    use super::*;

    fn param() -> baml_type::ParamTy {
        baml_type::ParamTy::new(0x8000_0001, baml_type::Name::new("Out"))
    }

    fn var(param: &baml_type::ParamTy) -> Ty {
        Ty::intern(InferTy::TypeVar(param.clone(), TyAttr::default()))
    }

    /// The whole rule is one boundary: the parameter ITSELF is a value's type
    /// and does not escape; one constructor deeper it is an assertion about a
    /// value, and does.
    #[test]
    fn the_bare_parameter_is_the_only_shape_that_does_not_escape() {
        let param = param();
        assert!(!runtime_param_escapes(&var(&param), &param));
        assert!(!runtime_param_escapes(&Ty::int(), &param));
        assert!(!runtime_param_escapes(&Ty::never(), &param));
        assert!(runtime_param_escapes(
            &Ty::intern(InferTy::List(var(&param), TyAttr::default())),
            &param
        ));
        assert!(runtime_param_escapes(
            &syntactic_union(&[var(&param), Ty::null()]),
            &param
        ));
    }

    /// A different parameter is a different name: an occurs-check that keyed on
    /// shape alone would refuse every generic result.
    #[test]
    fn another_parameter_is_not_this_one() {
        let other = baml_type::ParamTy::new(0x8000_0002, baml_type::Name::new("Other"));
        assert!(!runtime_param_escapes(&var(&other), &param()));
        assert!(!runtime_param_escapes(
            &Ty::intern(InferTy::List(var(&other), TyAttr::default())),
            &param()
        ));
    }
}
