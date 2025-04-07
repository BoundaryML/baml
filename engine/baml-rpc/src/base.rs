use serde::{Deserialize, Serialize};
use serde::{Deserializer, Serializer};
use std::time::Duration;
use time::OffsetDateTime;

#[derive(Debug)]
pub struct EpochMsTimestamp(time::OffsetDateTime);

impl Serialize for EpochMsTimestamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let epoch_millis = Duration::from_nanos(self.0.unix_timestamp_nanos() as u64).as_millis();
        serializer.serialize_u64(epoch_millis as u64)
    }
}

impl<'de> Deserialize<'de> for EpochMsTimestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let epoch_ms: u64 = serde::Deserialize::deserialize(deserializer)?;
        let v = OffsetDateTime::from_unix_timestamp_nanos(
            Duration::from_millis(epoch_ms).as_nanos() as i128,
        )
        .map_err(serde::de::Error::custom)?;
        Ok(EpochMsTimestamp(v))
    }
}

impl Into<time::OffsetDateTime> for EpochMsTimestamp {
    fn into(self) -> time::OffsetDateTime {
        self.0
    }
}
