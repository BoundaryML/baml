//! Type system BAML exposes for code-gen (this is both for streaming and non-streaming types).
//!
//! Types are fully resolved - no unresolved references. Class and Enum IDs
//! from VIR are resolved to their names during lowering.

use crate::docstring::PyString;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
pub(crate) enum Namespace {
    Other,
    BamlPy,
    Types,
    StreamTypes,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Name {
    name: baml_base::Name,
    namespace: Namespace,
}

impl Name {
    pub(crate) fn render(&self, ns: Namespace) -> String {
        if self.namespace == ns {
            self.name.to_string()
        } else {
            format!("{}.{}", self.namespace, self.name)
        }
    }
}

impl Namespace {
    pub(crate) fn render(self, ns: Namespace) -> String {
        if self == ns || self == Namespace::Other {
            String::new()
        } else {
            format!("{self}.")
        }
    }
}

impl std::fmt::Display for Namespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Namespace::Other => "<other (this shouldn't happen. please report this as a bug)>",
                Namespace::BamlPy => "baml_py",
                Namespace::Types => "types",
                Namespace::StreamTypes => "stream_types",
            }
        )
    }
}

impl Name {
    pub(crate) fn from_codegen_types(name: &baml_codegen_types::Name) -> Self {
        Name {
            name: name.name.clone(),
            namespace: Namespace::from_codegen_types(&name.namespace),
        }
    }
}

impl Namespace {
    pub(crate) fn from_codegen_types(namespace: &baml_codegen_types::Namespace) -> Self {
        match namespace {
            baml_codegen_types::Namespace::Types => Self::Types,
            baml_codegen_types::Namespace::StreamTypes => Self::StreamTypes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Literal {
    Int(i64),
    Float(String),
    String(PyString),
    Bool(bool),
}

impl Literal {
    pub(crate) fn from_codegen_types(lit: &baml_base::Literal) -> Self {
        match lit {
            baml_base::Literal::Int(v) => Self::Int(*v),
            baml_base::Literal::Float(s) => Self::Float(s.clone()),
            baml_base::Literal::String(v) => Self::String(PyString::new(v)),
            baml_base::Literal::Bool(v) => Self::Bool(*v),
        }
    }
}

impl std::fmt::Display for Literal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Literal::Int(v) => write!(f, "{v}"),
            Literal::Float(s) => write!(f, "{s}"),
            Literal::String(v) => write!(f, "{v}"),
            Literal::Bool(true) => write!(f, "True"),
            Literal::Bool(false) => write!(f, "False"),
        }
    }
}

/// A resolved type in BAML.
///
/// Unlike `baml_compiler_vir::Ty` which may contain `Unknown`, `Error`, `Never` references,
/// this type is guaranteed to be valid.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Ty {
    // Primitive types
    Int,
    Float,
    String,
    Bool,
    None,

    // Media types are top level types in python
    Image,
    Audio,
    Pdf,
    Video,

    Literal(Literal),

    /// Class type with resolved name.
    Class(Name),

    /// Enum type with resolved name.
    Enum(Name),

    // Type constructors
    Optional(Box<Ty>),
    List(Box<Ty>),
    Map {
        key: Box<Ty>,
        value: Box<Ty>,
    },
    Checked(Box<Ty>, Vec<String>),
    StreamState(Box<Ty>),

    /// Assumes no unions within unions
    Union(Vec<Ty>),

    // Any type
    Any,

    // Special types
    BamlOptions,
    Stream {
        stream_type: Box<Ty>,
        return_type: Box<Ty>,
    },
}

