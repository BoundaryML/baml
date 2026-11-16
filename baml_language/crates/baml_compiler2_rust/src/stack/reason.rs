//! Why a group of locals stays on the heap; the report prints it.

use std::fmt;

use baml_compiler2_mir::escape::MASKED_PARAMS;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum HeapReason {
    // The class itself.
    NotInProgram {
        class: String,
    },
    NotClassObject {
        class: String,
    },
    Cleanup {
        class: String,
    },
    Generic {
        class: String,
    },
    LaidOutDifferently {
        class: String,
    },
    MutableField {
        class: String,
        field: String,
    },
    OutsideSource,
    GenericClass,
    TwoClasses,
    NotThisClass,
    NotStackClass,
    // Parameters and allocations.
    MayHoldParameter,
    EitherOfTwoParameters,
    ParameterOrNew,
    NeverAllocated,
    NotStackObject,
    NoSuchParameter,
    ParamEscapes {
        param: usize,
        reason: Box<HeapReason>,
    },
    LiveAlias,
    FieldDoesNotFit,
    // Calls.
    PassedTo {
        callee: String,
    },
    PassedWithTrace {
        callee: String,
    },
    PassedByName {
        callee: String,
    },
    PassedToGeneric {
        callee: String,
    },
    PassedWithDefaults {
        callee: String,
    },
    PassedTwice {
        callee: String,
    },
    PassedPastMask {
        callee: String,
    },
    NotCompiled {
        callee: String,
    },
    MaySuspend {
        callee: String,
    },
    VmFrame {
        callee: String,
    },
    NoDirectEntry {
        callee: String,
    },
    CalleeVariant {
        callee: String,
        dead: Box<HeapReason>,
    },
    Called,
    Passed,
    Compared,
    CallResult,
    // Terminators.
    Returned,
    CaughtError,
    TypeTestedNonClass,
    Narrowed,
    HoldsNarrowed,
    InterfaceCall {
        method: String,
    },
    SysOp,
    Spawned,
    HoldsFuture,
    Awaited,
    Thrown,
    BranchedOn,
    ShortCircuitResult,
    UsedByTerminator,
    // Statements and places.
    Captured,
    HeldInCell,
    Indexed,
    UsedAsIndex,
    ComputedHere,
    Copied,
    Stored,
    StoredInto,
    StoredThrough,
    StoredIntoField,
    StoredIntoAnotherObject,
    StoredIntoHeapObject,
    StoredIntoContainer,
    BoundAsReceiver,
    OperatedOn,
    UsedAsValue,
    Inspected,
    Logged,
    TraceHook,
    ThroughInterface,
}

