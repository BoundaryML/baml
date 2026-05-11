//! Type system BAML exposes for code-gen (this is both for streaming and non-streaming types).
//!
//! Types are fully resolved - no unresolved references. Class and Enum IDs
//! from VIR are resolved to their names during lowering.

use std::fmt;

/// Fully qualified type name carried by `Ty::Class`, `Ty::Enum`, and
/// `Ty::TypeAlias`.
///
/// The triple `(pkg, namespace_path, name)` mirrors `baml_compiler2_tir::QualifiedTypeName`
/// and is preserved end-to-end so language-specific code generators can route
/// each type to the correct module path (e.g. `vendor.<pkg>.…`, `baml.…`,
/// or the user package's root layout).
///
/// `$stream` companions live on the `name` field as a `…$stream` suffix —
/// there is no separate stream-types namespace.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Name {
    pub pkg: baml_base::Name,
    pub namespace_path: Vec<baml_base::Name>,
    pub name: baml_base::Name,
}

impl Name {
    pub fn new(
        pkg: baml_base::Name,
        namespace_path: Vec<baml_base::Name>,
        name: baml_base::Name,
    ) -> Self {
        Self {
            pkg,
            namespace_path,
            name,
        }
    }

    /// `true` if the bare name carries the `$stream` suffix.
    pub fn is_stream(&self) -> bool {
        self.name.as_str().ends_with("$stream")
    }

