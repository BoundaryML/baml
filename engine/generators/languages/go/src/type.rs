use std::fmt::format;

use baml_types::baml_value::TypeLookups;

use crate::package::{CurrentRenderPackage, Package};

#[derive(Clone, PartialEq, Debug)]
pub enum MediaTypeGo {
    Image,
    Audio,
    Pdf,
    Video,
}

#[derive(Clone, PartialEq, Debug)]
pub enum TypeGo {
    // In case the user has an explicit Null type somewhere
    Null,
    // Primitive types (with optional literal value)
    String(Option<String>),
    Int(Option<i64>),
    Float,
    Bool(Option<bool>),
    Media(MediaTypeGo),
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
    List(Box<TypeGo>),
    Map(Box<TypeGo>, Box<TypeGo>),
    // Fallback type
    Any {
        reason: String,
    },
    // Wrapper types
    Optional(Box<TypeGo>),
    Checked(Box<TypeGo>),
    StreamState(Box<TypeGo>),
}

fn safe_name(name: &str) -> String {
    // replace all non-alphanumeric characters with an underscore
    name.replace(|c: char| !c.is_alphanumeric(), "_")
}

impl TypeGo {
    pub fn make_optional(self) -> Self {
        TypeGo::Optional(Box::new(self))
    }

    pub fn make_checked(self) -> Self {
        TypeGo::Checked(Box::new(self))
    }

    pub fn make_stream_state(self) -> Self {
        TypeGo::StreamState(Box::new(self))
    }

    pub fn flatten_unions(self) -> Vec<TypeGo> {
        match self {
            TypeGo::Union { .. } => {
                vec![self]
            }
            TypeGo::Optional(inner) => inner.flatten_unions(),
            TypeGo::Checked(inner) => inner.flatten_unions(),
            TypeGo::StreamState(inner) => inner.flatten_unions(),
            _ => vec![],
        }
    }

    // for unions, we need a default name for the type when the union is not named
    pub fn default_name_within_union(&self) -> String {
        match self {
            TypeGo::Null => "Null".to_string(),
            // Handle wrapper types first - they delegate to inner type with prefix
            TypeGo::Optional(inner) => format!("Optional{}", inner.default_name_within_union()),
            TypeGo::Checked(inner) => format!("Checked{}", inner.default_name_within_union()),
            TypeGo::StreamState(inner) => {
                format!("StreamState{}", inner.default_name_within_union())
            }
            // Base types
            TypeGo::String(val) => val.as_ref().map_or("String".to_string(), |v| {
                let safe_name = safe_name(v);
                format!("K{safe_name}")
            }),
            TypeGo::Int(val) => val.map_or("Int".to_string(), |v| format!("IntK{v}")),
            TypeGo::Float => "Float".to_string(),
            TypeGo::Bool(val) => val.map_or("Bool".to_string(), |v| {
                format!("BoolK{}", if v { "True" } else { "False" })
            }),
            TypeGo::Media(media_type_go) => match media_type_go {
                MediaTypeGo::Image => "Image".to_string(),
                MediaTypeGo::Audio => "Audio".to_string(),
                MediaTypeGo::Pdf => "PDF".to_string(),
                MediaTypeGo::Video => "Video".to_string(),
            },
            TypeGo::TypeAlias { name, .. } => name.clone(),
            TypeGo::Class { name, .. } => name.clone(),
            TypeGo::Union { name, .. } => name.clone(),
            TypeGo::Enum { name, .. } => name.clone(),
            TypeGo::List(type_go) => format!("List{}", type_go.default_name_within_union()),
            TypeGo::Map(key, value) => format!(
                "Map{}Key{}Value",
                key.default_name_within_union(),
                value.default_name_within_union()
            ),
            TypeGo::Any { .. } => "Any".to_string(),
        }
    }

    pub fn zero_value(&self, pkg: &CurrentRenderPackage) -> String {
        match self {
            TypeGo::Null => "(*interface{})(nil)".to_string(),
            TypeGo::Optional(_) => "nil".to_string(),
            TypeGo::Checked(_) | TypeGo::StreamState(_) => {
                format!("{}{{}}", self.serialize_type(pkg))
            }
            TypeGo::String(val) => val.as_ref().map_or("\"\"".to_string(), |v| {
                format!("\"{}\"", v.replace("\"", "\\\"")).to_string()
            }),
            TypeGo::Int(val) => val.map_or("0".to_string(), |v| format!("{v}")),
            TypeGo::Float => "0.0".to_string(),
            TypeGo::Bool(val) => val.map_or("false".to_string(), |v| {
                if v { "true" } else { "false" }.to_string()
            }),
            TypeGo::Media(..) | TypeGo::Class { .. } | TypeGo::Union { .. } => {
                format!("{}{{}}", self.serialize_type(pkg))
            }
            TypeGo::Enum { .. } => {
                format!("{}(\"\")", self.serialize_type(pkg))
            }
            TypeGo::TypeAlias { name, package, .. } => {
                let lookup = pkg.lookup();
                match lookup.expand_recursive_type(name) {
                    Ok(expansion) => {
                        if package == &Package::types() {
                            crate::ir_to_go::type_to_go(
                                &expansion.to_non_streaming_type(lookup),
                                lookup,
                            )
                            .zero_value(pkg)
                        } else {
                            crate::ir_to_go::stream_type_to_go(
                                &expansion.to_streaming_type(lookup),
                                lookup,
                            )
                            .zero_value(pkg)
                        }
                    }
                    Err(_) => format!("{}{{}}", self.serialize_type(pkg)),
                }
            }
            TypeGo::List(..) => "nil".to_string(),
            TypeGo::Map(..) => "nil".to_string(),
            TypeGo::Any { .. } => "nil".to_string(),
        }
    }

