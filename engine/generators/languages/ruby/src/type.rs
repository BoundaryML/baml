use crate::package::{CurrentRenderPackage, Package};

#[derive(Clone, PartialEq, Debug, Default)]
pub enum TypeWrapper {
    #[default]
    None,
    Checked(Box<TypeWrapper>),
    Optional(Box<TypeWrapper>),
}

impl TypeWrapper {
    pub fn wrap_with_checked<T: AsRef<str>>(self, _names: Vec<T>) -> TypeWrapper {
        TypeWrapper::Checked(Box::new(self))
    }
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct TypeMetaRb {
    pub type_wrapper: TypeWrapper,
    pub wrap_stream_state: bool,
}

impl TypeMetaRb {
    pub fn is_optional(&self) -> bool {
        matches!(self.type_wrapper, TypeWrapper::Optional(_))
    }

    pub fn is_checked(&self) -> bool {
        matches!(self.type_wrapper, TypeWrapper::Checked(_))
    }

    pub fn make_checked<T: AsRef<str>>(&mut self, names: Vec<T>) -> &mut Self {
        self.type_wrapper = std::mem::take(&mut self.type_wrapper).wrap_with_checked(names);
        self
    }

    pub fn make_optional(&mut self) -> &mut Self {
        self.type_wrapper = TypeWrapper::Optional(Box::new(std::mem::take(&mut self.type_wrapper)));
        self
    }

