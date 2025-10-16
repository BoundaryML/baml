use crate::package::{CurrentRenderPackage, Package};

#[derive(Clone, PartialEq, Debug, Default)]
pub enum TypeWrapper {
    #[default]
    None,
    Checked(Box<TypeWrapper>, Vec<EscapedPythonString>),
    Optional(Box<TypeWrapper>),
}

impl TypeWrapper {
    pub fn wrap_with_checked<T: AsRef<str>>(self, names: Vec<T>) -> TypeWrapper {
        TypeWrapper::Checked(
            Box::new(self),
            names
                .into_iter()
                .map(|i| EscapedPythonString::new(i.as_ref()))
                .collect(),
        )
    }
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct TypeMetaPy {
    pub type_wrapper: TypeWrapper,
    pub wrap_stream_state: bool,
}

impl TypeMetaPy {
    pub fn is_optional(&self) -> bool {
        matches!(self.type_wrapper, TypeWrapper::Optional(_))
    }

    pub fn is_checked(&self) -> bool {
        matches!(self.type_wrapper, TypeWrapper::Checked(_, _))
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
            TypeWrapper::Checked(inner, names) => format!(
                "{}Checked[{}, typing_extensions.Literal[{}]]",
                Package::checked().relative_from(pkg),
                inner.wrap_type(params),
                names
                    .iter()
                    .map(|n| n.0.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            TypeWrapper::Optional(inner) => format!("typing.Optional[{}]", inner.wrap_type(params)),
        }
    }
}

impl WrapType for TypeMetaPy {
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
    Literal(Vec<LiteralValue>, TypeMetaPy),
    String(TypeMetaPy),
    Int(TypeMetaPy),
    Float(TypeMetaPy),
    Bool(TypeMetaPy),
    Media(MediaTypePy, TypeMetaPy),
    // unions become classes
    Class {
        package: Package,
        name: String,
        dynamic: bool,
        meta: TypeMetaPy,
    },
    Union {
        variants: Vec<TypePy>,
        meta: TypeMetaPy,
    },
    Enum {
        package: Package,
        name: String,
        dynamic: bool,
        meta: TypeMetaPy,
    },
    TypeAlias {
        name: String,
        package: Package,
        meta: TypeMetaPy,
    },
    List(Box<TypePy>, TypeMetaPy),
    Map(Box<TypePy>, Box<TypePy>, TypeMetaPy),
    // For any type that we can't represent in Py, we'll use this
    Any {
        reason: String,
        meta: TypeMetaPy,
    },
}

impl TypePy {
    pub fn with_meta(mut self, meta: TypeMetaPy) -> Self {
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

    pub fn meta(&self) -> Option<&TypeMetaPy> {
        Some(match self {
            TypePy::Literal(_, meta) => meta,
            TypePy::String(meta) => meta,
            TypePy::Int(meta) => meta,
            TypePy::Float(meta) => meta,
            TypePy::Bool(meta) => meta,
            TypePy::Media(_, meta) => meta,
            TypePy::Class { meta, .. } => meta,
            TypePy::TypeAlias { meta, .. } => meta,
            TypePy::Union { meta, .. } => meta,
            TypePy::Enum { meta, .. } => meta,
            TypePy::List(_, meta) => meta,
            TypePy::Map(_, _, meta) => meta,
            TypePy::Any { meta, .. } => meta,
        })
    }

    pub fn meta_mut(&mut self) -> Option<&mut TypeMetaPy> {
        Some(match self {
            TypePy::Literal(_, meta) => meta,
            TypePy::String(meta) => meta,
            TypePy::Int(meta) => meta,
            TypePy::Float(meta) => meta,
            TypePy::Bool(meta) => meta,
            TypePy::Media(_, meta) => meta,
            TypePy::Class { meta, .. } => meta,
            TypePy::TypeAlias { meta, .. } => meta,
            TypePy::Union { meta, .. } => meta,
            TypePy::Enum { meta, .. } => meta,
            TypePy::List(_, meta) => meta,
            TypePy::Map(_, _, meta) => meta,
            TypePy::Any { meta, .. } => meta,
        })
    }
}

pub trait SerializeType {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String;
}

impl SerializeType for TypePy {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String {
        let meta = self.meta();
        let pkg = if meta
            .map(|m| m.is_optional() || m.wrap_stream_state || m.is_checked())
            .unwrap_or(false)
        {
            &pkg.in_type_definition()
        } else {
            pkg
        };

        let type_str = match self {
            TypePy::Literal(items, meta) => meta.wrap_type((
                pkg,
                format!(
                    "typing_extensions.Literal[{}]",
                    items
                        .iter()
                        .map(|s| s.serialize_type(pkg))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )),
            TypePy::String(_) => "str".to_string(),
            TypePy::Int(_) => "int".to_string(),
            TypePy::Float(_) => "float".to_string(),
            TypePy::Bool(_) => "bool".to_string(),
            TypePy::Media(media, _) => media.serialize_type(pkg),
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
            TypePy::List(inner, _) => format!(
                "typing.List[{}]",
                inner.serialize_type(&pkg.in_type_definition())
            ),
            TypePy::Map(key, value, _) => {
                let pkg = pkg.in_type_definition();
                format!(
                    "typing.Dict[{}, {}]",
                    key.serialize_type(&pkg),
                    value.serialize_type(&pkg)
                )
            }
            TypePy::Any { .. } => "typing.Any".to_string(),
        };

        match meta {
            Some(meta) => meta.wrap_type((pkg, type_str)),
            None => type_str,
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
