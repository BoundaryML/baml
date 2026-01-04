use crate::package::{CurrentRenderPackage, Package};

#[derive(Clone, PartialEq, Debug)]
pub enum MediaTypeRust {
    Image,
    Audio,
    Pdf,
    Video,
}

#[derive(Clone, PartialEq, Debug)]
pub enum TypeRust {
    // In case the user has an explicit Null type somewhere
    Null,
    // Primitive types (with optional literal value)
    String(Option<String>),
    Int(Option<i64>),
    Float,
    Bool(Option<bool>),
    Media(MediaTypeRust),
    // Named types
    Class {
        package: Package,
        name: String,
        dynamic: bool,
    },
    Union {
        package: Package,
        name: String,
    },
    Enum {
        package: Package,
        name: String,
        dynamic: bool,
    },
    TypeAlias {
        name: String,
        package: Package,
    },
    // Container types
    List(Box<TypeRust>),
    Map(Box<TypeRust>, Box<TypeRust>),
    // Fallback type
    Any {
        reason: String,
    },
    // Wrapper types
    Optional(Box<TypeRust>),
    Checked(Box<TypeRust>),
    StreamState(Box<TypeRust>),
    /// Heap-allocated wrapper for recursive types (breaks infinite size)
    Boxed(Box<TypeRust>),
}

fn safe_name(name: &str) -> String {
    // replace all non-alphanumeric characters with an underscore
    name.replace(|c: char| !c.is_alphanumeric(), "_")
}

impl TypeRust {
    pub fn make_optional(self) -> Self {
        TypeRust::Optional(Box::new(self))
    }

    pub fn make_checked(self) -> Self {
        TypeRust::Checked(Box::new(self))
    }

    pub fn make_stream_state(self) -> Self {
        TypeRust::StreamState(Box::new(self))
    }

    pub fn make_boxed(self) -> Self {
        TypeRust::Boxed(Box::new(self))
    }

    pub fn flatten_unions(self) -> Vec<TypeRust> {
        match self {
            TypeRust::Union { .. } => {
                vec![self]
            }
            TypeRust::Optional(inner) => inner.flatten_unions(),
            TypeRust::Checked(inner) => inner.flatten_unions(),
            TypeRust::StreamState(inner) => inner.flatten_unions(),
            TypeRust::Boxed(inner) => inner.flatten_unions(),
            _ => vec![],
        }
    }

    // for unions, we need a default name for the type when the union is not named
    pub fn default_name_within_union(&self) -> String {
        match self {
            TypeRust::Null => "Null".to_string(),
            // Handle wrapper types first - they delegate to inner type with prefix
            TypeRust::Optional(inner) => format!("Optional{}", inner.default_name_within_union()),
            TypeRust::Checked(inner) => format!("Checked{}", inner.default_name_within_union()),
            TypeRust::StreamState(inner) => {
                format!("StreamState{}", inner.default_name_within_union())
            }
            // Box is transparent for naming - it's just an implementation detail for recursion
            TypeRust::Boxed(inner) => inner.default_name_within_union(),
            // Base types
            TypeRust::String(val) => val.as_ref().map_or("String".to_string(), |v| {
                let safe_name = safe_name(v);
                format!("K{safe_name}")
            }),
            TypeRust::Int(val) => val.map_or("Int".to_string(), |v| format!("IntK{v}")),
            TypeRust::Float => "Float".to_string(),
            TypeRust::Bool(val) => val.map_or("Bool".to_string(), |v| {
                format!("BoolK{}", if v { "True" } else { "False" })
            }),
            TypeRust::Media(media_type_rust) => match media_type_rust {
                MediaTypeRust::Image => "Image".to_string(),
                MediaTypeRust::Audio => "Audio".to_string(),
                MediaTypeRust::Pdf => "PDF".to_string(),
                MediaTypeRust::Video => "Video".to_string(),
            },
            TypeRust::TypeAlias { name, .. } => name.clone(),
            TypeRust::Class { name, .. } => name.clone(),
            TypeRust::Union { name, .. } => name.clone(),
            TypeRust::Enum { name, .. } => name.clone(),
            TypeRust::List(type_rust) => format!("List{}", type_rust.default_name_within_union()),
            TypeRust::Map(key, value) => format!(
                "Map{}Key{}Value",
                key.default_name_within_union(),
                value.default_name_within_union()
            ),
            TypeRust::Any { .. } => "Any".to_string(),
        }
    }

    /// Returns the Rust zero/default value for this type.
    pub fn zero_value(&self, pkg: &CurrentRenderPackage) -> String {
        match self {
            TypeRust::Null => "()".to_string(),
            TypeRust::String(_) => "String::new()".to_string(),
            TypeRust::Int(_) => "0".to_string(),
            TypeRust::Float => "0.0".to_string(),
            TypeRust::Bool(_) => "false".to_string(),
            TypeRust::Optional(_) => "None".to_string(),
            TypeRust::List(_) => "Vec::new()".to_string(),
            TypeRust::Map(_, _) => "std::collections::HashMap::new()".to_string(),
            TypeRust::Class { name, package, .. } => {
                format!("{}{}::default()", package.relative_from(pkg), name)
            }
            TypeRust::Enum { name, package, .. } => {
                format!("{}{}::default()", package.relative_from(pkg), name)
            }
            TypeRust::Union { name, package } => {
                format!("{}{}::default()", package.relative_from(pkg), name)
            }
            TypeRust::TypeAlias { name, package } => {
                format!("{}{}::default()", package.relative_from(pkg), name)
            }
            TypeRust::Checked(inner) => {
                format!("baml::Checked::new({})", inner.zero_value(pkg))
            }
            TypeRust::StreamState(inner) => {
                format!("baml::StreamState::new({})", inner.zero_value(pkg))
            }
            TypeRust::Boxed(inner) => {
                format!("Box::new({})", inner.zero_value(pkg))
            }
            TypeRust::Media(_) => "Default::default()".to_string(),
            TypeRust::Any { .. } => "serde_json::Value::Null".to_string(),
        }
    }

