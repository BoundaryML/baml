//! Semantic interface declarations shared by SDK generators.
//!
//! Unlike a projected native signature, these types retain associated-type
//! projections and the compiler's flattened generic frame. `RuntimeTy` rejects
//! recovery/inference states while preserving valid symbolic declarations.

use std::collections::BTreeMap;

use baml_type::{ParamTy, RuntimeFunctionParamTy, RuntimeInterface, RuntimeTy, Ty as SemanticTy};

use crate::Name;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InterfaceGraph {
    pub declarations: BTreeMap<Name, InterfaceDeclaration>,
    /// Concrete class input views and live caller contracts resolved before
    /// native projection. Copied records have input views but no live methods.
    /// Conditional specializations remain in `implementations`.
    pub concrete_classes: BTreeMap<Name, ConcreteDeclaration>,
    /// Implementation rules are not keyed by class: builtin and generic
    /// receiver patterns participate in exactly the same dispatch mechanism.
    pub implementations: Vec<InterfaceImplementation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConcreteDeclaration {
    pub name: Name,
    /// Class parameters have a private frame distinct from method parameters.
    /// Native display names come from the class declaration, not this frame.
    pub generic_params: Vec<InterfaceParameter>,
    /// Complete associated bindings for views proved under the class bounds.
    /// This is native input evidence, not an exhaustive list of implementors
    /// or a claim that conditional rules apply to every specialization.
    pub implemented_interfaces: Vec<RuntimeInterface>,
    /// Public receiver methods valid for every permitted class instantiation.
    pub methods: Vec<ConcreteMethod>,
    /// These names require qualification; generating an arbitrary winner
    /// would change the language's ambiguity rule.
    pub ambiguous_methods: BTreeMap<baml_base::Name, Vec<RuntimeInterface>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConcreteMethod {
    pub name: baml_base::Name,
    /// Function type including the concrete self receiver. This is the
    /// selected implementation signature, which may refine its interface.
    pub signature: RuntimeTy,
    pub generic_params: Vec<InterfaceParameter>,
    pub target: ConcreteMethodTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConcreteMethodTarget {
    Inherent {
        class: Name,
    },
    /// Concrete dispatch through a complete compiler-proven view, including
    /// associated defaults and overrides. This does not turn the call into
    /// an existential call with a wider signature.
    Interface(RuntimeInterface),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceDeclaration {
    pub name: Name,
    pub self_param: ParamTy,
    pub generic_params: Vec<InterfaceParameter>,
    pub requires: Vec<RuntimeInterface>,
    /// Declaration order, distinct from sorted pins in a type expression.
    pub associated_types: Vec<AssociatedType>,
    /// Metadata only. These do not grant native field access or host binding.
    pub fields: Vec<InterfaceField>,
    pub required_methods: Vec<InterfaceMethod>,
    pub default_methods: Vec<InterfaceMethod>,
    /// Caller surface resolved by the compiler, separately from the own
    /// declarations that define implementation obligations and default bodies.
    pub callers: InterfaceCallers,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InterfaceCallers {
    pub methods: Vec<InterfaceCaller>,
    /// A name requiring qualification cannot acquire an arbitrary native
    /// winner just because one requirement was enumerated first.
    pub ambiguous_methods: BTreeMap<baml_base::Name, Vec<RuntimeInterface>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceCaller {
    /// The compiler-selected declaring obligation, expressed in this root
    /// interface's frame. Unspecified associated pins remain unspecified.
    pub declaring_interface: RuntimeInterface,
    /// Substituted caller contract. Method-owned parameters are rebased after
    /// the root's Self/generic prefix, preserving their authored names.
    pub method: InterfaceMethod,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceParameter {
    /// Slot identity is relative to the containing declaration/call frame.
    /// The display name alone cannot distinguish shadowed type parameters.
    pub param: ParamTy,
    pub bounds: Vec<RuntimeInterface>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssociatedType {
    pub name: baml_base::Name,
    pub bound: Option<RuntimeInterface>,
    pub default: Option<RuntimeTy>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceField {
    pub name: baml_base::Name,
    pub ty: RuntimeTy,
    pub alias: Option<String>,
    pub description: Option<String>,
    pub docstring: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceMethod {
    pub name: baml_base::Name,
    /// Includes self when present, retaining optional/required argument modes.
    pub params: Vec<RuntimeFunctionParamTy>,
    pub return_type: RuntimeTy,
    pub declared_throws: Option<RuntimeTy>,
    pub callable_throws: RuntimeTy,
    /// Method-owned parameters, with their original flattened frame indices.
    pub generic_params: Vec<InterfaceParameter>,
    pub target: InterfaceMethodTarget,
    pub linkage: InterfaceMethodLinkage,
    pub builtin: Option<InterfaceBuiltin>,
    /// Set for declaration members. Implementation methods do not determine
    /// whether an existential caller can invoke the declared operation.
    pub callability: Option<InterfaceCallability>,
}

/// Callability belongs to the interface declaration, not an implementation's
/// potentially more specific signature. Backends must not re-derive BAML's
/// concrete-Self restrictions from their native type systems.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterfaceCallability {
    Existential,
    ConcreteSelf,
    Receiverless,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InterfaceMethodTarget {
    Free(Name),
    Method {
        class: Name,
        method: baml_base::Name,
    },
    Interface {
        interface: Name,
        method: baml_base::Name,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterfaceMethodLinkage {
    Linkable,
    ReservedBuiltin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterfaceBuiltin {
    Vm,
    Io,
    Intrinsic,
    AwaitAny,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceImplementation {
    /// Package defining the rule, including out-of-body implementations.
    pub package: baml_base::Name,
    pub interface: RuntimeInterface,
    pub receiver_pattern: RuntimeTy,
    pub generic_params: Vec<InterfaceParameter>,
    pub associated_types: Vec<(baml_base::Name, RuntimeTy)>,
    pub field_links: Vec<(baml_base::Name, baml_base::Name)>,
    /// Source provenance only; it does not control dispatch.
    pub enclosing_class: Option<Name>,
    pub methods: Vec<InterfaceMethod>,
}

/// Reject invalid compiler state at the export boundary instead of generating
/// an `unknown` API. The path identifies the declaration and affected member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceExportError {
    pub path: String,
    pub reason: String,
}

impl std::fmt::Display for InterfaceExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot export {}: {}", self.path, self.reason)
    }
}

impl std::error::Error for InterfaceExportError {}

/// Substitute one checked interface view before native-language projection.
/// Preserve parameter slot identity and unresolved projections; never widen them.
pub fn project_interface_type(
    ty: &RuntimeTy,
    declaration: &InterfaceDeclaration,
    parameters: &BTreeMap<ParamTy, SemanticTy>,
    associated: &BTreeMap<baml_base::Name, SemanticTy>,
) -> Result<crate::Ty, String> {
    let self_view = || {
        SemanticTy::Interface(
            declaration.name.clone(),
            declaration
                .generic_params
                .iter()
                .map(|p| parameters[&p.param].clone())
                .collect(),
            associated
                .iter()
                .map(|(name, ty)| (name.clone(), ty.clone()))
                .collect(),
            baml_base::TyAttr::EMPTY,
        )
    };
    let rewritten =
        baml_type::unify::rewrite_ty(&SemanticTy::from(ty.clone()), &mut |node| match node {
            SemanticTy::AssociatedTypeProjection {
                base,
                interface,
                member,
                ..
            } if matches!(base.as_ref(), SemanticTy::TypeVar(p, _) if p == &declaration.self_param)
                && interface.name == declaration.name =>
            {
                associated.get(member).cloned()
            }
            SemanticTy::TypeVar(param, _) if param == &declaration.self_param => Some(self_view()),
            SemanticTy::TypeVar(param, _) => parameters.get(param).cloned(),
            _ => None,
        });
    crate::Ty::try_from(&rewritten).map_err(|error| format!("{}: {error}", declaration.name))
}
