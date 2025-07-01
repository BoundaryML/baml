use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::OffsetDateTime;
use ts_rs::TS;

#[derive(Debug, Clone, TS)]
#[ts(export, type = "number")]
pub struct EpochMsTimestamp(time::OffsetDateTime);

impl PartialEq for EpochMsTimestamp {
    fn eq(&self, other: &Self) -> bool {
        self.0.unix_timestamp_nanos() == other.0.unix_timestamp_nanos()
    }
}

impl PartialOrd for EpochMsTimestamp {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(
            self.0
                .unix_timestamp_nanos()
                .cmp(&other.0.unix_timestamp_nanos()),
        )
    }
}

impl From<time::OffsetDateTime> for EpochMsTimestamp {
    fn from(value: time::OffsetDateTime) -> Self {
        Self(value)
    }
}

impl TryFrom<web_time::SystemTime> for EpochMsTimestamp {
    type Error = anyhow::Error;

    fn try_from(system_time: web_time::SystemTime) -> anyhow::Result<Self> {
        let duration = system_time.duration_since(web_time::SystemTime::UNIX_EPOCH)?;
        let offset_date_time =
            OffsetDateTime::from_unix_timestamp_nanos(duration.as_nanos() as i128)?;
        Ok(EpochMsTimestamp(offset_date_time))
    }
}

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

impl From<EpochMsTimestamp> for time::OffsetDateTime {
    fn from(value: EpochMsTimestamp) -> Self {
        value.0
    }
}