    /// The bare name without any `$stream` suffix.
    pub fn bare_name(&self) -> &str {
        self.name
            .as_str()
            .strip_suffix("$stream")
            .unwrap_or_else(|| self.name.as_str())
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.pkg)?;
        for seg in &self.namespace_path {
            write!(f, ".{seg}")?;
        }
        write!(f, ".{}", self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CallableParam {
    pub name: Option<baml_base::Name>,
    pub ty: Ty,
    pub mode: CodegenFunctionParamMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodegenFunctionParamMode {
    Required,
    Optional,
}

/// A resolved type in BAML.
///
/// Unlike `baml_compiler_vir::Ty` which may contain `Unknown`, `Error`, `Never` references,
/// this type is guaranteed to be valid.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    // Primitive types
    Int,
    Float,
    String,
    Bool,
    Null,
    Literal(baml_base::Literal),

    // Binary data
    Uint8Array,

    // Media types
    Media(baml_base::MediaKind),

    /// Class type with resolved name and concrete generic arguments.
    /// `generic_args` is empty for non-generic classes; otherwise it has one
    /// entry per declared `generic_params` slot, in order. Mirrors TIR's
    /// `Ty::Class(QualifiedTypeName, Vec<Ty>, TyAttr)`.
    Class(Name, Vec<Ty>),

    /// Enum type with resolved name.
    Enum(Name),

    /// Type alias with resolved name (used for recursive type aliases).
    TypeAlias(Name),

    /// A type-variable reference — `T` inside `class Box<T> { item T }` or
    /// `function deep_copy<T>(value: T) -> T`. Carries the bare `TypeVar`
    /// identifier; generators render it as a `typing.TypeVar`. Mirrors
    /// TIR's `Ty::TypeVar(Name, TyAttr)`.
    TypeVar(baml_base::Name),

    // Type constructors
    Optional(Box<Ty>),
    List(Box<Ty>),
    Map {
        key: Box<Ty>,
        value: Box<Ty>,
    },

    /// Assumes no unions within unions
    Union(Vec<Ty>),

    /// BAML's `unknown` type — the top type where every type is a subtype.
    /// Renders to `typing.Any` in Python.
    BuiltinUnknown,

    /// Callable type, e.g. `callable<[int, string], bool>`.
    Callable {
        params: Vec<CallableParam>,
        ret: Box<Ty>,
    },

    // Special types
    /// Void/Unit type - the type of effectful expressions.
    Unit,
    BamlOptions,
    /// Opaque Rust-managed state — `$rust_type` fields in stdlib stubs
    /// (e.g. `Response._body`, `SseStream._handle`). Generators render
    /// this as the host-language opaque-handle type (Python:
    /// `baml.baml_core.BamlPyHandle`).
    RustType,
}

impl Ty {
    pub(crate) fn validate(&self) -> Result<(), super::CodegenTypeError> {
        match self {
            Ty::BamlOptions => Ok(()),
            Ty::Int
            | Ty::Float
            | Ty::String
            | Ty::Bool
            | Ty::Uint8Array
            | Ty::Media(_)
            | Ty::Enum(_)
            | Ty::TypeAlias(_)
            | Ty::TypeVar(_)
            | Ty::RustType
            | Ty::BuiltinUnknown => Ok(()),
            Ty::Class(_, args) => args.iter().try_for_each(Ty::validate),
            Ty::Callable { params, ret } => {
                params.iter().try_for_each(|param| param.ty.validate())?;
                ret.validate()
            }
            Ty::Null => Ok(()),
            Ty::Optional(ty) => {
                ty.validate()?;
                if matches!(ty.as_ref(), Ty::Optional(_) | Ty::Null | Ty::Unit) {
                    Err(super::CodegenTypeError::InvalidOptionalUsage(self.clone()))
                } else {
                    Ok(())
                }
            }
            Ty::Literal(_) => Ok(()),
            Ty::List(ty) => {
                if matches!(ty.as_ref(), Ty::Unit) {
                    return Err(super::CodegenTypeError::InvalidUnit(*ty.clone()));
                }
                ty.validate()
            }
            Ty::Map { key, value } => {
                if !matches!(key.as_ref(), Ty::String | Ty::Enum(_)) {
                    return Err(super::CodegenTypeError::InvalidMapKey(*key.clone()));
                }
                if matches!(value.as_ref(), Ty::Unit) {
                    return Err(super::CodegenTypeError::InvalidUnit(*value.clone()));
                }
                value.validate()
            }
            Ty::Union(items) => {
                if items.is_empty() {
                    return Err(super::CodegenTypeError::InvalidUnionUsage(self.clone()));
                }
                items
                    .iter()
                    .map(Ty::validate)
                    .reduce(|acc, res| match (acc, res) {
                        (Ok(()), Ok(())) => Ok(()),
                        (Err(err), _) => Err(err),
                        (_, Err(err)) => Err(err),
                    })
                    .expect("Union is guaranteed to have atleast 1 item")?;

                // Check if any inner type is a union or a null, if so, nope
                if items
                    .iter()
                    .any(|ty| matches!(ty, Ty::Union(_) | Ty::Optional(_) | Ty::Null | Ty::Unit))
                {
                    Err(super::CodegenTypeError::InvalidUnionUsage(self.clone()))
                } else {
                    Ok(())
                }
            }
            Ty::Unit => Ok(()),
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::BamlOptions => write!(f, "baml.Options"),
            Ty::Int => write!(f, "int"),
            Ty::Float => write!(f, "float"),
            Ty::String => write!(f, "string"),
            Ty::Bool => write!(f, "bool"),
            Ty::Null => write!(f, "null"),
            Ty::Uint8Array => write!(f, "uint8array"),
            Ty::Media(kind) => write!(f, "{kind}"),
            Ty::Class(name, args) => {
                if args.is_empty() {
                    write!(f, "{name}")
                } else {
                    let parts: Vec<std::string::String> =
                        args.iter().map(std::string::ToString::to_string).collect();
                    write!(f, "{name}<{}>", parts.join(", "))
                }
            }
            Ty::Enum(name) | Ty::TypeAlias(name) => write!(f, "{name}"),
            Ty::TypeVar(name) => write!(f, "{name}"),
            Ty::Optional(inner) => write!(f, "{inner}?"),
            Ty::List(inner) => write!(f, "{inner}[]"),
            Ty::Map { key, value } => write!(f, "map<{key}, {value}>"),
            Ty::Union(types) => {
                let parts: Vec<std::string::String> =
                    types.iter().map(std::string::ToString::to_string).collect();
                write!(f, "({})", parts.join(" | "))
            }
            Ty::RustType => write!(f, "$rust_type"),
            Ty::Literal(lit) => match lit {
                baml_base::Literal::Int(v) => write!(f, "int({v})"),
                baml_base::Literal::Float(s) => write!(f, "float({s})"),
                baml_base::Literal::String(v) => write!(f, "string({v:?})"),
                baml_base::Literal::Bool(v) => write!(f, "bool({v})"),
            },
            Ty::Unit => write!(f, "void"),
            Ty::BuiltinUnknown => write!(f, "unknown"),
            Ty::Callable { params, ret } => {
                let param_strs: Vec<std::string::String> = params
                    .iter()
                    .map(|param| {
                        let ty = &param.ty;
                        match (&param.name, param.mode) {
                            (Some(name), CodegenFunctionParamMode::Optional) => {
                                format!("{name}?: {ty}")
                            }
                            (Some(name), CodegenFunctionParamMode::Required) => {
                                format!("{name}: {ty}")
                            }
                            (None, _) => ty.to_string(),
                        }
                    })
                    .collect();
                write!(f, "callable<[{}], {ret}>", param_strs.join(", "))
            }
        }
    }
}
