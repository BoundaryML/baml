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
                    Err(e) => Err(serde::de::Error::custom(e.to_string())),
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
