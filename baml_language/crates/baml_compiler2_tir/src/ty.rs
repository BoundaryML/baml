//! Resolved type representation — the output of type resolution.

use std::fmt;

use baml_base::Name;

/// A qualified type name with separate package and local name.
///
/// Used in `Ty::Class`, `Ty::Enum`, and `Ty::TypeAlias` to unambiguously
/// identify a type by its definition's package (e.g. `"user"`, `"baml"`)
/// and its short name (e.g. `"Foo"`, `"PrimitiveClient"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QualifiedTypeName {
    /// The package this type is defined in (e.g. `"user"`, `"baml"`).
    pub pkg: Name,
    /// The short/local name of the type (e.g. `"Foo"`).
    pub name: Name,
}

impl QualifiedTypeName {
    pub fn new(pkg: Name, name: Name) -> Self {
        Self { pkg, name }
    }
}

impl fmt::Display for QualifiedTypeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.pkg, self.name)
    }
}

/// Resolved type — the output of type resolution (Pass 2).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    /// A class type — just the name, no expansion.
    Class(QualifiedTypeName),
    /// An enum type.
    Enum(QualifiedTypeName),
    /// An enum variant — Enum(qualified) . Variant(name).
    EnumVariant(QualifiedTypeName, Name),
    /// A type alias — opaque name reference, NOT expanded.
    /// Expansion happens lazily at subtype-checking time.
    TypeAlias(QualifiedTypeName),
    /// Primitive types.
    Primitive(PrimitiveType),
    /// T[]
    List(Box<Ty>),
    /// map<K, V>
    Map(Box<Ty>, Box<Ty>),
    /// A | B | C
    Union(Vec<Ty>),
    /// T?
    Optional(Box<Ty>),
    /// Literal string/int/bool as a type.
    ///
    /// Carries a `Freshness` flag modeled after TypeScript's fresh/regular
    /// literal types. Fresh literals (from expressions) widen to their base
    /// primitive at mutable binding sites. Regular literals (from type
    /// annotations or contextual typing) are preserved.
    Literal(baml_base::Literal, Freshness),
    /// Evolving list — created from empty array literal `[]` at mutable
    /// binding sites (via `make_evolving()`). Element type starts as `Never`
    /// and is refined by mutations (`.push()`, index assignment).
    ///
    /// Reading the variable in expression position produces the fixed
    /// `List(T)` type; the local entry stays `EvolvingList` so further
    /// mutations still work.
    ///
    /// Parallel to `Freshness` on literals: `make_evolving()` is the mirror
    /// of `widen_fresh()` — both called at `let` binding sites without
    /// type annotations.
    ///
    /// # Two parallel paths for container mutations
    ///
    /// There are two ways container method calls (e.g. `.push()`) are resolved:
    ///
    /// 1. **Evolving path** (`try_container_method_call` in `builder.rs`): For
    ///    mutable locals, `.push(x)` is intercepted *before* normal method
    ///    resolution. It widens the element type in-place (e.g. `EvolvingList(Never)`
    ///    → `EvolvingList(int)`) and returns `Void`. This path takes priority.
    ///
    /// 2. **Builtin method path** (`resolve_builtin_method` in `builder.rs`): For
    ///    typed arrays (e.g. `let arr: int[] = ...`), `.push(x)` is resolved via
    ///    the `Array<T>` class declared in `baml_builtins2/baml/containers.baml`.
    ///    The type checker bridges `Ty::List(int)` → `Array<int>`, binds `T = int`,
    ///    and type-checks the call against the method signature. This path does NOT
    ///    widen — the element type is already known.
    ///
    /// The evolving path exists because empty containers (`[]`, `{}`) need
    /// flow-sensitive type refinement that the static builtin signatures can't
    /// express. Once an evolving container is read, it freezes to a normal
    /// `List`/`Map` and subsequent method calls go through the builtin path.
    EvolvingList(Box<Ty>),
    /// Evolving map — created from empty map literal at mutable binding sites.
    /// Same semantics as `EvolvingList` but for maps (see doc on `EvolvingList`).
    EvolvingMap(Box<Ty>, Box<Ty>),
    /// Function type: (params) -> return.
    Function {
        params: Vec<(Option<Name>, Ty)>,
        ret: Box<Ty>,
    },
    /// The bottom type — expression never produces a value.
    /// Assigned to `return`, `break`, `continue`, and blocks that always diverge.
    /// `Never` is a subtype of every type: `join(Never, T) = T`.
    Never,
    /// The void type — produced by statements and expressions that don't yield
    /// a useful value (e.g. `if` without `else`, bare function calls used as
    /// statements, `while` loops).
    ///
    /// `Void` is **not** a subtype of any other type. Consuming a `Void` value
    /// (assigning it, passing it as an argument, returning it) is a type error.
    /// In statement position the value is simply discarded.
    ///
    /// Analogous to TypeScript's fresh-object excess-property check pattern:
    /// the type is valid only when nobody reads the value.
    Void,
    /// The explicit `unknown` keyword type — a top type (supertype of everything).
    ///
    /// Any `T` is a subtype of `BuiltinUnknown`, but `BuiltinUnknown` is NOT a
    /// subtype of any specific type. Analogous to TypeScript's `unknown`.
    ///
    /// Used in builtin function signatures where any value may be accepted, e.g.:
    /// ```baml
    /// function render_prompt(function_name: string, args: map<string, unknown>) -> PromptAst
    /// ```
    ///
    /// NOTE: This is **distinct** from `Ty::Unknown` which is the error-recovery
    /// sentinel meaning "type inference failed". `BuiltinUnknown` is a well-formed
    /// type that appears in valid programs; `Unknown` signals a compiler error.
    BuiltinUnknown,
    /// Opaque Rust-managed state.
    ///
    /// Used for `$rust_type` fields in builtin class stubs (e.g., `Media._data`,
    /// `Response._body`). The containing class is non-constructable from user code.
    /// Fields of this type cannot be accessed directly from BAML code.
    ///
    /// This is distinct from `Ty::Unknown` (which means "type inference failed") —
    /// `RustType` is intentional and well-formed in the builtin stubs.
    RustType,
    /// Error recovery — the type is structurally unknown (e.g., name unresolved).
    Unknown,
    /// Error sentinel — a hard error was emitted for this expression.
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PrimitiveType {
    Int,
    Float,
    String,
    Bool,
    Null,
    Image,
    Audio,
    Video,
    Pdf,
}

