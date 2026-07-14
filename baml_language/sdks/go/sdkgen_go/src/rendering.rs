//! Generator-owned Go identifiers.
//!
//! These names share lexical scope with projected BAML parameters. Keeping
//! them typed and centralized lets name allocation reserve exactly the names
//! the renderer emits.

use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GeneratorIdent {
    ContextPackage,
    BigPackage,
    BootstrapPackage,
    RuntimePackage,
    ContextParameter,
    ErrorLocal,
    ResultLocal,
    ZeroLocal,
    StringType,
    Int64Type,
    Float64Type,
    BoolType,
    ByteType,
    ErrorType,
}

impl GeneratorIdent {
    pub(crate) const FUNCTION_SCOPE: &'static [Self] = &[
        Self::ContextPackage,
        Self::BigPackage,
        Self::BootstrapPackage,
        Self::RuntimePackage,
        Self::ContextParameter,
        Self::ErrorLocal,
        Self::ResultLocal,
        Self::ZeroLocal,
        Self::StringType,
        Self::Int64Type,
        Self::Float64Type,
        Self::BoolType,
        Self::ByteType,
        Self::ErrorType,
    ];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ContextPackage => "context",
            Self::BigPackage => "big",
            Self::BootstrapPackage => "bootstrap",
            Self::RuntimePackage => "baml_go",
            Self::ContextParameter => "ctx",
            Self::ErrorLocal => "err",
            Self::ResultLocal => "result",
            Self::ZeroLocal => "zero",
            Self::StringType => "string",
            Self::Int64Type => "int64",
            Self::Float64Type => "float64",
            Self::BoolType => "bool",
            Self::ByteType => "byte",
            Self::ErrorType => "error",
        }
    }
}

impl fmt::Display for GeneratorIdent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
