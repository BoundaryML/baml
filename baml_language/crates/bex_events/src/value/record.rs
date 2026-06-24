use std::io;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueCodec {
    BamlOutboundValue,
}

impl ValueCodec {
    #[must_use]
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::BamlOutboundValue => "bamlOutboundValue",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueAvailability {
    Pending,
    Available,
    Missing,
    Omitted,
    Lost,
}

impl ValueAvailability {
    #[must_use]
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Available => "available",
            Self::Missing => "missing",
            Self::Omitted => "omitted",
            Self::Lost => "lost",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueRef {
    pub id: String,
    pub codec: ValueCodec,
    pub availability: ValueAvailability,
    pub original_size_bytes: Option<usize>,
    pub retained_size_bytes: Option<usize>,
    pub diagnostic: Option<String>,
}

impl ValueRef {
    #[must_use]
    pub fn pending(id: impl Into<String>, codec: ValueCodec) -> Self {
        Self {
            id: id.into(),
            codec,
            availability: ValueAvailability::Pending,
            original_size_bytes: None,
            retained_size_bytes: None,
            diagnostic: None,
        }
    }

    #[must_use]
    pub fn available(
        id: impl Into<String>,
        codec: ValueCodec,
        original_size_bytes: usize,
        retained_size_bytes: usize,
    ) -> Self {
        Self {
            id: id.into(),
            codec,
            availability: ValueAvailability::Available,
            original_size_bytes: Some(original_size_bytes),
            retained_size_bytes: Some(retained_size_bytes),
            diagnostic: None,
        }
    }

    #[must_use]
    pub fn lost(id: impl Into<String>, codec: ValueCodec, diagnostic: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            codec,
            availability: ValueAvailability::Lost,
            original_size_bytes: None,
            retained_size_bytes: Some(0),
            diagnostic: Some(diagnostic.into()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueRecord {
    pub value_ref: ValueRef,
    pub body: Vec<u8>,
}

impl TryFrom<crate::value::pb::ValueMetadataV1> for ValueRef {
    type Error = io::Error;

    fn try_from(metadata: crate::value::pb::ValueMetadataV1) -> Result<Self, Self::Error> {
        let codec = match metadata.codec() {
            crate::value::pb::ValueCodec::BamlOutboundValue => ValueCodec::BamlOutboundValue,
            crate::value::pb::ValueCodec::Unspecified => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "value metadata omitted codec",
                ));
            }
        };
        let availability = match metadata.availability() {
            crate::value::pb::ValueAvailability::Pending => ValueAvailability::Pending,
            crate::value::pb::ValueAvailability::Available => ValueAvailability::Available,
            crate::value::pb::ValueAvailability::Missing => ValueAvailability::Missing,
            crate::value::pb::ValueAvailability::Omitted => ValueAvailability::Omitted,
            crate::value::pb::ValueAvailability::Lost => ValueAvailability::Lost,
            crate::value::pb::ValueAvailability::Unspecified => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "value metadata omitted availability",
                ));
            }
        };
        Ok(Self {
            id: metadata.id,
            codec,
            availability,
            original_size_bytes: metadata
                .original_size_bytes
                .map(usize::try_from)
                .transpose()
                .map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "original size does not fit usize",
                    )
                })?,
            retained_size_bytes: metadata
                .retained_size_bytes
                .map(usize::try_from)
                .transpose()
                .map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "retained size does not fit usize",
                    )
                })?,
            diagnostic: metadata.diagnostic,
        })
    }
}

impl From<&ValueRef> for crate::value::pb::ValueMetadataV1 {
    fn from(value_ref: &ValueRef) -> Self {
        Self {
            id: value_ref.id.clone(),
            codec: match value_ref.codec {
                ValueCodec::BamlOutboundValue => {
                    crate::value::pb::ValueCodec::BamlOutboundValue as i32
                }
            },
            availability: match value_ref.availability {
                ValueAvailability::Pending => crate::value::pb::ValueAvailability::Pending as i32,
                ValueAvailability::Available => {
                    crate::value::pb::ValueAvailability::Available as i32
                }
                ValueAvailability::Missing => crate::value::pb::ValueAvailability::Missing as i32,
                ValueAvailability::Omitted => crate::value::pb::ValueAvailability::Omitted as i32,
                ValueAvailability::Lost => crate::value::pb::ValueAvailability::Lost as i32,
            },
            original_size_bytes: value_ref
                .original_size_bytes
                .and_then(|value| u64::try_from(value).ok()),
            retained_size_bytes: value_ref
                .retained_size_bytes
                .and_then(|value| u64::try_from(value).ok()),
            diagnostic: value_ref.diagnostic.clone(),
        }
    }
}