impl PrimitiveType {
    /// Map media primitives to their builtin class path in the `baml` package.
    ///
    /// Each media primitive (`image`, `audio`, `video`, `pdf`) has a
    /// corresponding class declared in `baml_builtins2/baml_std/baml/media.baml`
    /// (e.g. `class Image { ... }`). The file is at `<builtin>/baml/media.baml`
    /// which routes to the `baml.media` namespace, so the lookup path is
    /// `&["media", "Image"]` etc.
    pub fn builtin_class_path(&self) -> &'static [&'static str] {
        match self {
            Self::Image => &["media", "Image"],
            Self::Audio => &["media", "Audio"],
            Self::Video => &["media", "Video"],
            Self::Pdf => &["media", "Pdf"],
            other => panic!("{other:?} is not a media primitive with a builtin class"),
        }
    }

    pub fn from_literal(lit: &baml_base::Literal) -> Self {
        match lit {
            baml_base::Literal::Int(_) => Self::Int,
            baml_base::Literal::Float(_) => Self::Float,
            baml_base::Literal::String(_) => Self::String,
            baml_base::Literal::Bool(_) => Self::Bool,
        }
    }
}

/// Freshness flag for literal types.
///
/// Modeled after TypeScript's fresh/regular literal type distinction.
/// - **Fresh**: produced by literal expressions (`1`, `"hello"`). Widens to
///   the base primitive at mutable binding sites (`let x = 1` → `int`).
/// - **Regular**: produced by type annotations (`let x: 1 = 1`) or contextual
///   typing. Preserved through mutable bindings.
///
/// Freshness is **ignored** by the subtype checker — `Literal(1, Fresh)` and
/// `Literal(1, Regular)` are structurally identical for assignability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Freshness {
    Fresh,
    Regular,
}