// One arm per variant: a flat table, so its cyclomatic count is its size.
impl fmt::Display for HeapReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInProgram { class } => write!(f, "class `{class}` is not in the program"),
            Self::NotClassObject { class } => write!(f, "`{class}` is not a class object"),
            Self::Cleanup { class } => write!(f, "`{class}` has a cleanup finalizer"),
            Self::Generic { class } => write!(f, "`{class}` is generic"),
            Self::LaidOutDifferently { class } => {
                write!(f, "`{class}` has fields the program lays out differently")
            }
            Self::MutableField { class, field } => {
                write!(f, "`{class}.{field}` holds a mutable value")
            }
            Self::ParamEscapes { param, reason } => {
                write!(f, "parameter {param} escapes: {reason}")
            }
            Self::PassedTo { callee } => write!(f, "passed to {callee}"),
            Self::PassedWithTrace { callee } => write!(f, "passed to {callee} with a trace"),
            Self::PassedByName { callee } => write!(f, "passed to {callee} by name"),
            Self::PassedToGeneric { callee } => write!(f, "passed to generic {callee}"),
            Self::PassedWithDefaults { callee } => {
                write!(f, "passed to {callee} with defaulted arguments")
            }
            Self::PassedTwice { callee } => write!(f, "passed twice to {callee}"),
            Self::PassedPastMask { callee } => write!(
                f,
                "passed to {callee} beyond its first {MASKED_PARAMS} parameters"
            ),
            Self::NotCompiled { callee } => write!(f, "passed to {callee}, which is not compiled"),
            Self::MaySuspend { callee } => write!(f, "passed to {callee}, which may suspend"),
            Self::VmFrame { callee } => write!(f, "passed to {callee}, which runs in a VM frame"),
            Self::NoDirectEntry { callee } => {
                write!(f, "passed to {callee}, which has no direct entry")
            }
            Self::CalleeVariant { callee, dead } => write!(f, "passed to {callee}, whose {dead}"),
            Self::InterfaceCall { method } => write!(f, "interface call .{method}"),
            Self::OutsideSource => f.write_str("class declared outside the source"),
            Self::GenericClass => f.write_str("generic class"),
            Self::TwoClasses => f.write_str("holds objects of two classes"),
            Self::NotThisClass => f.write_str("may also hold a value that is not this class"),
            Self::NotStackClass => f.write_str("not a stack class"),
            Self::MayHoldParameter => f.write_str("may hold a parameter"),
            Self::EitherOfTwoParameters => f.write_str("may hold either of two parameters"),
            Self::ParameterOrNew => f.write_str("may hold a parameter or a new object"),
            Self::NeverAllocated => f.write_str("never allocated here"),
            Self::NotStackObject => f.write_str("not a stack object"),
            Self::NoSuchParameter => f.write_str("no such parameter"),
            Self::LiveAlias => {
                f.write_str("an alias of its previous object is live when it is allocated again")
            }
            Self::FieldDoesNotFit => f.write_str("a field's value does not fit its declared type"),
            Self::Called => f.write_str("called"),
            Self::Passed => f.write_str("passed"),
            Self::Compared => f.write_str("compared"),
            Self::CallResult => f.write_str("holds a call result"),
            Self::Returned => f.write_str("returned"),
            Self::CaughtError => f.write_str("caught error"),
            Self::TypeTestedNonClass => f.write_str("type-tested against a non-class type"),
            Self::Narrowed => f.write_str("narrowed"),
            Self::HoldsNarrowed => f.write_str("holds a narrowed value"),
            Self::SysOp => f.write_str("passed to a sys-op"),
            Self::Spawned => f.write_str("spawned"),
            Self::HoldsFuture => f.write_str("holds a future"),
            Self::Awaited => f.write_str("awaited"),
            Self::Thrown => f.write_str("thrown"),
            Self::BranchedOn => f.write_str("branched on"),
            Self::ShortCircuitResult => f.write_str("holds a short-circuit result"),
            Self::UsedByTerminator => f.write_str("used by a terminator"),
            Self::Captured => f.write_str("captured by a closure"),
            Self::HeldInCell => f.write_str("held in a cell"),
            Self::Indexed => f.write_str("indexed"),
            Self::UsedAsIndex => f.write_str("used as an index"),
            Self::ComputedHere => f.write_str("holds a value computed here"),
            Self::Copied => f.write_str("copied"),
            Self::Stored => f.write_str("stored"),
            Self::StoredInto => f.write_str("stored into"),
            Self::StoredThrough => f.write_str("stored through"),
            Self::StoredIntoField => f.write_str("stored into a field"),
            Self::StoredIntoAnotherObject => f.write_str("stored into a field of another object"),
            Self::StoredIntoHeapObject => f.write_str("stored into a heap object"),
            Self::StoredIntoContainer => f.write_str("stored into a container"),
            Self::BoundAsReceiver => f.write_str("bound as a method receiver"),
            Self::OperatedOn => f.write_str("compared by identity or operated on"),
            Self::UsedAsValue => f.write_str("used as a value"),
            Self::Inspected => f.write_str("inspected"),
            Self::Logged => f.write_str("logged"),
            Self::TraceHook => f.write_str("passed to a trace hook"),
            Self::ThroughInterface => f.write_str("accessed through an interface"),
        }
    }
}