    pub fn set_stream_state(&mut self) -> &mut Self {
        self.wrap_stream_state = true;
        self
    }
}

trait WrapType {
    fn wrap_type(&self, params: (&CurrentRenderPackage, String)) -> String;
}

impl WrapType for TypeWrapper {
    fn wrap_type(&self, params: (&CurrentRenderPackage, String)) -> String {
        let (pkg, orig) = &params;
        match self {
            TypeWrapper::None => orig.clone(),
            TypeWrapper::Checked(inner) => format!(
                "{}Checked[{}]",
                Package::checked().relative_from(pkg),
                inner.wrap_type(params),
            ),
            TypeWrapper::Optional(inner) => format!("T.nilable({})", inner.wrap_type(params)),
        }
    }
}

impl WrapType for TypeMetaRb {
    fn wrap_type(&self, params: (&CurrentRenderPackage, String)) -> String {
        let pkg = params.0;
        let wrapped = self.type_wrapper.wrap_type(params);
        if self.wrap_stream_state {
            format!(
                "{}StreamState[{}]",
                Package::stream_state().relative_from(pkg),
                wrapped
            )
        } else {
            wrapped
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum MediaTypeRb {
    Image,
    Audio,
    Pdf,
    Video,
}

#[derive(Clone, PartialEq, Debug)]
pub struct EscapedRubyString(String);

impl EscapedRubyString {
    pub fn new(s: &str) -> Self {
        let has_single_quote = s.contains('\'');
        let has_double_quote = s.contains('"');
        let has_newline = s.contains('\n');
        if has_newline {
            return Self(format!("\"\"\"{s}\"\"\""));
        }
        match (has_single_quote, has_double_quote) {
            (true, false) => Self(format!("\"{s}\"")),
            (true, true) => Self(format!("'{}'", s.replace('\'', "\\'"))),
            (false, true) => Self(format!("'{s}'")),
            (false, false) => Self(format!("'{s}'")),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum TypeRb {
    // in case a literal
    String(Option<String>, TypeMetaRb),
    Int(Option<i64>, TypeMetaRb),
    Bool(Option<bool>, TypeMetaRb),

    Float(TypeMetaRb),
    Media(MediaTypeRb, TypeMetaRb),
    // unions become classes
    Class {
        package: Package,
        name: String,
        dynamic: bool,
        meta: TypeMetaRb,
    },
    Union {
        variants: Vec<TypeRb>,
        meta: TypeMetaRb,
    },
    Enum {
        package: Package,
        name: String,
        dynamic: bool,
        meta: TypeMetaRb,
    },
    TypeAlias {
        name: String,
        package: Package,
        meta: TypeMetaRb,
    },
    List(Box<TypeRb>, TypeMetaRb),
    Map(Box<TypeRb>, Box<TypeRb>, TypeMetaRb),
    // For any type that we can't represent in Go, we'll use this
    Any {
        reason: String,
        meta: TypeMetaRb,
    },
}

impl TypeRb {
    pub fn with_meta(mut self, meta: TypeMetaRb) -> Self {
        if let Some(m) = self.meta_mut() {
            *m = meta;
        }
        self
    }

    pub fn is_optional(&self) -> bool {
        self.meta().map(|m| m.is_optional()).unwrap_or(false)
    }

    pub fn default_value(&self) -> Option<String> {
        let meta = self.meta();
        match meta {
            Some(meta) => {
                if meta.is_optional() && !meta.wrap_stream_state {
                    Some("None".to_string())
                } else {
                    None
                }
            }
            None => None,
        }
    }

    pub fn meta(&self) -> Option<&TypeMetaRb> {
        Some(match self {
            TypeRb::String(_, meta) => meta,
            TypeRb::Int(_, meta) => meta,
            TypeRb::Bool(_, meta) => meta,
            TypeRb::Float(meta) => meta,
            TypeRb::Media(_, meta) => meta,
            TypeRb::Class { meta, .. } => meta,
            TypeRb::TypeAlias { meta, .. } => meta,
            TypeRb::Union { meta, .. } => meta,
            TypeRb::Enum { meta, .. } => meta,
            TypeRb::List(_, meta) => meta,
            TypeRb::Map(_, _, meta) => meta,
            TypeRb::Any { meta, .. } => meta,
        })
    }

    pub fn meta_mut(&mut self) -> Option<&mut TypeMetaRb> {
        Some(match self {
            TypeRb::String(_, meta) => meta,
            TypeRb::Int(_, meta) => meta,
            TypeRb::Bool(_, meta) => meta,
            TypeRb::Float(meta) => meta,
            TypeRb::Media(_, meta) => meta,
            TypeRb::Class { meta, .. } => meta,
            TypeRb::TypeAlias { meta, .. } => meta,
            TypeRb::Union { meta, .. } => meta,
            TypeRb::Enum { meta, .. } => meta,
            TypeRb::List(_, meta) => meta,
            TypeRb::Map(_, _, meta) => meta,
            TypeRb::Any { meta, .. } => meta,
        })
    }
}

pub trait SerializeType {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String;
}

impl SerializeType for TypeRb {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String {
        let meta = self.meta();

        let type_str: String = match self {
            TypeRb::String(..) => "String".to_string(),
            TypeRb::Int(..) => "Integer".to_string(),
            TypeRb::Bool(..) => "T::Boolean".to_string(),
            TypeRb::Float(_) => "Float".to_string(),
            TypeRb::Media(media, _) => media.serialize_type(pkg),
            TypeRb::Class { package, name, .. } | TypeRb::TypeAlias { package, name, .. } => {
                if pkg.is_defining_alias(name) {
                    // Recursive types are not supported in sorbet, so we use T.anything
                    "T.anything".to_string()
                } else {
                    format!("{}{}", package.relative_from(pkg), name)
                }
            }
            TypeRb::Union { variants, .. } => {
                format!(
                    "T.any({})",
                    variants
                        .iter()
                        .map(|v| v.serialize_type(pkg))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            TypeRb::Enum {
                package,
                name,
                dynamic,
                ..
            } => {
                let enm = format!("{}{}", package.relative_from(pkg), name);
                if *dynamic {
                    format!("T.any({enm}, String)")
                } else {
                    enm
                }
            }
            TypeRb::List(inner, _) => format!("T::Array[{}]", inner.serialize_type(pkg)),
            TypeRb::Map(key, value, _) => {
                format!(
                    "T::Hash[{}, {}]",
                    key.serialize_type(pkg),
                    value.serialize_type(pkg)
                )
            }
            TypeRb::Any { .. } => "T.anything".to_string(),
        };

        match meta {
            Some(meta) => meta.wrap_type((pkg, type_str)),
            None => type_str,
        }
    }
}

impl SerializeType for MediaTypeRb {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String {
        match self {
            MediaTypeRb::Image => format!("{}Image", Package::imported_base().relative_from(pkg)),
            MediaTypeRb::Audio => format!("{}Audio", Package::imported_base().relative_from(pkg)),
            MediaTypeRb::Pdf => format!("{}Pdf", Package::imported_base().relative_from(pkg)),
            MediaTypeRb::Video => format!("{}Video", Package::imported_base().relative_from(pkg)),
        }
    }
}
