use std::collections::HashSet;

use crate::{CodegenTypeError, Ty};

pub type SymbolPool = std::collections::HashMap<super::Name, Symbol>;

pub enum Symbol {
    Function(Function),
    Class(Class),
    Enum(Enum),
    TypeAlias(TypeAlias),
}

/// Where the symbol was defined in user BAML source.
#[derive(Clone, Debug)]
pub struct Origin {
    /// Path of the defining `.baml` file, relative to the
    /// `baml_src/` root. Same string form as keys in
    /// `_inlinedbaml.py`'s FILES dict.
    pub source_file_path: String,
    /// Byte offset of the symbol's definition start within that
    /// file. Used as the secondary sort key when ordering symbols
    /// inside an emitter leaf.
    pub span_start: u32,
}

pub struct Function {
    pub name: baml_base::Name,
    /// `TypeVar`s declared on this function. Empty for non-generic functions.
    /// Mirrors AST `FunctionDef.generic_params`. Inner `Ty::TypeVar`
    /// references in `arguments` / `return_type` resolve against this list.
    pub generic_params: Vec<baml_base::Name>,
    pub docstring: Option<String>,
    pub arguments: Vec<FunctionArgument>,
    pub return_type: super::Ty,

    // TODO: add other APIs here that impact code-gen
    pub watchers: Vec<(baml_base::Name, super::Ty)>,

    /// Source-origin info: the defining `.baml` file and byte span start.
    /// Used by the emitter to order symbols deterministically within a leaf.
    pub origin: Origin,
}

pub struct FunctionArgument {
    pub name: baml_base::Name,
    pub docstring: Option<String>,
    pub ty: super::Ty,
    pub default: Option<FunctionArgumentDefault>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FunctionArgumentDefault {
    Null,
    Literal(DefaultLiteral),
    Expression { source: Option<String> },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DefaultLiteral {
    Scalar(baml_base::Literal),
    EmptyList,
    EmptyMap,
}

pub struct Class {
    pub name: super::Name,
    /// `TypeVar`s declared on this class. Empty for non-generic classes.
    /// Mirrors AST `ClassDef.generic_params`. Inner `Ty::TypeVar`
    /// references in `properties` / methods resolve against this list.
    pub generic_params: Vec<baml_base::Name>,
    pub docstring: Option<String>,
    pub properties: Vec<ClassProperty>,
    /// Static methods on this class. Source-declaration order is
    /// preserved by the pool builder; the emitter sorts at fan-out
    /// time. Static vs. instance is encoded structurally — kind is
    /// implied by which vec the method lives in.
    pub static_methods: Vec<Function>,
    /// Instance methods on this class. The receiver (`self`) is **not**
    /// in `Function::arguments` — it's a Python convention prepended at
    /// render time.
    pub instance_methods: Vec<Function>,
    pub origin: Origin,
}

pub struct ClassProperty {
    pub name: baml_base::Name,
    pub docstring: Option<String>,
    pub ty: super::Ty,
}

pub struct Enum {
    pub name: super::Name,
    pub docstring: Option<String>,
    pub variants: Vec<EnumVariant>,
    pub origin: Origin,
}

pub struct EnumVariant {
    pub name: baml_base::Name,
    pub docstring: Option<String>,
    pub value: String,
}

pub struct TypeAlias {
    pub name: super::Name,
    pub resolves_to: super::Ty,
    /// Whether this type alias is recursive (i.e., references itself).
    pub recursive: bool,
    pub origin: Origin,
}

impl Symbol {
    pub fn validate(&self) -> Result<(), CodegenTypeError> {
        match self {
            Symbol::Function(function) => function.validate(),
            Symbol::Class(class) => class.validate(),
            Symbol::Enum(_) => Ok(()),
            Symbol::TypeAlias(type_alias) => type_alias.validate(),
        }
    }

    pub fn walk_all_unions(&self) -> HashSet<super::Ty> {
        match self {
            Symbol::Function(function) => function.walk_all_unions(),
            Symbol::Class(class) => class.walk_all_unions(),
            Symbol::Enum(_) => HashSet::default(),
            Symbol::TypeAlias(type_alias) => type_alias.walk_all_unions(),
        }
    }
}

impl Function {
    fn validate(&self) -> Result<(), CodegenTypeError> {
        self.arguments
            .iter()
            .map(|args| args.ty.validate())
            .collect::<Result<Vec<_>, _>>()?;
        self.return_type.validate()?;

        Ok(())
    }

    fn walk_all_unions(&self) -> HashSet<super::Ty> {
        self.arguments
            .iter()
            .flat_map(|args| args.ty.walk_all_unions().into_iter())
            .chain(self.return_type.walk_all_unions())
            .collect()
    }
}

impl Class {
    fn validate(&self) -> Result<(), CodegenTypeError> {
        self.properties
            .iter()
            .map(|prop| prop.ty.validate())
            .collect::<Result<Vec<_>, _>>()?;
        for method in self.static_methods.iter().chain(&self.instance_methods) {
            method.validate()?;
        }
        Ok(())
    }

    fn walk_all_unions(&self) -> HashSet<super::Ty> {
        self.properties
            .iter()
            .flat_map(|prop| prop.ty.walk_all_unions().into_iter())
            .chain(
                self.static_methods
                    .iter()
                    .chain(&self.instance_methods)
                    .flat_map(|m| m.walk_all_unions().into_iter()),
            )
            .collect::<_>()
    }
}

impl TypeAlias {
    fn validate(&self) -> Result<(), CodegenTypeError> {
        self.resolves_to.validate()
    }

    fn walk_all_unions(&self) -> HashSet<super::Ty> {
        self.resolves_to.walk_all_unions()
    }
}

impl super::Ty {
    pub fn walk_all_unions(&self) -> HashSet<super::Ty> {
        let mut unions = HashSet::<Ty>::default();
        if matches!(self, Ty::Union(_)) {
            unions.insert(self.clone());
        }

        match self {
            Ty::Int
            | Ty::Float
            | Ty::String
            | Ty::Bool
            | Ty::Null
            | Ty::Unit
            | Ty::Uint8Array
            | Ty::Media(_)
            | Ty::Enum(_)
            | Ty::TypeAlias(_)
            | Ty::TypeVar(_)
            | Ty::RustType
            | Ty::BuiltinUnknown
            // Unions are guaranteed to not have unions thanks to .validate()
            | Ty::Union(_)
            | Ty::Literal(_) => {}
            Ty::Class(_, args) => {
                for a in args {
                    unions.extend(a.walk_all_unions());
                }
            }
            Ty::Optional(ty) | Ty::List(ty) | Ty::Map { key: _, value: ty } => {
                unions.extend(ty.walk_all_unions());
            }
            Ty::Callable { params, ret } => {
                for p in params {
                    unions.extend(p.ty.walk_all_unions());
                }
                unions.extend(ret.walk_all_unions());
            }
            Ty::BamlOptions => {}
        }

        unions
    }
}
