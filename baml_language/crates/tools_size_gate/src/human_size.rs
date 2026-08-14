use bytesize::ByteSize;

pub(crate) fn format(bytes: u64) -> String {
    ByteSize::b(bytes).display().iec().to_string()
}

pub(crate) fn parse(value: &str) -> Result<u64, String> {
    value
        .parse::<ByteSize>()
        .map(|size| size.as_u64())
        .map_err(|error| format!("invalid size `{value}`: {error}"))
}

pub(crate) mod required {
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

    use super::{format, parse};

    #[allow(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde serialize_with requires a reference to the field"
    )]
    pub(crate) fn serialize<S>(bytes: &u64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        format(*bytes).serialize(serializer)
    }

    pub(crate) fn deserialize<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        parse(&value).map_err(de::Error::custom)
    }
}

pub(crate) mod optional {
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

    use super::{format, parse};

    #[allow(
        clippy::ref_option,
        reason = "serde serialize_with requires a reference to the field"
    )]
    pub(crate) fn serialize<S>(bytes: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        bytes.map(format).serialize(serializer)
    }

    pub(crate) fn deserialize<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<String>::deserialize(deserializer)?
            .map(|value| parse(&value).map_err(de::Error::custom))
            .transpose()
    }
}

pub(crate) mod map {
    use std::collections::BTreeMap;

    use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

    use super::{format, parse};
    pub(crate) fn serialize<S>(
        sizes: &BTreeMap<String, u64>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        sizes
            .iter()
            .map(|(name, bytes)| (name, format(*bytes)))
            .collect::<BTreeMap<_, _>>()
            .serialize(serializer)
    }

    pub(crate) fn deserialize<'de, D>(deserializer: D) -> Result<BTreeMap<String, u64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        BTreeMap::<String, String>::deserialize(deserializer)?
            .into_iter()
            .map(|(name, value)| {
                parse(&value)
                    .map(|bytes| (name, bytes))
                    .map_err(de::Error::custom)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use bytesize::ByteSize;

    use super::{format, parse};

    #[test]
    fn uses_bytesize_iec_display() {
        assert_eq!("518.0 GiB", ByteSize::gib(518).display().iec().to_string());
        assert_eq!("14.8 MiB", format(15_519_843));
        assert_eq!("1.0 KiB", format(1_024));
        assert_eq!("0 B", format(0));
    }

    #[test]
    fn parses_bytesize_strings() {
        assert_eq!(parse("1.5 KiB").unwrap(), 1_536);
        assert_eq!(parse("2 GiB").unwrap(), 2_147_483_648);
        assert!(parse("not a size").is_err());
    }
}
