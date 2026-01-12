use crate::package::{CurrentRenderPackage, Package};

#[derive(Clone, PartialEq, Debug)]
pub enum MediaTypePy {
    Image,
    Audio,
    Pdf,
    Video,
}

#[derive(Clone, PartialEq, Debug)]
pub struct EscapedPythonString(String);

impl EscapedPythonString {
    pub fn new(s: &str) -> Self {
        // Escape special characters and wrap in quotes
        // We always use single quotes and escape as needed
        let escaped = s
            .replace('\\', "\\\\")
            .replace('\'', "\\'")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t");
        Self(format!("'{}'", escaped))
    }
}

impl std::fmt::Display for EscapedPythonString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum LiteralValue {
    String(EscapedPythonString),
    Int(i64),
    Bool(bool),
    None,
}

impl LiteralValue {
    pub fn serialize_type(&self, _pkg: &CurrentRenderPackage) -> String {
        match self {
            LiteralValue::String(s) => s.0.clone(),
            LiteralValue::Int(i) => i.to_string(),
            LiteralValue::Bool(b) => if *b { "True" } else { "False" }.to_string(),
            LiteralValue::None => "None".to_string(),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum TypePy {
    Literal(Vec<LiteralValue>),
    String,
    Int,
    Float,
    Bool,
    Media(MediaTypePy),
    // unions become classes
    Class {
        package: Package,
        name: String,
        dynamic: bool,
    },
    Union {
        variants: Vec<TypePy>,
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
    List(Box<TypePy>),
    Map(Box<TypePy>, Box<TypePy>),
    Optional(Box<TypePy>),
    Checked {
        inner: Box<TypePy>,
        names: Vec<EscapedPythonString>,
    },
    StreamState(Box<TypePy>),
    // For any type that we can't represent in Py, we'll use this
    Any {
        reason: String,
    },
}

impl TypePy {
    pub fn make_optional(self) -> Self {
        TypePy::Optional(Box::new(self))
    }

    pub fn make_checked<T: AsRef<str>>(self, names: Vec<T>) -> Self {
        TypePy::Checked {
            inner: Box::new(self),
            names: names
                .into_iter()
                .map(|s| EscapedPythonString::new(s.as_ref()))
                .collect(),
        }
    }

    pub fn make_stream_state(self) -> Self {
        TypePy::StreamState(Box::new(self))
    }

    pub fn is_optional(&self) -> bool {
        matches!(self, TypePy::Optional(..))
    }

    pub fn default_value(&self) -> Option<String> {
        match self {
            TypePy::StreamState(_) => None, // StreamState never has default
            TypePy::Optional(..) => Some("None".to_string()),
            _ => None,
        }
    }
}

pub trait SerializeType {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String;
}

impl SerializeType for TypePy {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String {
        let is_wrapped = matches!(
            self,
            TypePy::Optional(..) | TypePy::Checked { .. } | TypePy::StreamState(_)
        );
        let pkg = if is_wrapped {
            &pkg.in_type_definition()
        } else {
            pkg
        };

        match self {
            TypePy::Literal(items) => {
                format!(
                    "typing_extensions.Literal[{}]",
                    items
                        .iter()
                        .map(|s| s.serialize_type(pkg))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            TypePy::String => "str".to_string(),
            TypePy::Int => "int".to_string(),
            TypePy::Float => "float".to_string(),
            TypePy::Bool => "bool".to_string(),
            TypePy::Media(media) => media.serialize_type(pkg),
            TypePy::Class { package, name, .. } | TypePy::TypeAlias { package, name, .. } => {
                if pkg.get().in_type_definition() {
                    format!("\"{}{}\"", package.relative_from(pkg), name)
                } else {
                    format!("{}{}", package.relative_from(pkg), name)
                }
            }
            TypePy::Union { variants, .. } => {
                let pkg = pkg.in_type_definition();
                format!(
                    "typing.Union[{}]",
                    variants
                        .iter()
                        .map(|v| v.serialize_type(&pkg))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            TypePy::Enum {
                package,
                name,
                dynamic,
                ..
            } => {
                let enm = format!("{}{}", package.relative_from(pkg), name);
                if *dynamic {
                    format!("typing.Union[{enm}, str]")
                } else {
                    enm
                }
            }
            TypePy::List(inner) => format!(
                "typing.List[{}]",
                inner.serialize_type(&pkg.in_type_definition())
            ),
            TypePy::Map(key, value) => {
                let pkg = pkg.in_type_definition();
                format!(
                    "typing.Dict[{}, {}]",
                    key.serialize_type(&pkg),
                    value.serialize_type(&pkg)
                )
            }
            TypePy::Optional(inner) => format!("typing.Optional[{}]", inner.serialize_type(pkg)),
            TypePy::Checked { inner, names } => format!(
                "{}Checked[{}, typing_extensions.Literal[{}]]",
                Package::checked().relative_from(pkg),
                inner.serialize_type(pkg),
                names
                    .iter()
                    .map(|n| n.0.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            TypePy::StreamState(inner) => format!(
                "{}StreamState[{}]",
                Package::stream_state().relative_from(pkg),
                inner.serialize_type(pkg)
            ),
            TypePy::Any { .. } => "typing.Any".to_string(),
        }
    }
}

impl SerializeType for MediaTypePy {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String {
        match self {
            MediaTypePy::Image => format!("{}Image", Package::imported_base().relative_from(pkg)),
            MediaTypePy::Audio => format!("{}Audio", Package::imported_base().relative_from(pkg)),
            MediaTypePy::Pdf => format!("{}Pdf", Package::imported_base().relative_from(pkg)),
            MediaTypePy::Video => format!("{}Video", Package::imported_base().relative_from(pkg)),
        }
    }
}
