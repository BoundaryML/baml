use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Repr {
    Int,
    Bigint,
    Float,
    Bool,
    Str,
    Bytes,
    Opaque,
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
    Map(usize),
    MapStrPtr(usize),
    Iter,
    Naming,
    Enum(usize),
    Null,
    Future,
    ListFuture(u64),
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
        if self == Self::Float {
            format!("f64::from_bits({bits})")
        } else if self == Self::Bool {
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
            Self::ListFuture(_)
                | Self::ListClosure(_)
                | Self::ListInt
                | Self::ListPtr(_)
                | Self::ListStr
                | Self::ListListStr
                | Self::List(_)
        )
    }
    pub(super) fn slot(self) -> Gen<&'static str> {
        Ok(match self {
            Self::Int => "i64",
            Self::Enum(_) | Self::Naming=>"usize",
            Self::Null=>"rt::maps::Null",
            Self::Float => "f64",
            Self::Bool => "bool",
            Self::Str => "rt::lists::Ptr",
            Self::Closure => "rt::closures::Closure",
            Self::Future => "rt::tasks::FutureSlot",
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
                | Self::Bigint
                | Self::Bytes
                | Self::Opaque
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
                | Self::Map(_)
                | Self::Future
                | Self::ListFuture(_)
                | Self::MapStrPtr(_)
        )
    }
}
