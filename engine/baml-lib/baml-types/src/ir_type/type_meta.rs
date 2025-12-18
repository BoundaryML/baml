pub type IR = base::TypeMeta;
pub type NonStreaming = non_streaming::TypeMeta;
pub type Streaming = stream::TypeMetaStreaming;

/// Trait to check if a type metadata has @check constraints.
/// Used by flatten() to preserve unions that have checks.
pub trait MayHaveMeta {
    fn has_checks(&self) -> bool;
    fn has_stream_state(&self) -> bool;
}

pub mod base {
    use super::MayHaveMeta;
    use crate::{Constraint, ConstraintLevel};

    #[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash, Default)]
    pub struct TypeMeta {
        pub constraints: Vec<Constraint>,
        pub streaming_behavior: StreamingBehavior,
    }

    /// Metadata on a type that determines how it behaves under streaming conditions.
    #[derive(Clone, Debug, PartialEq, serde::Serialize, Eq, Hash, Default)]
    pub struct StreamingBehavior {
        /// A type with the `not_null` property will not be visible in a stream until
        /// we are certain that it is not null (as in the value has at least begun)
        pub needed: bool,

        /// A type with the `done` property will not be visible in a stream until
        /// we are certain that it is completely available (i.e. the parser did
        /// not finalize it through any early termination, enough tokens were available
        /// from the LLM response to be certain that it is done).
        pub done: bool,

        /// A type with the `state` property will be represented in client code as
        /// a struct: `{value: T, streaming_state: "incomplete" | "complete"}`.
        pub state: bool,
    }

    impl StreamingBehavior {
        pub fn combine(&self, other: &Self) -> Self {
            Self {
                needed: self.needed || other.needed,
                done: self.done || other.done,
                state: self.state || other.state,
            }
        }
    }

    impl MayHaveMeta for TypeMeta {
        fn has_checks(&self) -> bool {
            self.constraints
                .iter()
                .any(|c| matches!(c.level, ConstraintLevel::Check))
        }

        fn has_stream_state(&self) -> bool {
            self.streaming_behavior.state
        }
    }
}

pub mod non_streaming {
    use super::MayHaveMeta;
    use crate::{Constraint, ConstraintLevel};

    #[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash, Default)]
    pub struct TypeMeta {
        pub constraints: Vec<Constraint>,
    }

    impl MayHaveMeta for TypeMeta {
        fn has_checks(&self) -> bool {
            self.constraints
                .iter()
                .any(|c| matches!(c.level, ConstraintLevel::Check))
        }

        fn has_stream_state(&self) -> bool {
            false
        }
    }
}

pub mod stream {
    use super::MayHaveMeta;
    use crate::{Constraint, ConstraintLevel};

    #[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash, Default)]
    pub struct TypeMetaStreaming {
        pub constraints: Vec<Constraint>,
        pub streaming_behavior: StreamingBehavior,
    }

    impl MayHaveMeta for TypeMetaStreaming {
        fn has_checks(&self) -> bool {
            self.constraints
                .iter()
                .any(|c| matches!(c.level, ConstraintLevel::Check))
        }

        fn has_stream_state(&self) -> bool {
            self.streaming_behavior.state
        }
    }

    /// Metadata on a type that determines how it behaves under streaming conditions.
    #[derive(Clone, Debug, PartialEq, serde::Serialize, Eq, Hash, Default)]
    pub struct StreamingBehavior {
        /// A type with the `done` property will not be visible in a stream until
        /// we are certain that it is completely available (i.e. the parser did
        /// not finalize it through any early termination, enough tokens were available
        /// from the LLM response to be certain that it is done).
        pub done: bool,

        /// A type with the `state` property will be represented in client code as
        /// a struct: `{value: T, streaming_state: "incomplete" | "complete"}`.
        pub state: bool,
    }

    impl TypeMetaStreaming {
        pub fn done(mut self) -> Self {
            self.streaming_behavior.done = true;
            self
        }

        pub fn state(mut self) -> Self {
            self.streaming_behavior.state = true;
            self
        }
    }
}
