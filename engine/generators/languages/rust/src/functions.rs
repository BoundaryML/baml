use crate::r#type::TypeRust;

// Stub for function generation - to be implemented in Phase 4
pub struct FunctionRust {
    pub(crate) documentation: Option<String>,
    pub(crate) name: String,
    pub(crate) args: Vec<(String, TypeRust)>,
    pub(crate) return_type: TypeRust,
    pub(crate) stream_return_type: TypeRust,
}
