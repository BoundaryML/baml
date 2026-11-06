#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodegenTypeError {
    InvalidMapKey(Box<super::Ty>),
}
