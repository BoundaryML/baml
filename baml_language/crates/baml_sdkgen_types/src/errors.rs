#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodegenTypeError {
    InvalidUnionUsage(Box<super::Ty>),
    InvalidOptionalUsage(Box<super::Ty>),
    InvalidMapKey(Box<super::Ty>),
    InvalidUnit(Box<super::Ty>),
}
