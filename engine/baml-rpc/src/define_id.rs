use type_safe_id::{StaticType, TypeSafeId};

macro_rules! define_id {
    ($name:ident, $inner_name:ident, $type_str:expr) => {
        #[derive(Default, Clone, PartialEq, Eq, Hash)]
        struct $inner_name;

        impl StaticType for $inner_name {
            const TYPE: &'static str = $type_str;
        }

        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(TypeSafeId<$inner_name>);

        impl $name {
            #[allow(dead_code)]
            pub fn new() -> Self {
                Self(TypeSafeId::<$inner_name>::new())
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

        impl ToString for $name {
            fn to_string(&self) -> String {
                self.0.to_string()
            }
        }
    };
}

define_id!(SpanId, Span_, "bspan");
define_id!(TraceEventId, TraceEvent_, "bevent");
define_id!(HttpRequestId, HttpRequest_, "breq");
define_id!(ProjectId, Project_, "proj");
define_id!(TraceBatchId, TraceBatch_, "tracebatch");