    pub fn construct_instance(&self, pkg: &CurrentRenderPackage) -> String {
        match self {
            TypeGo::Null => "(*interface{})(nil)".to_string(),
            TypeGo::Optional(inner) => {
                let base_type = inner.serialize_type(pkg);
                format!("(*{base_type})(nil)")
            }
            TypeGo::Checked(_) | TypeGo::StreamState(_) => {
                format!("{}{{}}", self.serialize_type(pkg))
            }
            TypeGo::String(val) => val.as_ref().map_or("\"\"".to_string(), |v| {
                format!("\"{}\"", v.replace("\"", "\\\"")).to_string()
            }),
            TypeGo::Int(val) => val.map_or("int64(0)".to_string(), |v| format!("int64({v})")),
            TypeGo::Float => "float64(0.0)".to_string(),
            TypeGo::Bool(val) => val.map_or("false".to_string(), |v| {
                if v { "true" } else { "false" }.to_string()
            }),
            TypeGo::Media(..) | TypeGo::Class { .. } | TypeGo::Union { .. } => {
                format!("{}{{}}", self.serialize_type(pkg))
            }
            TypeGo::Enum { .. } => {
                format!("{}(\"\")", self.serialize_type(pkg))
            }
            TypeGo::TypeAlias { .. } => {
                format!("{}{{}}", self.serialize_type(pkg))
            }
            TypeGo::List(inner) => format!("[]{}{{}}", inner.serialize_type(pkg)),
            TypeGo::Map(key, value) => {
                format!(
                    "map[{}]{}{{}}",
                    key.serialize_type(pkg),
                    value.serialize_type(pkg)
                )
            }
            TypeGo::Any { .. } => "any".to_string(),
        }
    }

    pub fn cast_from_function(&self, param: &str, pkg: &CurrentRenderPackage) -> String {
        format!("({param}).({})", self.serialize_type(pkg))
    }

    pub fn decode_from_any(&self, param: &str, pkg: &CurrentRenderPackage) -> String {
        match self {
            TypeGo::Null => self.zero_value(pkg),
            TypeGo::Bool(_) => {
                format!("baml.Decode({param}).Bool()")
            }
            TypeGo::Int(_) => {
                format!("baml.Decode({param}).Int()")
            }
            TypeGo::Float => {
                format!("baml.Decode({param}).Float()")
            }
            _ => format!(
                "baml.Decode({param}).Interface().({})",
                self.serialize_type(pkg)
            ),
        }
    }
}

pub trait SerializeType {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String;
}

impl SerializeType for TypeGo {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String {
        match self {
            TypeGo::Null => "*interface{}".to_string(),
            // Wrapper types
            TypeGo::Optional(inner) => format!("*{}", inner.serialize_type(pkg)),
            TypeGo::Checked(inner) => format!(
                "{}Checked[{}]",
                Package::checked().relative_from(pkg),
                inner.serialize_type(pkg)
            ),
            TypeGo::StreamState(inner) => format!(
                "{}StreamState[{}]",
                Package::stream_state().relative_from(pkg),
                inner.serialize_type(pkg)
            ),
            // Primitive types
            TypeGo::String(..) => "string".to_string(),
            TypeGo::Int(..) => "int64".to_string(),
            TypeGo::Float => "float64".to_string(),
            TypeGo::Bool(..) => "bool".to_string(),
            TypeGo::Media(media) => media.serialize_type(pkg),
            // Named types
            TypeGo::Class { package, name, .. } => {
                format!("{}{}", package.relative_from(pkg), name)
            }
            TypeGo::TypeAlias { package, name, .. } => {
                format!("{}{}", package.relative_from(pkg), name)
            }
            TypeGo::Union { package, name, .. } => {
                format!("{}{}", package.relative_from(pkg), name)
            }
            TypeGo::Enum { package, name, .. } => format!("{}{}", package.relative_from(pkg), name),
            // Container types
            TypeGo::List(inner) => format!("[]{}", inner.serialize_type(pkg)),
            TypeGo::Map(key, value) => {
                format!(
                    "map[{}]{}",
                    key.serialize_type(pkg),
                    value.serialize_type(pkg)
                )
            }
            TypeGo::Any { .. } => "any".to_string(),
        }
    }
}

impl SerializeType for MediaTypeGo {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String {
        match self {
            MediaTypeGo::Image => format!("{}Image", Package::types().relative_from(pkg)),
            MediaTypeGo::Audio => format!("{}Audio", Package::types().relative_from(pkg)),
            MediaTypeGo::Pdf => format!("{}PDF", Package::types().relative_from(pkg)),
            MediaTypeGo::Video => format!("{}Video", Package::types().relative_from(pkg)),
        }
    }
}
