use baml_types::baml_value::TypeLookups;

use crate::package::{CurrentRenderPackage, Package};

#[derive(Clone, PartialEq, Debug, Default)]
pub enum TypeWrapper {
    #[default]
    None,
    Checked(Box<TypeWrapper>),
    Optional(Box<TypeWrapper>),
}

impl TypeWrapper {
    pub fn wrap_with_checked(self) -> TypeWrapper {
        TypeWrapper::Checked(Box::new(self))
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
            TypeWrapper::Checked(inner) => {
                *self = std::mem::take(inner);
                self
            }
            _ => self,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct TypeMetaGo {
    pub type_wrapper: TypeWrapper,
    pub wrap_stream_state: bool,
}

impl TypeMetaGo {
    pub fn is_optional(&self) -> bool {
        matches!(self.type_wrapper, TypeWrapper::Optional(_))
    }

    pub fn is_checked(&self) -> bool {
        matches!(self.type_wrapper, TypeWrapper::Checked(_))
    }

    pub fn make_checked(&mut self) -> &mut Self {
        self.type_wrapper = TypeWrapper::Checked(Box::new(std::mem::take(&mut self.type_wrapper)));
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
                inner.wrap_type(params)
            ),
            TypeWrapper::Optional(inner) => format!("*{}", inner.wrap_type(params)),
        }
    }
}

impl WrapType for TypeMetaGo {
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
pub enum MediaTypeGo {
    Image,
    Audio,
    Pdf,
    Video,
}

#[derive(Clone, PartialEq, Debug)]
pub enum TypeGo {
    // in case a literal
    String(Option<String>, TypeMetaGo),
    // in case a literal
    Int(Option<i64>, TypeMetaGo),
    Float(TypeMetaGo),
    Bool(Option<bool>, TypeMetaGo),
    Media(MediaTypeGo, TypeMetaGo),
    // unions become classes
    Class {
        package: Package,
        name: String,
        dynamic: bool,
        meta: TypeMetaGo,
    },
    Union {
        package: Package,
        name: String,
        meta: TypeMetaGo,
    },
    Enum {
        package: Package,
        name: String,
        dynamic: bool,
        meta: TypeMetaGo,
    },
    TypeAlias {
        name: String,
        package: Package,
        meta: TypeMetaGo,
    },
    List(Box<TypeGo>, TypeMetaGo),
    Map(Box<TypeGo>, Box<TypeGo>, TypeMetaGo),
    // For any type that we can't represent in Go, we'll use this
    Any {
        reason: String,
        meta: TypeMetaGo,
    },
}

fn safe_name(name: &str) -> String {
    // replace all non-alphanumeric characters with an underscore
    name.replace(|c: char| !c.is_alphanumeric(), "_")
}

impl TypeGo {
    // for unions, we need a default name for the type when the union is not named
    pub fn default_name_within_union(&self) -> String {
        match self {
            TypeGo::String(val, _) => val.as_ref().map_or("String".to_string(), |v| {
                let safe_name = safe_name(v);
                format!("K{safe_name}")
            }),
            TypeGo::Int(val, _) => val.map_or("Int".to_string(), |v| format!("IntK{v}")),
            TypeGo::Float(_) => "Float".to_string(),
            TypeGo::Bool(val, _) => val.map_or("Bool".to_string(), |v| {
                format!("BoolK{}", if v { "True" } else { "False" })
            }),
            TypeGo::Media(media_type_go, _) => match media_type_go {
                MediaTypeGo::Image => "Image".to_string(),
                MediaTypeGo::Audio => "Audio".to_string(),
                MediaTypeGo::Pdf => "PDF".to_string(),
                MediaTypeGo::Video => "Video".to_string(),
            },
            TypeGo::TypeAlias { name, .. } => name.clone(),
            TypeGo::Class { name, .. } => name.clone(),
            TypeGo::Union { name, .. } => name.clone(),
            TypeGo::Enum { name, .. } => name.clone(),
            TypeGo::List(type_go, _) => format!("List{}", type_go.default_name_within_union()),
            TypeGo::Map(key, value, _) => format!(
                "Map{}Key{}Value",
                key.default_name_within_union(),
                value.default_name_within_union()
            ),
            TypeGo::Any { .. } => "Any".to_string(),
        }
    }

