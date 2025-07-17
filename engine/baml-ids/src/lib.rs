use type_safe_id::{StaticType, TypeSafeId};

macro_rules! define_id {
    ($name:ident, $inner_name:ident, $type_str:expr) => {
        #[derive(Default, Clone, PartialEq, Eq, Hash)]
        pub struct $inner_name;

        impl StaticType for $inner_name {
            const TYPE: &'static str = $type_str;
        }

        #[derive(Clone, PartialEq, Eq, Hash)]
        pub struct $name(TypeSafeId<$inner_name>);

        impl $name {
            #[allow(dead_code)]
            pub fn new() -> Self {
                Self(TypeSafeId::<$inner_name>::new())
            }

            pub fn from_uuid(uuid: uuid::Uuid) -> Self {
                Self(TypeSafeId::<$inner_name>::from_uuid(uuid))
            }

            pub fn timestamp(&self) -> time::OffsetDateTime {
                let (seconds, subsec_nanos) = self
                    .0
                    .uuid()
                    .get_timestamp()
                    .expect(&format!(
                        "{} should always contain a timestamp",
                        $inner_name::TYPE
                    ))
                    .to_unix();

                use std::time::Duration;
                let epoch_timestamp =
                    Duration::from_secs(seconds) + Duration::from_nanos(subsec_nanos as u64);
                time::OffsetDateTime::from_unix_timestamp_nanos(epoch_timestamp.as_nanos() as i128)
                    .expect(&format!(
                        "{} should always be convertible to a timestamp",
                        $inner_name::TYPE
                    ))
            }

            pub fn testonly_from_u16(counter: u16) -> Self {
                use std::str::FromStr;
                // f008 sets the version and variant bits correctly, for the resulting uuid to be a uuidv7
                Self::from_str(&format!(
                    "{}_0000000000f008000test{:05}",
                    $type_str, counter
                ))
                .unwrap()
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                self.0.to_string().serialize(serializer)
            }
        }

        // Add Deserialize implementation
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let s = String::deserialize(deserializer)?;
                match s.parse() {
                    Ok(id) => Ok($name(id)),
                    Err(e) => Err(serde::de::Error::custom(format!(
                        "Failed to parse {:?} as {}: {}",
                        s,
                        $inner_name::TYPE,
                        e
                    ))),
                }
            }
        }

        // FromStr
        impl std::str::FromStr for $name {
            type Err = anyhow::Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok($name(TypeSafeId::<$inner_name>::from_str(s)?))
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0.to_string())
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_tuple(stringify!($name))
                    .field(&self.0.to_string())
                    .finish()
            }
        }
    };
}

// USED FOR TOP LEVEL IDS
define_id!(FunctionCallId, FunctionCall_, "bfcall");
// USED FOR CONTENT SPAN IDS
define_id!(FunctionEventId, FunctionEvent_, "bfevent");
// Used for HTTP request IDs internal to BAML
define_id!(HttpRequestId, HttpRequest_, "breq");

define_id!(ProjectId, Project_, "proj");
define_id!(TraceBatchId, TraceBatch_, "tracebatch");
