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
    /// How the method uses the VM parameter, determined by `//baml:vm` or `//baml:mut_vm`
    /// directives. Mutually exclusive with `receiver.is_mut` (enforced at extraction time).
    pub vm_usage: VmUsage,
}

/// How a native method accesses the VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmUsage {
    /// No `vm` parameter.
    None,
    /// Immutable borrow: `vm: &BexVm` (from `//baml:vm`).
    Ref,
    /// Mutable borrow: `vm: &mut BexVm` (from `//baml:mut_vm`).
    MutRef,
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
    /// A `$rust_type` field — opaque Rust-managed state.
    RustType,
}

/// A single field of a builtin class.
#[derive(Debug, Clone)]
pub struct NativeClassField {
    /// Field name as it appears in the `.baml` source (e.g. `"_data"`, `"message"`).
    pub name: String,
    /// BAML type of the field.
    pub field_type: BamlType,
    /// Positional index in the `Instance.fields` Vec (0-based, matches emission order).
    pub index: usize,
}

/// A builtin class definition extracted from a `.baml` stdlib file.
#[derive(Debug, Clone)]
pub struct NativeClassDef {
    /// Simple class name (e.g. `"Pdf"`, `"InvalidArgument"`).
    pub name: String,
    /// Dotted namespace prefix (e.g. `"baml.media"`, `"baml.errors"`, `"baml"`).
    pub namespace_prefix: String,
    /// Generic type parameters (e.g. `["T"]` for `Array<T>`).
    pub generic_params: Vec<String>,
    /// Fields in declaration order.
    pub fields: Vec<NativeClassField>,
}
