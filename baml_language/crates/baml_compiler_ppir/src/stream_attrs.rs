#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StreamAttrTarget {
    Type,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StreamAttrArgRule {
    None,
    ExactlyOne,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StreamAttrKind {
    Done,
    NotNull,
    StartsAs,
    Type,
    WithState,
}

impl StreamAttrKind {
    #[inline]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Done => "stream.done",
            Self::NotNull => "stream.not_null",
            Self::StartsAs => "stream.starts_as",
            Self::Type => "stream.type",
            Self::WithState => "stream.with_state",
        }
    }

    #[inline]
    pub const fn arg_rule(self) -> StreamAttrArgRule {
        match self {
            Self::Done | Self::NotNull | Self::WithState => StreamAttrArgRule::None,
            Self::StartsAs | Self::Type => StreamAttrArgRule::ExactlyOne,
        }
    }

    #[inline]
    pub const fn supports_target(self, target: StreamAttrTarget) -> bool {
        match (self, target) {
            (Self::Done, StreamAttrTarget::Type | StreamAttrTarget::Block) => true,
            (Self::NotNull, StreamAttrTarget::Type | StreamAttrTarget::Block) => true,
            (Self::StartsAs, StreamAttrTarget::Type) => true,
            (Self::Type, StreamAttrTarget::Type) => true,
            (Self::WithState, StreamAttrTarget::Type) => true,
            _ => false,
        }
    }
}

const TYPE_STREAM_ATTR_NAMES: &[&str] = &[
    StreamAttrKind::Done.name(),
    StreamAttrKind::NotNull.name(),
    StreamAttrKind::StartsAs.name(),
    StreamAttrKind::Type.name(),
    StreamAttrKind::WithState.name(),
];

const BLOCK_STREAM_ATTR_NAMES: &[&str] =
    &[StreamAttrKind::Done.name(), StreamAttrKind::NotNull.name()];

#[inline]
pub fn parse_stream_attr(name: &str) -> Option<StreamAttrKind> {
    match name {
        "stream.done" => Some(StreamAttrKind::Done),
        "stream.not_null" => Some(StreamAttrKind::NotNull),
        "stream.starts_as" => Some(StreamAttrKind::StartsAs),
        "stream.type" => Some(StreamAttrKind::Type),
        "stream.with_state" => Some(StreamAttrKind::WithState),
        _ => None,
    }
}

#[inline]
pub const fn valid_stream_attr_names(target: StreamAttrTarget) -> &'static [&'static str] {
    match target {
        StreamAttrTarget::Type => TYPE_STREAM_ATTR_NAMES,
        StreamAttrTarget::Block => BLOCK_STREAM_ATTR_NAMES,
    }
}
