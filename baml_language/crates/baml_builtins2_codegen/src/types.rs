/// A single extracted `$rust_function` builtin.
pub struct NativeBuiltin {
    /// Dotted path: e.g. `"baml.Array.length"`, `"baml.deep_copy"`, `"baml.math.trunc"`
    pub path: String,
    /// Rust function name derived from path (dots → underscores, lowercased):
    /// e.g. `"baml_array_length"`, `"baml_deep_copy"`, `"baml_math_trunc"`
    pub fn_name: String,
    /// Non-self/receiver parameters only.
    pub params: Vec<Param>,
    pub return_type: BamlType,
    /// Generic type parameters declared on the function or class (e.g. `["T"]`).
    pub generics: Vec<String>,
    /// None for free functions; Some for methods with a `self` receiver.
    pub receiver: Option<Receiver>,
}

/// A single non-receiver parameter.
pub struct Param {
    pub name: String,
    pub ty: BamlType,
}

/// Receiver (the `self` parameter) of a method.
pub struct Receiver {
    /// The class name (e.g. `"Array"`, `"Map"`, `"String"`, `"Pdf"`).
    pub class_name: String,
    /// Generic type parameters of the class (e.g. `["T"]` for `Array<T>`).
    pub class_generics: Vec<String>,
    /// True when preceded by `//baml:mut_self` in the source.
    pub is_mut: bool,
}

/// BAML type extracted from a type expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BamlType {
    String,
    Int,
    Float,
    Bool,
    Null,
    List(Box<BamlType>),
    Map(Box<BamlType>, Box<BamlType>),
    Optional(Box<BamlType>),
    /// A generic type parameter like `T`, `K`, `V`.
    Generic(String),
    /// A named media class: `"Pdf"`, `"Audio"`, `"Image"`, `"Video"`.
    Media(String),
    /// Some other named type (class reference, path type).
    Named(String),
}
