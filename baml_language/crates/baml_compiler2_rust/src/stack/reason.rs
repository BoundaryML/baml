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
        use HeapReason::*;
        match self {
            NotInProgram { class } => write!(f, "class `{class}` is not in the program"),
            NotClassObject { class } => write!(f, "`{class}` is not a class object"),
            Cleanup { class } => write!(f, "`{class}` has a cleanup finalizer"),
            Generic { class } => write!(f, "`{class}` is generic"),
            LaidOutDifferently { class } => {
                write!(f, "`{class}` has fields the program lays out differently")
            }
            MutableField { class, field } => write!(f, "`{class}.{field}` holds a mutable value"),
            ParamEscapes { param, reason } => write!(f, "parameter {param} escapes: {reason}"),
            PassedTo { callee } => write!(f, "passed to {callee}"),
            PassedWithTrace { callee } => write!(f, "passed to {callee} with a trace"),
            PassedByName { callee } => write!(f, "passed to {callee} by name"),
            PassedToGeneric { callee } => write!(f, "passed to generic {callee}"),
            PassedWithDefaults { callee } => {
                write!(f, "passed to {callee} with defaulted arguments")
            }
            PassedTwice { callee } => write!(f, "passed twice to {callee}"),
            PassedPastMask { callee } => write!(
                f,
                "passed to {callee} beyond its first {MASKED_PARAMS} parameters"
            ),
            NotCompiled { callee } => write!(f, "passed to {callee}, which is not compiled"),
            MaySuspend { callee } => write!(f, "passed to {callee}, which may suspend"),
            VmFrame { callee } => write!(f, "passed to {callee}, which runs in a VM frame"),
            NoDirectEntry { callee } => write!(f, "passed to {callee}, which has no direct entry"),
            CalleeVariant { callee, dead } => write!(f, "passed to {callee}, whose {dead}"),
            InterfaceCall { method } => write!(f, "interface call .{method}"),
            OutsideSource => f.write_str("class declared outside the source"),
            GenericClass => f.write_str("generic class"),
            TwoClasses => f.write_str("holds objects of two classes"),
            NotThisClass => f.write_str("may also hold a value that is not this class"),
            NotStackClass => f.write_str("not a stack class"),
            MayHoldParameter => f.write_str("may hold a parameter"),
            EitherOfTwoParameters => f.write_str("may hold either of two parameters"),
            ParameterOrNew => f.write_str("may hold a parameter or a new object"),
            NeverAllocated => f.write_str("never allocated here"),
            NotStackObject => f.write_str("not a stack object"),
            NoSuchParameter => f.write_str("no such parameter"),
            LiveAlias => {
                f.write_str("an alias of its previous object is live when it is allocated again")
            }
            FieldDoesNotFit => f.write_str("a field's value does not fit its declared type"),
            Called => f.write_str("called"),
            Passed => f.write_str("passed"),
            Compared => f.write_str("compared"),
            CallResult => f.write_str("holds a call result"),
            Returned => f.write_str("returned"),
            CaughtError => f.write_str("caught error"),
            TypeTestedNonClass => f.write_str("type-tested against a non-class type"),
            Narrowed => f.write_str("narrowed"),
            HoldsNarrowed => f.write_str("holds a narrowed value"),
            SysOp => f.write_str("passed to a sys-op"),
            Spawned => f.write_str("spawned"),
            HoldsFuture => f.write_str("holds a future"),
            Awaited => f.write_str("awaited"),
            Thrown => f.write_str("thrown"),
            BranchedOn => f.write_str("branched on"),
            ShortCircuitResult => f.write_str("holds a short-circuit result"),
            UsedByTerminator => f.write_str("used by a terminator"),
            Captured => f.write_str("captured by a closure"),
            HeldInCell => f.write_str("held in a cell"),
            Indexed => f.write_str("indexed"),
            UsedAsIndex => f.write_str("used as an index"),
            ComputedHere => f.write_str("holds a value computed here"),
            Copied => f.write_str("copied"),
            Stored => f.write_str("stored"),
            StoredInto => f.write_str("stored into"),
            StoredThrough => f.write_str("stored through"),
            StoredIntoField => f.write_str("stored into a field"),
            StoredIntoAnotherObject => f.write_str("stored into a field of another object"),
            StoredIntoHeapObject => f.write_str("stored into a heap object"),
            StoredIntoContainer => f.write_str("stored into a container"),
            BoundAsReceiver => f.write_str("bound as a method receiver"),
            OperatedOn => f.write_str("compared by identity or operated on"),
            UsedAsValue => f.write_str("used as a value"),
            Inspected => f.write_str("inspected"),
            Logged => f.write_str("logged"),
            TraceHook => f.write_str("passed to a trace hook"),
            ThroughInterface => f.write_str("accessed through an interface"),
        }
    }
}
