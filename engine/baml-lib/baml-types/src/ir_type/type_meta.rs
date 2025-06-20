pub type Base = base::TypeMeta;
pub type Streaming = stream::TypeMetaStreaming;

pub mod base {
    use crate::Constraint;

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
}

pub mod stream {
    use crate::Constraint;

    #[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash, Default)]
    pub struct TypeMetaStreaming {
        pub constraints: Vec<Constraint>,
        pub streaming_behavior: StreamingBehavior,
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
