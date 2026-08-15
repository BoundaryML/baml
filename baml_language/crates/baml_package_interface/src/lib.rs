//! Serializable semantic boundary between a package and its dependents.

use std::collections::{BTreeMap, BTreeSet};

use baml_base::Name;
use baml_compiler2_ast::BuiltinKind;
use baml_type::{FunctionParamTy, Interface, ParamTy, QualifiedTypeName, Ty, TyAttr};
use rustc_hash::FxHashMap;

#[derive(Debug, Clone, PartialEq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PackageInterface {
    pub types: FxHashMap<Vec<Name>, FxHashMap<Name, ExportedType>>,
    pub functions: FxHashMap<Vec<Name>, FxHashMap<Name, ExportedFunction>>,
    pub impls: Vec<ExportedImpl>,
    pub throw_sets: FunctionThrowSets,
}

pub type GenericBounds = Vec<(ParamTy, Vec<Interface>)>;

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    borsh::BorshSerialize,
    borsh::BorshDeserialize,
)]
pub struct PackageItemId {
    pub package: Name,
    pub namespace: Vec<Name>,
    pub name: Name,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    borsh::BorshSerialize,
    borsh::BorshDeserialize,
)]
pub struct PackageMethodId {
    pub package: Name,
    pub namespace: Vec<Name>,
    pub class: Name,
    pub name: Name,
}

#[derive(Debug, Clone, PartialEq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum ExportedType {
    Class {
        qtn: QualifiedTypeName,
        fields: Vec<(Name, Ty)>,
        methods: Vec<ExportedFunction>,
        generic_params: Vec<ParamTy>,
        generic_bounds: GenericBounds,
    },
    Enum {
        qtn: QualifiedTypeName,
        variants: Vec<Name>,
    },
    TypeAlias {
        qtn: QualifiedTypeName,
        resolved: Ty,
    },
    Interface {
        qtn: QualifiedTypeName,
        frame: Vec<ParamTy>,
        generic_params: Vec<ParamTy>,
        generic_bounds: GenericBounds,
        requires: Vec<Interface>,
        fields: Vec<(Name, Ty)>,
        associated_types: Vec<ExportedAssociatedType>,
        methods: Vec<ExportedInterfaceMethod>,
    },
}

#[derive(Debug, Clone, PartialEq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct ExportedAssociatedType {
    pub name: Name,
    pub bound: Option<Ty>,
    pub default: Option<Ty>,
}

#[derive(Debug, Clone, PartialEq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct ExportedInterfaceMethod {
    pub name: Name,
    pub function_ty: Ty,
    pub generic_params: Vec<ParamTy>,
    pub generic_bounds: GenericBounds,
    pub default_impl: Option<ExportedFunction>,
}

#[derive(Debug, Clone, PartialEq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct ExportedImpl {
    pub interface: Interface,
    pub for_ty_pattern: Ty,
    pub generic_params: GenericBounds,
    pub associated_types: Vec<(Name, Ty)>,
    pub methods: Vec<ExportedImplMethod>,
}

#[derive(Debug, Clone, PartialEq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct ExportedImplMethod {
    pub name: Name,
    pub symbol: PackageMethodId,
}

#[derive(Debug, Clone, PartialEq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct ExportedFunction {
    pub name: Name,
    pub params: Vec<FunctionParamTy>,
    pub return_type: Ty,
    pub declared_throws: Option<Ty>,
    pub callable_throws: Ty,
    pub generic_params: Vec<ParamTy>,
    pub generic_bounds: GenericBounds,
    pub builtin_kind: Option<BuiltinKind>,
}

#[derive(Debug, Clone, PartialEq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct CallableThrowsFragment {
    pub by_id: BTreeMap<u32, Ty>,
}

pub type ThrowFact = Ty;

#[derive(Debug, Clone, Default, PartialEq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct FunctionThrowSets {
    pub direct: BTreeMap<Name, BTreeSet<ThrowFact>>,
    pub transitive: BTreeMap<Name, BTreeSet<ThrowFact>>,
}

impl PackageInterface {
    pub fn lookup_type(&self, namespace: &[Name], item: &Name) -> Option<&ExportedType> {
        self.types.get(namespace)?.get(item)
    }

    pub fn lookup_function(&self, namespace: &[Name], item: &Name) -> Option<&ExportedFunction> {
        self.functions.get(namespace)?.get(item)
    }
}

impl ExportedType {
    pub fn to_ty(&self) -> Ty {
        match self {
            Self::Class {
                qtn,
                generic_params,
                ..
            } => Ty::Class(
                qtn.clone(),
                generic_params
                    .iter()
                    .map(|param| Ty::TypeVar(param.clone(), TyAttr::default()))
                    .collect(),
                TyAttr::default(),
            ),
            Self::Enum { qtn, .. } => Ty::Enum(qtn.clone(), TyAttr::default()),
            Self::TypeAlias { qtn, .. } => Ty::TypeAlias(qtn.clone(), TyAttr::default()),
            Self::Interface {
                qtn,
                generic_params,
                ..
            } => Ty::Interface(
                qtn.clone(),
                generic_params
                    .iter()
                    .map(|param| Ty::TypeVar(param.clone(), TyAttr::default()))
                    .collect(),
                Vec::new(),
                TyAttr::default(),
            ),
        }
    }
}

impl FunctionThrowSets {
    pub fn transitive_for(&self, key: &Name) -> Option<&BTreeSet<ThrowFact>> {
        self.transitive.get(key)
    }
}

#[allow(unsafe_code)]
unsafe impl salsa::Update for PackageInterface {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        unsafe { update_if_changed(old_pointer, new_value) }
    }
}

#[allow(unsafe_code)]
unsafe impl salsa::Update for FunctionThrowSets {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        unsafe { update_if_changed(old_pointer, new_value) }
    }
}

#[allow(unsafe_code)]
unsafe fn update_if_changed<T: PartialEq>(old_pointer: *mut T, new_value: T) -> bool {
    let old = unsafe { &*old_pointer };
    if old == &new_value {
        false
    } else {
        unsafe {
            std::ptr::drop_in_place(old_pointer);
            std::ptr::write(old_pointer, new_value);
        }
        true
    }
}