/// Re-export `baml_base::Literal` as `LiteralValue` for backward compatibility.
pub type LiteralValue = baml_base::Literal;

impl Ty {
    /// Widen fresh literal types to their base primitive.
    ///
    /// Called at mutable binding sites (`let` without annotation).
    /// Regular (non-fresh) literals pass through unchanged.
    pub fn widen_fresh(self) -> Ty {
        match self {
            Ty::Literal(lit, Freshness::Fresh) => Ty::Primitive(PrimitiveType::from_literal(&lit)),
            other => other,
        }
    }

    /// Promote empty containers to evolving containers.
    ///
    /// Called at mutable binding sites (`let` without annotation), right
    /// after `widen_fresh()`. This is the mirror of `widen_fresh()`:
    /// - `widen_fresh` *removes* literal specificity (1 → int)
    /// - `make_evolving` *adds* container mutability (List(Never) → EvolvingList(Never))
    ///
    /// Only converts `List(Never)` and `Map(Never, Never)` — non-empty
    /// container literals already have a known element type and don't need
    /// evolving semantics.
    pub fn make_evolving(self) -> Ty {
        match self {
            Ty::List(inner) if matches!(*inner, Ty::Never) => Ty::EvolvingList(inner),
            Ty::Map(k, v) if matches!(*k, Ty::Never) && matches!(*v, Ty::Never) => {
                Ty::EvolvingMap(k, v)
            }
            other => other,
        }
    }
}

// ── Display impls ────────────────────────────────────────────────────────────

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Class(qn) => write!(f, "{qn}"),
            Ty::Enum(qn) => write!(f, "{qn}"),
            Ty::EnumVariant(qn, v) => write!(f, "{qn}.{v}"),
            Ty::TypeAlias(qn) => write!(f, "{qn}"),
            Ty::Primitive(p) => write!(f, "{p}"),
            Ty::List(inner) => write!(f, "{inner}[]"),
            Ty::Map(k, v) => write!(f, "map<{k}, {v}>"),
            Ty::EvolvingList(inner) => {
                if matches!(**inner, Ty::Never) {
                    write!(f, "_[]")
                } else {
                    write!(f, "{inner}[] (evolving)")
                }
            }
            Ty::EvolvingMap(k, v) => {
                if matches!(**k, Ty::Never) && matches!(**v, Ty::Never) {
                    write!(f, "map<_, _>")
                } else {
                    write!(f, "map<{k}, {v}> (evolving)")
                }
            }
            Ty::Union(members) => {
                let parts: Vec<_> = members.iter().map(|m| m.to_string()).collect();
                write!(f, "{}", parts.join(" | "))
            }
            Ty::Optional(inner) => write!(f, "{inner}?"),
            Ty::Literal(lit, _freshness) => write!(f, "{lit}"),
            Ty::Function { params, ret } => {
                let ps: Vec<String> = params
                    .iter()
                    .map(|(name, ty)| {
                        name.as_ref()
                            .map(|n| format!("{n}: {ty}"))
                            .unwrap_or_else(|| ty.to_string())
                    })
                    .collect();
                write!(f, "({}) -> {ret}", ps.join(", "))
            }
            Ty::Never => write!(f, "never"),
            Ty::Void => write!(f, "void"),
            Ty::BuiltinUnknown => write!(f, "unknown"),
            Ty::RustType => write!(f, "$rust_type"),
            Ty::Unknown => write!(f, "unknown"),
            Ty::Error => write!(f, "!error"),
        }
    }
}

impl fmt::Display for PrimitiveType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrimitiveType::Int => write!(f, "int"),
            PrimitiveType::Float => write!(f, "float"),
            PrimitiveType::String => write!(f, "string"),
            PrimitiveType::Bool => write!(f, "bool"),
            PrimitiveType::Null => write!(f, "null"),
            PrimitiveType::Image => write!(f, "image"),
            PrimitiveType::Audio => write!(f, "audio"),
            PrimitiveType::Video => write!(f, "video"),
            PrimitiveType::Pdf => write!(f, "pdf"),
        }
    }
}
