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
}

impl GeneratorIdent {
    pub(crate) const FUNCTION_SCOPE: [Self; 5] = [
        Self::BootstrapPackage,
        Self::ContextParameter,
        Self::ErrorLocal,
        Self::ResultLocal,
        Self::ZeroLocal,
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
        }
    }
}

impl fmt::Display for GeneratorIdent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
