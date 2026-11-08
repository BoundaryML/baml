use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Repr {
    Int,
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
    Null,
    Closure,
    Cell,
    ListClosure(u64),
}
impl Repr {
    pub(super) fn rust(self) -> &'static str {
        match self {
            Self::Int => "i64",
            Self::Bool => "bool",
            Self::Dyn(_) | Self::Tagged => "rt::Dyn",
            _ => "usize",
        }
    }
    // Shared by ordinary dynamic boundaries and erased callable adapters.
    pub(super) fn decode_payload(self, bits: &str) -> String {
        if self == Self::Bool {
            format!("({bits}!=0)")
        } else {
            format!("({bits} as {})", self.rust())
        }
    }
    pub(super) fn zero(self) -> &'static str {
        match self {
            Self::Bool => "false",
            Self::Dyn(_) | Self::Tagged => "rt::Dyn{tag:2,payload:0}",
            _ => "0",
        }
    }
    pub(super) fn growing(self) -> bool {
        matches!(
            self,
            Self::ListStr | Self::ListListStr | Self::ListClosure(_)
        )
    }
    pub(super) fn slot(self) -> Gen<&'static str> {
        match self {
            Self::Str => Ok("rt::strings::Str"),
            Self::ListStr => Ok("rt::lists::List<rt::strings::Str>"),
            Self::Closure => Ok("rt::closures::Closure"),
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
                | Self::Closure
                | Self::Cell
                | Self::ListClosure(_)
        )
    }
}