    pub fn zero_value(&self, pkg: &CurrentRenderPackage) -> String {
        if matches!(self.meta().type_wrapper, TypeWrapper::Optional(_)) {
            return "nil".to_string();
        }
        if matches!(self.meta().type_wrapper, TypeWrapper::Checked(_)) {
            return format!("{}{{}}", self.serialize_type(pkg));
        }
        match self {
            TypeGo::String(val, _) => val.as_ref().map_or("\"\"".to_string(), |v| {
                format!("\"{}\"", v.replace("\"", "\\\"")).to_string()
            }),
            TypeGo::Int(val, _) => val.map_or("0".to_string(), |v| format!("{v}")),
            TypeGo::Float(_) => "0.0".to_string(),
            TypeGo::Bool(val, _) => val.map_or("false".to_string(), |v| {
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
        let instance = match self {
            TypeGo::String(val, _) => val.as_ref().map_or("\"\"".to_string(), |v| {
                format!("\"{}\"", v.replace("\"", "\\\"")).to_string()
            }),
            TypeGo::Int(val, _) => val.map_or("int64(0)".to_string(), |v| format!("int64({v})")),
            TypeGo::Float(_) => "float64(0.0)".to_string(),
            TypeGo::Bool(val, _) => val.map_or("false".to_string(), |v| {
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
            TypeGo::List(inner, _) => format!("[]{}{{}}", inner.serialize_type(pkg)),
            TypeGo::Map(key, value, _) => {
                format!(
                    "map[{}]{}{{}}",
                    key.serialize_type(pkg),
                    value.serialize_type(pkg)
                )
            }
            TypeGo::Any { .. } => "any".to_string(),
        };
        if matches!(self.meta().type_wrapper, TypeWrapper::Optional(_)) {
            let mut non_optional = self.clone();
            non_optional.meta_mut().type_wrapper.pop_optional();
            let base_type = non_optional.serialize_type(pkg);
            format!("(*{base_type})(nil)")
        } else {
            instance
        }
    }

    pub fn cast_from_function(&self, param: &str, pkg: &CurrentRenderPackage) -> String {
        if self.meta().is_checked() {
            let mut without_checked = self.clone();
            without_checked.meta_mut().type_wrapper.pop_checked();
            format!(
                "baml.CastChecked({param}, func(inner any) {t} {{
                return {casted}
            }})",
                t = without_checked.serialize_type(pkg),
                casted = without_checked.cast_from_function("inner", pkg)
            )
        } else if self.meta().wrap_stream_state {
            let mut without_stream_state = self.clone();
            without_stream_state.meta_mut().wrap_stream_state = false;
            format!(
                "baml.CastStreamState({param}, func(inner any) {t} {{
                return {casted}
            }})",
                t = without_stream_state.serialize_type(pkg),
                casted = without_stream_state.cast_from_function("inner", pkg)
            )
        } else {
            format!("({param}).({})", self.serialize_type(pkg))
        }
    }

    pub fn decode_from_any(&self, param: &str, pkg: &CurrentRenderPackage) -> String {
        if self.meta().wrap_stream_state {
            let mut without_stream_state = self.clone();
            without_stream_state.meta_mut().wrap_stream_state = false;
            format!(
                "baml.DecodeStreamingState({param}, func(inner *cffi.CFFIValueHolder) {t} {{
                return {casted}
            }})",
                t = without_stream_state.serialize_type(pkg),
                casted = without_stream_state.decode_from_any("inner", pkg)
            )
        } else if self.meta().is_checked() {
            let mut without_checked = self.clone();
            without_checked.meta_mut().type_wrapper.pop_checked();

            format!(
                "baml.DecodeChecked({param}, func(inner *cffi.CFFIValueHolder) {t} {{
                return {casted}
            }})",
                t = without_checked.serialize_type(pkg),
                casted = without_checked.decode_from_any("inner", pkg)
            )
        } else {
            format!(
                "baml.Decode({param}).Interface().({})",
                self.serialize_type(pkg)
            )
        }
    }

    pub fn meta(&self) -> &TypeMetaGo {
        match self {
            TypeGo::String(.., meta) => meta,
            TypeGo::Int(.., meta) => meta,
            TypeGo::Float(meta) => meta,
            TypeGo::Bool(.., meta) => meta,
            TypeGo::Media(_, meta) => meta,
            TypeGo::Class { meta, .. } => meta,
            TypeGo::TypeAlias { meta, .. } => meta,
            TypeGo::Union { meta, .. } => meta,
            TypeGo::Enum { meta, .. } => meta,
            TypeGo::List(_, meta) => meta,
            TypeGo::Map(_, _, meta) => meta,
            TypeGo::Any { meta, .. } => meta,
        }
    }

    pub fn meta_mut(&mut self) -> &mut TypeMetaGo {
        match self {
            TypeGo::String(.., meta) => meta,
            TypeGo::Int(.., meta) => meta,
            TypeGo::Float(meta) => meta,
            TypeGo::Bool(.., meta) => meta,
            TypeGo::Media(_, meta) => meta,
            TypeGo::Class { meta, .. } => meta,
            TypeGo::TypeAlias { meta, .. } => meta,
            TypeGo::Union { meta, .. } => meta,
            TypeGo::Enum { meta, .. } => meta,
            TypeGo::List(_, meta) => meta,
            TypeGo::Map(_, _, meta) => meta,
            TypeGo::Any { meta, .. } => meta,
        }
    }

    pub fn with_meta(mut self, meta: TypeMetaGo) -> Self {
        *(self.meta_mut()) = meta;
        self
    }
}

pub trait SerializeType {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String;
}

impl SerializeType for TypeGo {
    fn serialize_type(&self, pkg: &CurrentRenderPackage) -> String {
        let meta = self.meta();
        let type_str = match self {
            TypeGo::String(..) => "string".to_string(),
            TypeGo::Int(..) => "int64".to_string(),
            TypeGo::Float(_) => "float64".to_string(),
            TypeGo::Bool(..) => "bool".to_string(),
            TypeGo::Media(media, _) => media.serialize_type(pkg),
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
            TypeGo::List(inner, _) => format!("[]{}", inner.serialize_type(pkg)),
            TypeGo::Map(key, value, _) => {
                format!(
                    "map[{}]{}",
                    key.serialize_type(pkg),
                    value.serialize_type(pkg)
                )
            }
            TypeGo::Any { .. } => "any".to_string(),
        };

        meta.wrap_type((pkg, type_str))
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
