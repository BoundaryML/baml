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

/// Context for rendering types.
///
/// `self_ref_name` is `Some(name)` when rendering the RHS of a TypeAlias
/// definition.  Any `Class` or `TypeAlias` reference whose rendered name
/// matches `self_ref_name` will be wrapped in a forward-reference string
/// literal to avoid a `NameError` at module load time.
#[derive(Debug, Clone)]
pub(crate) struct RenderCtx {
    pub(crate) ns: Namespace,
    /// When `Some`, any name that renders to this string will be quoted.
    pub(crate) self_ref_name: Option<String>,
}

impl RenderCtx {
    pub(crate) fn for_namespace(ns: Namespace) -> Self {
        RenderCtx {
            ns,
            self_ref_name: None,
        }
    }

    /// Build a context for rendering inside a TypeAlias definition.
    /// Any reference to `alias_name` will be quoted.
    pub(crate) fn for_type_alias(ns: Namespace, alias_name: String) -> Self {
        RenderCtx {
            ns,
            self_ref_name: Some(alias_name),
        }
    }
}

impl Name {
    pub(crate) fn render(&self, ns: Namespace) -> String {
        if self.namespace == ns {
            self.name.to_string()
        } else {
            format!("{}.{}", self.namespace, self.name)
        }
    }

    /// Render with `RenderCtx` - same as `render(ctx.ns)`.
    pub(crate) fn render_ctx(&self, ctx: &RenderCtx) -> String {
        self.render(ctx.ns)
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
                Namespace::BamlPy => "baml",
                Namespace::Types => "baml_types",
                Namespace::StreamTypes => "baml_stream_types",
            }
        )
    }
}

impl Name {
    pub(crate) fn from_codegen_types(name: &baml_codegen_types::Name) -> Self {
        // Strip the `$stream` suffix — stream types live in their own module
        // so they can keep the original user-defined name.
        let bare_name = baml_base::Name::new(name.bare_name());
        Name {
            name: bare_name,
            namespace: Namespace::for_codegen_name(name),
        }
    }
}

impl Namespace {
    /// Pick `Types` vs `StreamTypes` based on whether the codegen `Name`
    /// carries a `$stream` suffix. Phase 5 only changes the IR shape we
    /// consume — the emitter still routes objects to the legacy
    /// `baml_types/` vs `baml_stream_types/` trees.
    pub(crate) fn for_codegen_name(name: &baml_codegen_types::Name) -> Self {
        if name.is_stream() {
            Self::StreamTypes
        } else {
            Self::Types
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

    // Binary data
    Uint8Array,

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

    /// Type alias reference — renders as the bare alias name.
    TypeAlias(Name),

    // Type constructors
    Optional(Box<Ty>),
    List(Box<Ty>),
    Map {
        key: Box<Ty>,
        value: Box<Ty>,
    },

    /// Assumes no unions within unions
    Union(Vec<Ty>),

    // Any type
    Any,

    /// Callable type: `typing.Callable[[P1, P2, ...], Ret]`
    Callable {
        params: Vec<Ty>,
        ret: Box<Ty>,
    },

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
            baml_codegen_types::Ty::Uint8Array => Self::Uint8Array,
            baml_codegen_types::Ty::Media(kind) => match kind {
                baml_base::MediaKind::Image => Self::Image,
                baml_base::MediaKind::Video => Self::Video,
                baml_base::MediaKind::Audio => Self::Audio,
                baml_base::MediaKind::Pdf => Self::Pdf,
                baml_base::MediaKind::Generic => Self::Any,
            },
            baml_codegen_types::Ty::Class(name) => Self::Class(Name::from_codegen_types(name)),
            baml_codegen_types::Ty::Enum(name) => Self::Enum(Name::from_codegen_types(name)),
            // TypeAlias: render as a proper type alias reference.
            baml_codegen_types::Ty::TypeAlias(name) => {
                Self::TypeAlias(Name::from_codegen_types(name))
            }
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
            // BuiltinUnknown (BAML's `unknown` type) renders to Python's `typing.Any`.
            baml_codegen_types::Ty::BuiltinUnknown => Self::Any,
            // Callable: render as `typing.Callable[[params...], ret]`.
            baml_codegen_types::Ty::Callable { params, ret } => Self::Callable {
                params: params.iter().map(Self::from_codegen_types).collect(),
                ret: Box::new(Self::from_codegen_types(ret)),
            },
        }
    }
}

impl Ty {
    pub(crate) fn render(&self, ns: Namespace) -> String {
        self.render_with_ctx(&RenderCtx::for_namespace(ns))
    }

    pub(crate) fn render_with_ctx(&self, ctx: &RenderCtx) -> String {
        match self {
            Ty::Int => "int".to_string(),
            Ty::Float => "float".to_string(),
            Ty::String => "str".to_string(),
            Ty::Bool => "bool".to_string(),
            Ty::None => "None".to_string(),
            Ty::Uint8Array => "bytes".to_string(),
            Ty::Literal(lit) => format!("typing.Literal[{lit}]"),
            Ty::Image => format!("{}Image", Namespace::BamlPy.render(ctx.ns)),
            Ty::Video => format!("{}Video", Namespace::BamlPy.render(ctx.ns)),
            Ty::Audio => format!("{}Audio", Namespace::BamlPy.render(ctx.ns)),
            Ty::Pdf => format!("{}Pdf", Namespace::BamlPy.render(ctx.ns)),
            Ty::Class(name) => {
                let rendered = name.render_ctx(ctx);
                // Inside a TypeAlias definition, quote self-references to avoid NameError.
                if let Some(ref self_name) = ctx.self_ref_name {
                    if &rendered == self_name {
                        return format!("{rendered:?}");
                    }
                }
                rendered
            }
            Ty::Enum(name) => name.render_ctx(ctx),
            Ty::TypeAlias(name) => {
                let rendered = name.render_ctx(ctx);
                // Inside a TypeAlias definition, quote self-references to avoid NameError.
                if let Some(ref self_name) = ctx.self_ref_name {
                    if &rendered == self_name {
                        return format!("{rendered:?}");
                    }
                }
                rendered
            }
            Ty::Optional(inner) => {
                format!("typing.Optional[{}]", inner.render_with_ctx(ctx))
            }
            Ty::List(inner) => format!("typing.List[{}]", inner.render_with_ctx(ctx)),
            Ty::Map { key, value } => {
                format!(
                    "typing.Dict[{}, {}]",
                    key.render_with_ctx(ctx),
                    value.render_with_ctx(ctx)
                )
            }
            Ty::Union(types) => format!(
                "typing.Union[{}]",
                types
                    .iter()
                    .map(|ty| ty.render_with_ctx(ctx))
                    .collect::<Vec<String>>()
                    .join(", ")
            ),
            Ty::Any => "typing.Any".to_string(),
            Ty::Callable { params, ret } => {
                let params_str = params
                    .iter()
                    .map(|p| p.render_with_ctx(ctx))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "typing.Callable[[{}], {}]",
                    params_str,
                    ret.render_with_ctx(ctx)
                )
            }
            Ty::BamlOptions => "baml.Options".to_string(),
            Ty::Stream {
                stream_type,
                return_type,
            } => format!(
                "{}Stream[{}, {}]",
                Namespace::StreamTypes.render(ctx.ns),
                stream_type.render_with_ctx(ctx),
                return_type.render_with_ctx(ctx)
            ),
        }
    }
}