    /// Returns true if this type is Optional.
    pub fn is_optional(&self) -> bool {
        matches!(self, TypeRust::Optional(_))
    }

    /// Returns the inner type if this is a wrapper (Optional, List, Checked, StreamState, Boxed).
    pub fn inner_type(&self) -> Option<&TypeRust> {
        match self {
            TypeRust::Optional(inner) => Some(inner),
            TypeRust::List(inner) => Some(inner),
            TypeRust::Checked(inner) => Some(inner),
            TypeRust::StreamState(inner) => Some(inner),
            TypeRust::Boxed(inner) => Some(inner),
            _ => None,
        }
    }

    /// Returns true if this type is a primitive (String, Int, Float, Bool, Null).
    pub fn is_primitive(&self) -> bool {
        matches!(
            self,
            TypeRust::Null
                | TypeRust::String(_)
                | TypeRust::Int(_)
                | TypeRust::Float
                | TypeRust::Bool(_)
        )
    }

    /// Returns true if this is a String type (with or without literal).
    pub fn is_string(&self) -> bool {
        matches!(self, TypeRust::String(_))
    }

    /// Generates code to decode a BamlValue into this type.
    /// `param` is the variable name holding the BamlValue.
    pub fn decode_from_value(&self, param: &str, pkg: &CurrentRenderPackage) -> String {
        format!("{}.get::<{}>()?", param, self.serialize_type(pkg))
    }
}

pub trait SerializeType {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String;
    fn serialize_type_as_parameter(&self, pkg: &CurrentRenderPackage) -> String;
}

impl SerializeType for TypeRust {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String {
        match self {
            TypeRust::Null => "()".to_string(),
            // Wrapper types
            TypeRust::Optional(inner) => format!("Option<{}>", inner.serialize_type(pkg)),
            TypeRust::Checked(inner) => format!(
                "{}Checked<{}>",
                Package::checked().relative_from(pkg),
                inner.serialize_type(pkg)
            ),
            TypeRust::StreamState(inner) => format!(
                "{}StreamState<{}>",
                Package::stream_state().relative_from(pkg),
                inner.serialize_type(pkg)
            ),
            TypeRust::Boxed(inner) => format!("Box<{}>", inner.serialize_type(pkg)),
            // Primitive types
            TypeRust::String(..) => "String".to_string(),
            TypeRust::Int(..) => "i64".to_string(),
            TypeRust::Float => "f64".to_string(),
            TypeRust::Bool(..) => "bool".to_string(),
            TypeRust::Media(media) => media.serialize_type(pkg),
            // Named types
            TypeRust::Class { package, name, .. } => {
                format!("{}{}", package.relative_from(pkg), name)
            }
            TypeRust::TypeAlias { package, name, .. } => {
                format!("{}{}", package.relative_from(pkg), name)
            }
            TypeRust::Union { package, name, .. } => {
                format!("{}{}", package.relative_from(pkg), name)
            }
            TypeRust::Enum { package, name, .. } => {
                format!("{}{}", package.relative_from(pkg), name)
            }
            // Container types
            TypeRust::List(inner) => format!("Vec<{}>", inner.serialize_type(pkg)),
            TypeRust::Map(key, value) => {
                format!(
                    "std::collections::HashMap<{}, {}>",
                    key.serialize_type(pkg),
                    value.serialize_type(pkg)
                )
            }
            TypeRust::Any { .. } => "serde_json::Value".to_string(),
        }
    }

    fn serialize_type_as_parameter(&self, pkg: &CurrentRenderPackage) -> String {
        match self {
            TypeRust::Null => "()".to_string(),
            TypeRust::Optional(inner) => {
                format!("Option<{}>", inner.serialize_type_as_parameter(pkg))
            }
            TypeRust::String(..) => "impl AsRef<str> + BamlEncode".to_string(),
            TypeRust::List(inner) => format!("&[{}]", inner.serialize_type(pkg)),
            TypeRust::Checked(_) => format!("&{}", self.serialize_type(pkg)),
            TypeRust::StreamState(_) => format!("&{}", self.serialize_type(pkg)),
            TypeRust::Int(..) => "i64".to_string(),
            TypeRust::Float => "f64".to_string(),
            TypeRust::Bool(..) => "bool".to_string(),
            TypeRust::Boxed(inner) => inner.serialize_type_as_parameter(pkg).to_string(),
            TypeRust::Media(media) => media.serialize_type_as_parameter(pkg),
            TypeRust::Class { .. }
            | TypeRust::TypeAlias { .. }
            | TypeRust::Union { .. }
            | TypeRust::Enum { .. }
            | TypeRust::Map(..) => format!("&{}", self.serialize_type(pkg)),
            TypeRust::Any { .. } => "serde_json::Value".to_string(),
        }
    }
}

impl SerializeType for MediaTypeRust {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String {
        match self {
            MediaTypeRust::Image => format!("{}Image", Package::types().relative_from(pkg)),
            MediaTypeRust::Audio => format!("{}Audio", Package::types().relative_from(pkg)),
            MediaTypeRust::Pdf => format!("{}Pdf", Package::types().relative_from(pkg)),
            MediaTypeRust::Video => format!("{}Video", Package::types().relative_from(pkg)),
        }
    }

    fn serialize_type_as_parameter(&self, pkg: &CurrentRenderPackage) -> String {
        format!("&{}", self.serialize_type(pkg))
    }
}
