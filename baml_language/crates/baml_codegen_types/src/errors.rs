pub enum CodegenTypeError {
    InvalidUnionUsage(super::Ty),
    InvalidOptionalUsage(super::Ty),
    InvalidMapKey(super::Ty),
    InvalidUnit(super::Ty),
}
