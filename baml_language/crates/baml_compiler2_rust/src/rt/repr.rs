use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Repr {
    Int,
    Float,
    Bool,
    Str,
    Ptr(usize),
    ListInt,
    List(usize),
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
    Closure,
    Cell,
    ListClosure(u64),
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
            Self::Float => "0.0",
            Self::Dyn(_) | Self::Tagged => "rt::Dyn{tag:2,payload:0}",
            _ => "0",
        }
    }
    pub(super) fn growing(self) -> bool {
        matches!(
            self,
            Self::ListInt | Self::ListPtr(_) | Self::ListStr | Self::ListListStr | Self::List(_)
        )
    }
    pub(super) fn slot(self) -> Gen<&'static str> {
        Ok(match self {
            Self::Int => "i64",
            Self::Float => "f64",
            Self::Bool => "bool",
            Self::Str => "rt::lists::Ptr",
            Self::Dyn(_) | Self::Tagged => "rt::Dyn",
            r if r.pointer() => "rt::lists::Ptr",
            _ => return Err(no("list element slot")),
        })
    }
    pub(super) fn to_slot(self, value: &str) -> Gen<String> {
        Ok(if self.pointer() {
            format!("<{}>::from_raw({value})", self.slot()?)
        } else {
            value.into()
        })
    }
    pub(super) fn from_slot(self, value: &str) -> String {
        if self.pointer() {
            format!("({value}).as_raw()")
        } else {
            value.into()
        }
    }

    pub(super) fn pointer(self) -> bool {
        matches!(
            self,
            Self::Str
                | Self::Ptr(_)
                | Self::List(_)
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
