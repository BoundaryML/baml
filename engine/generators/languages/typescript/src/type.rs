use crate::package::{CurrentRenderPackage, Package};

#[derive(Clone, PartialEq, Debug)]
pub enum MediaTypeTS {
    Image,
    Audio,
    Pdf,
    Video,
}

#[derive(Clone, PartialEq, Debug)]
pub enum LiteralValue {
    String(String),
    Int(i64),
    Bool(bool),
}

// https://www.typescriptlang.org/docs/handbook/literal-types.html
impl LiteralValue {
    pub fn serialize_type(&self) -> String {
        match self {
            LiteralValue::String(s) => format!("\"{s}\""),
            LiteralValue::Int(i) => i.to_string(),
            LiteralValue::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum TypeTS {
    Literal(LiteralValue),
    String,
    Int,
    Float,
    Bool,
    Media(MediaTypeTS),
    // unions become classes
    Class {
        package: Package,
        name: String,
        dynamic: bool,
    },
    Union {
        variants: Vec<TypeTS>,
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
    List(Box<TypeTS>),
    Map(Box<TypeTS>, Box<TypeTS>),
    Interface {
        package: Package,
        name: String,
    },
    // For any type that we can't represent in TS, we'll use this
    Any {
        reason: String,
    },
    // Wrapper types
    Optional(Box<TypeTS>),
    Checked {
        inner: Box<TypeTS>,
        names: Vec<Option<String>>,
    },
    StreamState(Box<TypeTS>),
}

impl TypeTS {
    pub fn make_optional(self) -> Self {
        TypeTS::Optional(Box::new(self))
    }

    pub fn make_checked(self, names: Vec<Option<String>>) -> Self {
        TypeTS::Checked {
            inner: Box::new(self),
            names,
        }
    }

    pub fn is_optional(&self) -> bool {
        matches!(self, TypeTS::Optional(..))
    }

    pub fn make_stream_state(self) -> Self {
        TypeTS::StreamState(Box::new(self))
    }

    // for unions, we need a default name for the type when the union is not named
    pub fn default_name_within_union(&self) -> String {
        match self {
            TypeTS::Literal(val) => val.serialize_type(),
            TypeTS::String => "string".to_string(),
            TypeTS::Int => "number".to_string(),
            TypeTS::Float => "number".to_string(),
            TypeTS::Bool => "boolean".to_string(),
            TypeTS::Media(media_type) => match media_type {
                MediaTypeTS::Image => "Image".to_string(),
                MediaTypeTS::Audio => "Audio".to_string(),
                MediaTypeTS::Pdf => "Pdf".to_string(),
                MediaTypeTS::Video => "Video".to_string(),
            },
            TypeTS::TypeAlias { name, .. } => name.clone(),
            TypeTS::Class { name, .. } => name.clone(),
            TypeTS::Union { variants, .. } => variants
                .iter()
                .map(|v| v.default_name_within_union())
                .collect::<Vec<_>>()
                .join(" | "),
            TypeTS::Enum { name, .. } => name.clone(),
            TypeTS::List(inner) => format!("{}[]", inner.default_name_within_union()),
            TypeTS::Map(key, value) => format!(
                "Record<{}, {}>",
                key.default_name_within_union(),
                value.default_name_within_union()
            ),
            TypeTS::Interface { name, .. } => name.clone(),
            TypeTS::Any { .. } => "any".to_string(),
            TypeTS::Optional(inner) => format!("{} | null", inner.default_name_within_union()),
            TypeTS::Checked { inner, names, .. } => {
                let mut names = names.clone();
                names.dedup();
                names.sort();
                format!(
                    "Checked<{},{}>",
                    inner.default_name_within_union(),
                    names
                        .iter()
                        .filter_map(|n| n.as_ref().map(|n| format!("\"{n}\"")))
                        .collect::<Vec<_>>()
                        .join(" | ")
                )
            }
            TypeTS::StreamState(inner) => {
                format!("StreamState<{}>", inner.default_name_within_union())
            }
        }
    }
}

pub trait SerializeType {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String;
}

impl SerializeType for TypeTS {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String {
        match self {
            TypeTS::Literal(val) => val.serialize_type(),
            TypeTS::String => "string".to_string(),
            TypeTS::Int => "number".to_string(),
            TypeTS::Float => "number".to_string(),
            TypeTS::Bool => "boolean".to_string(),
            TypeTS::Media(media) => match media {
                MediaTypeTS::Image => "Image".to_string(),
                MediaTypeTS::Audio => "Audio".to_string(),
                MediaTypeTS::Pdf => "Pdf".to_string(),
                MediaTypeTS::Video => "Video".to_string(),
            },
            TypeTS::Class { package, name, .. } => {
                format!("{}{}", package.relative_from(pkg), name)
            }
            TypeTS::TypeAlias { package, name, .. } => {
                format!("{}{}", package.relative_from(pkg), name)
            }
            TypeTS::Union { variants, .. } => {
                let mut parts: Vec<String> = Vec::with_capacity(variants.len());
                for variant in variants.iter() {
                    let rendered = variant.serialize_type(pkg);
                    if !parts.contains(&rendered) {
                        parts.push(rendered);
                    }
                }
                parts.join(" | ")
            }
            TypeTS::Enum {
                package,
                name,
                dynamic,
                ..
            } => {
                if *dynamic {
                    format!("(string | {}{})", package.relative_from(pkg), name)
                } else {
                    format!("{}{}", package.relative_from(pkg), name)
                }
            }
            TypeTS::List(inner) => match &**inner {
                TypeTS::Union { .. } => format!("({})[]", inner.serialize_type(pkg)),
                _ => {
                    if inner.is_optional() {
                        format!("({})[]", inner.serialize_type(pkg))
                    } else {
                        format!("{}[]", inner.serialize_type(pkg))
                    }
                }
            },
            TypeTS::Map(key, value) => {
                let k = key.serialize_type(pkg);
                let v = value.serialize_type(pkg);
                match &**key {
                    TypeTS::Enum { .. } | TypeTS::Union { .. } => {
                        format!("Partial<Record<{k}, {v}>>")
                    }
                    _ => format!("Record<{k}, {v}>"),
                }
            }
            TypeTS::Interface { package, name, .. } => {
                format!("{}{}", package.relative_from(pkg), name)
            }
            TypeTS::Any { .. } => "undefined".to_string(),
            TypeTS::Optional(inner) => format!("{} | null", inner.serialize_type(pkg)),
            TypeTS::Checked { inner, names, .. } => {
                let mut names = names.clone();
                names.dedup();
                names.sort();
                format!(
                    "{}Checked<{},{}>",
                    Package::checked().relative_from(pkg),
                    inner.serialize_type(pkg),
                    names
                        .iter()
                        .filter_map(|n| n.as_ref().map(|n| format!("\"{n}\"")))
                        .collect::<Vec<_>>()
                        .join(" | ")
                )
            }
            TypeTS::StreamState(inner) => {
                format!(
                    "{}StreamState<{}>",
                    Package::stream_state().relative_from(pkg),
                    inner.serialize_type(pkg)
                )
            }
        }
    }
}

impl SerializeType for MediaTypeTS {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String {
        match self {
            MediaTypeTS::Image => format!("{}.Image", Package::imported_base().relative_from(pkg)),
            MediaTypeTS::Audio => format!("{}.Audio", Package::imported_base().relative_from(pkg)),
            MediaTypeTS::Pdf => format!("{}.Pdf", Package::imported_base().relative_from(pkg)),
            MediaTypeTS::Video => format!("{}.Video", Package::imported_base().relative_from(pkg)),
        }
    }
}
