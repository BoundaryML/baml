use crate::package::{CurrentRenderPackage, Package};

#[derive(Clone, PartialEq, Debug, Default)]
pub enum TypeWrapper {
    #[default]
    None,
    Checked(Box<TypeWrapper>, Vec<Option<String>>),
    Optional(Box<TypeWrapper>),
}

impl TypeWrapper {
    pub fn wrap_with_checked(self, names: Vec<Option<String>>) -> TypeWrapper {
        TypeWrapper::Checked(Box::new(self), names)
    }
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct TypeMetaTS {
    pub type_wrapper: TypeWrapper,
    pub wrap_stream_state: bool,
}

impl TypeMetaTS {
    pub fn is_optional(&self) -> bool {
        matches!(self.type_wrapper, TypeWrapper::Optional(_))
    }

    pub fn make_checked(&mut self, names: Vec<Option<String>>) -> &mut Self {
        self.type_wrapper =
            TypeWrapper::Checked(Box::new(std::mem::take(&mut self.type_wrapper)), names);
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
            TypeWrapper::Checked(inner, names) => {
                let mut names = names.clone();
                names.dedup();
                names.sort();
                format!(
                    "{}Checked<{},{}>",
                    Package::checked().relative_from(pkg),
                    inner.wrap_type(params),
                    names
                        .iter()
                        .filter_map(|n| n.as_ref().map(|n| format!("\"{n}\"")))
                        .collect::<Vec<_>>()
                        .join(" | ")
                )
            }
            TypeWrapper::Optional(inner) => format!("{} | null", inner.wrap_type(params)),
        }
    }
}

impl WrapType for TypeMetaTS {
    fn wrap_type(&self, params: (&CurrentRenderPackage, String)) -> String {
        let pkg = params.0;
        let wrapped = self.type_wrapper.wrap_type(params);
        if self.wrap_stream_state {
            format!(
                "{}StreamState<{}>",
                Package::stream_state().relative_from(pkg),
                wrapped
            )
        } else {
            wrapped
        }
    }
}

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
    Literal(LiteralValue, TypeMetaTS),
    String(TypeMetaTS),
    Int(TypeMetaTS),
    Float(TypeMetaTS),
    Bool(TypeMetaTS),
    Media(MediaTypeTS, TypeMetaTS),
    // unions become classes
    Class {
        package: Package,
        name: String,
        dynamic: bool,
        meta: TypeMetaTS,
    },
    Union {
        variants: Vec<TypeTS>,
        meta: TypeMetaTS,
    },
    Enum {
        package: Package,
        name: String,
        dynamic: bool,
        meta: TypeMetaTS,
    },
    TypeAlias {
        name: String,
        package: Package,
        meta: TypeMetaTS,
    },
    List(Box<TypeTS>, TypeMetaTS),
    Map(Box<TypeTS>, Box<TypeTS>, TypeMetaTS),
    Interface {
        package: Package,
        name: String,
        meta: TypeMetaTS,
    },
    // For any type that we can't represent in TS, we'll use this
    Any {
        reason: String,
        meta: TypeMetaTS,
    },
}

impl TypeTS {
    // for unions, we need a default name for the type when the union is not named
    pub fn default_name_within_union(&self) -> String {
        match self {
            TypeTS::Literal(val, _) => val.serialize_type(),
            TypeTS::String(_) => "string".to_string(),
            TypeTS::Int(_) => "number".to_string(),
            TypeTS::Float(_) => "number".to_string(),
            TypeTS::Bool(_) => "boolean".to_string(),
            TypeTS::Media(media_type, _) => match media_type {
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
            TypeTS::List(inner, _) => format!("{}[]", inner.default_name_within_union()),
            TypeTS::Map(key, value, _) => format!(
                "Record<{}, {}>",
                key.default_name_within_union(),
                value.default_name_within_union()
            ),
            TypeTS::Interface { name, .. } => name.clone(),
            TypeTS::Any { .. } => "any".to_string(),
        }
    }

    pub fn meta(&self) -> &TypeMetaTS {
        match self {
            TypeTS::Literal(_, meta) => meta,
            TypeTS::String(meta) => meta,
            TypeTS::Int(meta) => meta,
            TypeTS::Float(meta) => meta,
            TypeTS::Bool(meta) => meta,
            TypeTS::Media(_, meta) => meta,
            TypeTS::Class { meta, .. } => meta,
            TypeTS::TypeAlias { meta, .. } => meta,
            TypeTS::Union { meta, .. } => meta,
            TypeTS::Enum { meta, .. } => meta,
            TypeTS::List(_, meta) => meta,
            TypeTS::Map(_, _, meta) => meta,
            TypeTS::Interface { meta, .. } => meta,
            TypeTS::Any { meta, .. } => meta,
        }
    }

    pub fn meta_mut(&mut self) -> &mut TypeMetaTS {
        match self {
            TypeTS::Literal(_, meta) => meta,
            TypeTS::String(meta) => meta,
            TypeTS::Int(meta) => meta,
            TypeTS::Float(meta) => meta,
            TypeTS::Bool(meta) => meta,
            TypeTS::Media(_, meta) => meta,
            TypeTS::Class { meta, .. } => meta,
            TypeTS::TypeAlias { meta, .. } => meta,
            TypeTS::Union { meta, .. } => meta,
            TypeTS::Enum { meta, .. } => meta,
            TypeTS::List(_, meta) => meta,
            TypeTS::Map(_, _, meta) => meta,
            TypeTS::Interface { meta, .. } => meta,
            TypeTS::Any { meta, .. } => meta,
        }
    }

    pub fn with_meta(mut self, meta: TypeMetaTS) -> Self {
        *(self.meta_mut()) = meta;
        self
    }
}

pub trait SerializeType {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String;
}

impl SerializeType for TypeTS {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String {
        let meta = self.meta();
        let type_str = match self {
            TypeTS::Literal(val, _) => val.serialize_type(),
            TypeTS::String(_) => "string".to_string(),
            TypeTS::Int(_) => "number".to_string(),
            TypeTS::Float(_) => "number".to_string(),
            TypeTS::Bool(_) => "boolean".to_string(),
            TypeTS::Media(media, _) => match media {
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
            TypeTS::Union { .. } => self.default_name_within_union(),
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
            TypeTS::List(inner, _) => match &**inner {
                TypeTS::Union { .. } => format!("({})[]", inner.default_name_within_union()),
                _ => {
                    if inner.meta().is_optional() {
                        format!("({})[]", inner.serialize_type(pkg))
                    } else {
                        format!("{}[]", inner.serialize_type(pkg))
                    }
                }
            },
            TypeTS::Map(key, value, _) => {
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
        };

        meta.wrap_type((pkg, type_str))
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