impl Ty {
    pub(crate) fn from_codegen_types(ty: &baml_codegen_types::Ty) -> Self {
        match ty {
            baml_codegen_types::Ty::Literal(lit) => Self::Literal(Literal::from_codegen_types(lit)),
            baml_codegen_types::Ty::Unit => Self::None,
            baml_codegen_types::Ty::Int => Self::Int,
            baml_codegen_types::Ty::Float => Self::Float,
            baml_codegen_types::Ty::String => Self::String,
            baml_codegen_types::Ty::Bool => Self::Bool,
            baml_codegen_types::Ty::Null => Self::None,
            baml_codegen_types::Ty::Media(kind) => match kind {
                baml_base::MediaKind::Image => Self::Image,
                baml_base::MediaKind::Video => Self::Video,
                baml_base::MediaKind::Audio => Self::Audio,
                baml_base::MediaKind::Pdf => Self::Pdf,
                baml_base::MediaKind::Generic => Self::Any,
            },
            baml_codegen_types::Ty::Class(name) => Self::Class(Name::from_codegen_types(name)),
            baml_codegen_types::Ty::Enum(name) => Self::Enum(Name::from_codegen_types(name)),
            baml_codegen_types::Ty::Optional(inner) => {
                Self::Optional(Box::new(Self::from_codegen_types(inner)))
            }
            baml_codegen_types::Ty::List(inner) => {
                Self::List(Box::new(Self::from_codegen_types(inner)))
            }
            baml_codegen_types::Ty::Map { key, value } => Self::Map {
                key: Box::new(Self::from_codegen_types(key)),
                value: Box::new(Self::from_codegen_types(value)),
            },
            baml_codegen_types::Ty::Union(types) => {
                Self::Union(types.iter().map(Self::from_codegen_types).collect())
            }
            baml_codegen_types::Ty::BamlOptions => Self::BamlOptions,
            baml_codegen_types::Ty::Checked(inner, checks) => {
                Self::Checked(Box::new(Self::from_codegen_types(inner)), checks.clone())
            }
            baml_codegen_types::Ty::StreamState(inner) => {
                Self::StreamState(Box::new(Self::from_codegen_types(inner)))
            }
        }
    }
}

impl Ty {
    pub(crate) fn render(&self, ns: Namespace) -> String {
        match self {
            Ty::Int => "int".to_string(),
            Ty::Float => "float".to_string(),
            Ty::String => "str".to_string(),
            Ty::Bool => "bool".to_string(),
            Ty::None => "None".to_string(),
            Ty::Literal(lit) => format!("typing.Literal[{lit}]"),
            Ty::Image => format!("{}Image", Namespace::BamlPy.render(ns)),
            Ty::Video => format!("{}Video", Namespace::BamlPy.render(ns)),
            Ty::Audio => format!("{}Audio", Namespace::BamlPy.render(ns)),
            Ty::Pdf => format!("{}Pdf", Namespace::BamlPy.render(ns)),
            Ty::Class(name) => name.render(ns),
            Ty::Enum(name) => name.render(ns),
            Ty::Optional(inner) => format!("typing.Optional[{}]", inner.render(ns)),
            Ty::List(inner) => format!("typing.List[{}]", inner.render(ns)),
            Ty::Map { key, value } => {
                format!("typing.Dict[{}, {}]", key.render(ns), value.render(ns))
            }
            Ty::Union(types) => format!(
                "typing.Union[{}]",
                types
                    .iter()
                    .map(|ty| ty.render(ns))
                    .collect::<Vec<String>>()
                    .join(", ")
            ),
            Ty::Checked(inner, checks) => format!(
                "{}Checked[{}, {}]",
                Namespace::Types.render(ns),
                inner.render(ns),
                checks.join(", ")
            ),
            Ty::StreamState(inner) => format!(
                "{}StreamState[{}]",
                Namespace::StreamTypes.render(ns),
                inner.render(ns)
            ),
            Ty::Any => "typing.Any".to_string(),
            Ty::BamlOptions => "baml.Options".to_string(),
            Ty::Stream {
                stream_type,
                return_type,
            } => format!(
                "{}Stream[{}, {}]",
                Namespace::StreamTypes.render(ns),
                stream_type.render(ns),
                return_type.render(ns)
            ),
        }
    }
}
