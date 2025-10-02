use crate::package::{CurrentRenderPackage, Package};
use baml_types::ir_type::{TypeNonStreaming, TypeValue};

#[derive(Clone, PartialEq, Debug, Default)]
pub enum TypeWrapper {
    #[default]
    None,
    Checked(Box<TypeWrapper>, Vec<String>),
    Optional(Box<TypeWrapper>),
    Boxed(Box<TypeWrapper>),
}

impl TypeWrapper {
    pub fn wrap_with_checked(self, names: Vec<String>) -> TypeWrapper {
        TypeWrapper::Checked(Box::new(self), names)
    }

    pub fn pop_optional(&mut self) -> &mut Self {
        match self {
            TypeWrapper::Optional(inner) => {
                *self = std::mem::take(inner);
                self
            }
            _ => self,
        }
    }

    pub fn pop_checked(&mut self) -> &mut Self {
        match self {
            TypeWrapper::Checked(inner, _) => {
                *self = std::mem::take(inner);
                self
            }
            _ => self,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct TypeMetaRust {
    pub type_wrapper: TypeWrapper,
    pub wrap_stream_state: bool,
}

impl TypeMetaRust {
    pub fn is_optional(&self) -> bool {
        matches!(self.type_wrapper, TypeWrapper::Optional(_))
    }

    pub fn is_checked(&self) -> bool {
        matches!(self.type_wrapper, TypeWrapper::Checked(_, _))
    }

    pub fn make_checked(&mut self, names: Vec<String>) -> &mut Self {
        self.type_wrapper =
            TypeWrapper::Checked(Box::new(std::mem::take(&mut self.type_wrapper)), names);
        self
    }

    pub fn make_optional(&mut self) -> &mut Self {
        self.type_wrapper = TypeWrapper::Optional(Box::new(std::mem::take(&mut self.type_wrapper)));
        self
    }

    pub fn make_boxed(&mut self) -> &mut Self {
        self.type_wrapper = TypeWrapper::Boxed(Box::new(std::mem::take(&mut self.type_wrapper)));
        self
    }

    pub fn set_stream_state(&mut self) -> &mut Self {
        self.wrap_stream_state = true;
        self
    }
}

pub trait WrapType {
    fn wrap_type(&self, params: (&CurrentRenderPackage, String)) -> String;
}

impl WrapType for TypeWrapper {
    fn wrap_type(&self, params: (&CurrentRenderPackage, String)) -> String {
        let (pkg, orig) = &params;
        match self {
            TypeWrapper::None => orig.clone(),
            TypeWrapper::Checked(inner, _names) => inner.wrap_type(params),
            TypeWrapper::Boxed(inner) => format!("Box<{}>", inner.wrap_type(params)),
            TypeWrapper::Optional(inner) => format!("Option<{}>", inner.wrap_type(params)),
        }
    }
}

impl WrapType for TypeMetaRust {
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
pub enum MediaTypeRust {
    Image,
    Audio,
    Pdf,
    Video,
}

#[derive(Clone, PartialEq, Debug)]
pub enum TypeRust {
    // Literal types with specific values
    String(Option<String>, TypeMetaRust),
    Int(Option<i64>, TypeMetaRust),
    Float(TypeMetaRust),
    Bool(Option<bool>, TypeMetaRust),
    Null(TypeMetaRust),
    Media(MediaTypeRust, TypeMetaRust),
    // Complex types
    Class {
        package: Package,
        name: String,
        dynamic: bool,
        needs_box: bool,
        meta: TypeMetaRust,
    },
    Union {
        package: Package,
        name: String,
        meta: TypeMetaRust,
    },
    Enum {
        package: Package,
        name: String,
        dynamic: bool,
        meta: TypeMetaRust,
    },
    TypeAlias {
        name: String,
        package: Package,
        needs_box: bool,
        meta: TypeMetaRust,
    },
    List(Box<TypeRust>, TypeMetaRust),
    Map(Box<TypeRust>, Box<TypeRust>, TypeMetaRust),
    // For types we can't represent in Rust
    Any {
        reason: String,
        meta: TypeMetaRust,
    },
}

impl TypeRust {
    pub fn default_name_within_union(&self) -> String {
        match self {
            TypeRust::String(val, _) => val.as_ref().map_or("String".to_string(), |v| {
                format!("K{}", sanitize_literal_variant(v))
            }),
            TypeRust::Int(val, _) => val.map_or("Int".to_string(), |v| {
                if v < 0 {
                    format!("IntKNeg{}", v.abs())
                } else {
                    format!("IntK{}", v)
                }
            }),
            TypeRust::Float(_) => "Float".to_string(),
            TypeRust::Bool(val, _) => val.map_or("Bool".to_string(), |v| {
                format!("BoolK{}", if v { "True" } else { "False" })
            }),
            TypeRust::Media(media_type, _) => match media_type {
                MediaTypeRust::Image => "Image".to_string(),
                MediaTypeRust::Audio => "Audio".to_string(),
                MediaTypeRust::Pdf => "Pdf".to_string(),
                MediaTypeRust::Video => "Video".to_string(),
            },
            TypeRust::TypeAlias { name, .. } => name.clone(),
            TypeRust::Class { name, .. } => name.clone(),
            TypeRust::Union { name, .. } => name.clone(),
            TypeRust::Enum { name, .. } => name.clone(),
            TypeRust::List(inner, _) => format!("List{}", inner.default_name_within_union()),
            TypeRust::Map(key, value, _) => format!(
                "Map{}Key{}Value",
                key.default_name_within_union(),
                value.default_name_within_union()
            ),
            TypeRust::Null(_) => "Null".to_string(),
            TypeRust::Any { .. } => "Any".to_string(),
        }
    }

    pub fn meta(&self) -> &TypeMetaRust {
        match self {
            TypeRust::String(.., meta) => meta,
            TypeRust::Int(.., meta) => meta,
            TypeRust::Float(meta) => meta,
            TypeRust::Bool(.., meta) => meta,
            TypeRust::Media(_, meta) => meta,
            TypeRust::Class { meta, .. } => meta,
            TypeRust::TypeAlias { meta, .. } => meta,
            TypeRust::Union { meta, .. } => meta,
            TypeRust::Enum { meta, .. } => meta,
            TypeRust::List(_, meta) => meta,
            TypeRust::Map(_, _, meta) => meta,
            TypeRust::Null(meta) => meta,
            TypeRust::Any { meta, .. } => meta,
        }
    }

    pub fn is_string_primitive(&self) -> bool {
        matches!(self, TypeRust::String(_, _))
    }

    pub fn meta_mut(&mut self) -> &mut TypeMetaRust {
        match self {
            TypeRust::String(.., meta) => meta,
            TypeRust::Int(.., meta) => meta,
            TypeRust::Float(meta) => meta,
            TypeRust::Bool(.., meta) => meta,
            TypeRust::Media(_, meta) => meta,
            TypeRust::Class { meta, .. } => meta,
            TypeRust::TypeAlias { meta, .. } => meta,
            TypeRust::Union { meta, .. } => meta,
            TypeRust::Enum { meta, .. } => meta,
            TypeRust::List(_, meta) => meta,
            TypeRust::Map(_, _, meta) => meta,
            TypeRust::Null(meta) => meta,
            TypeRust::Any { meta, .. } => meta,
        }
    }

    pub fn is_class_named(&self, target: &str) -> bool {
        matches!(
            self,
            TypeRust::Class { name, .. } | TypeRust::TypeAlias { name, .. } if name == target
        )
    }

    pub fn make_boxed(&mut self) {
        match self {
            TypeRust::Class { needs_box, .. } => *needs_box = true,
            TypeRust::TypeAlias { needs_box, .. } => *needs_box = true,
            _ => {}
        }
    }

    pub fn with_meta(mut self, meta: TypeMetaRust) -> Self {
        *(self.meta_mut()) = meta;
        self
    }

    pub fn serialize_type_with_turbofish(&self, pkg: &CurrentRenderPackage) -> String {
        let type_str = self.serialize_type(pkg);
        if type_str.contains("::<") {
            return type_str;
        }
        if let Some(idx) = type_str.find('<') {
            let (prefix, rest) = type_str.split_at(idx);
            if prefix.ends_with("::") {
                type_str
            } else {
                format!("{}::{}", prefix, rest)
            }
        } else {
            type_str
        }
    }

    pub fn default_value(&self, pkg: &CurrentRenderPackage) -> String {
        if matches!(self.meta().type_wrapper, TypeWrapper::Optional(_)) {
            return "None".to_string();
        }
        if matches!(self.meta().type_wrapper, TypeWrapper::Checked(_, _)) {
            return default_call(self.serialize_type(pkg));
        }
        match self {
            TypeRust::String(val, _) => val.as_ref().map_or("String::new()".to_string(), |v| {
                format!("String::from(\"{}\")", v.replace("\"", "\\\""))
            }),
            TypeRust::Int(val, _) => val.map_or("0".to_string(), |v| format!("{v}")),
            TypeRust::Float(_) => "0.0".to_string(),
            TypeRust::Bool(val, _) => val.map_or("false".to_string(), |v| {
                if v { "true" } else { "false" }.to_string()
            }),
            TypeRust::Media(..) | TypeRust::Class { .. } | TypeRust::Union { .. } => {
                default_call(self.serialize_type(pkg))
            }
            TypeRust::Enum { .. } => default_call(self.serialize_type(pkg)),
            TypeRust::TypeAlias { .. } => default_call(self.serialize_type(pkg)),
            TypeRust::List(..) => "Vec::new()".to_string(),
            TypeRust::Map(..) => "std::collections::HashMap::new()".to_string(),
            TypeRust::Null(_) => format!("{}NullValue", Package::types().relative_from(pkg)),
            TypeRust::Any { .. } => "serde_json::Value::Null".to_string(),
        }
    }
}

fn default_call(type_str: String) -> String {
    if let Some(inner) = type_str
        .strip_prefix("Box<")
        .and_then(|s| s.strip_suffix('>'))
    {
        format!("Box::<{}>::default()", inner)
    } else {
        format!("{}::default()", type_str)
    }
}

fn sanitize_literal_variant(value: &str) -> String {
    let filtered: String = value
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let pascal = crate::utils::to_pascal_case(&filtered);
    if pascal.is_empty() {
        "Value".to_string()
    } else {
        pascal
    }
}

pub trait SerializeType {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String;
}

impl SerializeType for TypeRust {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String {
        let meta = self.meta();
        let type_str = match self {
            TypeRust::String(..) => "String".to_string(),
            TypeRust::Int(..) => "i64".to_string(),
            TypeRust::Float(_) => "f64".to_string(),
            TypeRust::Bool(..) => "bool".to_string(),
            TypeRust::Media(media, _) => media.serialize_type(pkg),
            TypeRust::Class {
                package,
                name,
                needs_box,
                ..
            } => {
                let path = format!("{}{}", package.relative_from(pkg), name);
                if *needs_box {
                    format!("Box<{}>", path)
                } else {
                    path
                }
            }
            TypeRust::TypeAlias {
                package,
                name,
                needs_box,
                ..
            } => {
                let path = format!("{}{}", package.relative_from(pkg), name);
                if *needs_box {
                    format!("Box<{}>", path)
                } else {
                    path
                }
            }
            TypeRust::Union { package, name, .. } => {
                format!("{}{}", package.relative_from(pkg), name)
            }
            TypeRust::Enum { package, name, .. } => {
                format!("{}{}", package.relative_from(pkg), name)
            }
            TypeRust::List(inner, _) => format!("Vec<{}>", inner.serialize_type(pkg)),
            TypeRust::Map(key, value, _) => {
                format!(
                    "std::collections::HashMap<{}, {}>",
                    key.serialize_type(pkg),
                    value.serialize_type(pkg)
                )
            }
            TypeRust::Null(_) => format!("{}NullValue", Package::types().relative_from(pkg)),
            TypeRust::Any { .. } => "serde_json::Value".to_string(),
        };

        meta.wrap_type((pkg, type_str))
    }
}

impl SerializeType for MediaTypeRust {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String {
        match self {
            MediaTypeRust::Image => format!("{}BamlImage", Package::types().relative_from(pkg)),
            MediaTypeRust::Audio => format!("{}BamlAudio", Package::types().relative_from(pkg)),
            MediaTypeRust::Pdf => format!("{}BamlPdf", Package::types().relative_from(pkg)),
            MediaTypeRust::Video => format!("{}BamlVideo", Package::types().relative_from(pkg)),
        }
    }
}

// Legacy functions for backward compatibility
pub fn to_rust_type(ty: &TypeNonStreaming) -> String {
    // This should be replaced by the new type system, but keeping for compatibility
    match ty {
        TypeNonStreaming::Primitive(prim, _) => match prim {
            TypeValue::String => "String".to_string(),
            TypeValue::Int => "i64".to_string(),
            TypeValue::Float => "f64".to_string(),
            TypeValue::Bool => "bool".to_string(),
            TypeValue::Null => "()".to_string(),
            TypeValue::Media(media_type) => match media_type {
                baml_types::BamlMediaType::Image => "BamlImage".to_string(),
                baml_types::BamlMediaType::Audio => "BamlAudio".to_string(),
                baml_types::BamlMediaType::Pdf => "BamlPdf".to_string(),
                baml_types::BamlMediaType::Video => "BamlVideo".to_string(),
            },
        },
        TypeNonStreaming::Class { name, .. } => name.clone(),
        TypeNonStreaming::Enum { name, .. } => name.clone(),
        TypeNonStreaming::List(inner, _) => format!("Vec<{}>", to_rust_type(inner)),
        TypeNonStreaming::Map(_, value, _) => {
            format!("std::collections::HashMap<String, {}>", to_rust_type(value))
        }
        TypeNonStreaming::Union(_inner, _) => {
            // TODO: This should use the new union type generation
            "serde_json::Value".to_string()
        }
        TypeNonStreaming::Literal(lit, _) => match lit {
            baml_types::LiteralValue::String(_) => "String".to_string(),
            baml_types::LiteralValue::Int(_) => "i64".to_string(),
            baml_types::LiteralValue::Bool(_) => "bool".to_string(),
        },
        TypeNonStreaming::Tuple(_, _) => "serde_json::Value".to_string(), // Fallback for tuples
        TypeNonStreaming::RecursiveTypeAlias { .. } => "serde_json::Value".to_string(), // Fallback
        TypeNonStreaming::Arrow(_, _) => "serde_json::Value".to_string(), // Fallback for function types
        // TODO(Cecilia): actually deal with this
        TypeNonStreaming::Top(_) => "serde_json::Value".to_string(), // Fallback for top type
    }
}

pub fn is_optional(ty: &TypeNonStreaming) -> bool {
    // Check if this is a union with null
    match ty {
        TypeNonStreaming::Union(_inner, _) => {
            // TODO: Check if union contains null - need to implement proper union analysis
            false
        }
        _ => false,
    }
}
