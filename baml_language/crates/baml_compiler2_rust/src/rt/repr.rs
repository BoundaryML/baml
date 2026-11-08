use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Repr {
    Int,
    Float,
    Bool,
    Str,
    Ptr(usize),
    ListInt,
    ListStr,
    ListListStr,
    Type(usize),
    ListPtr(usize),
    Dyn(usize),
    Tagged,
    MapStrInt,
    Iter,
    Naming,
    Enum(usize),
    Null,
}
impl Repr {
    pub(super) fn rust(self) -> &'static str {
        match self {
            Self::Int => "i64",
            Self::Float => "f64",
            Self::Bool => "bool",
            Self::Dyn(_) | Self::Tagged => "rt::Dyn",
            _ => "usize",
        }
    }
    pub(super) fn zero(self) -> &'static str {
        match self {
            Self::Bool => "false",
            Self::Float => "0.0",
            Self::Dyn(_) | Self::Tagged => "rt::Dyn{tag:2,payload:0}",
            _ => "0",
        }
    }
    pub(super) fn growing(self) -> bool {
        matches!(self, Self::ListStr | Self::ListListStr)
    }
    pub(super) fn slot(self) -> Gen<&'static str> {
        match self {
            Self::Str => Ok("rt::strings::Str"),
            Self::ListStr => Ok("rt::lists::List<rt::strings::Str>"),
            _ => Err(no("growing list element")),
        }
    }
    pub(super) fn pointer(self) -> bool {
        matches!(
            self,
            Self::Str
                | Self::Ptr(_)
                | Self::ListInt
                | Self::ListPtr(_)
                | Self::ListStr
                | Self::ListListStr
                | Self::MapStrInt
        )
    }
}
