use crate::{
    package::CurrentRenderPackage,
    r#type::{SerializeType, TypeRust},
    MediaTypeRust,
};

#[derive(Debug)]
pub struct ClassRust {
    pub name: String,
    pub docstring: Option<String>,
    pub fields: Vec<FieldRust>,
    pub dynamic: bool,
}

#[derive(Debug)]
pub struct FieldRust {
    name: String,
    raw_name: String,
    cffi_name: Option<String>,
    pub docstring: Option<String>,
    pub r#type: TypeRust,
}

impl FieldRust {
    pub fn new(name: &str, docstring: Option<String>, r#type: TypeRust) -> Self {
        let safe_name = crate::utils::escape_keyword(name);
        Self {
            cffi_name: if name == safe_name {
                None
            } else {
                Some(name.to_string())
            },
            raw_name: name.to_string(),
            name: safe_name,
            docstring,
            r#type,
        }
    }

    pub fn cffi_name(&self) -> Option<&str> {
        self.cffi_name.as_deref()
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Debug)]
pub struct EnumRust {
    pub name: String,
    pub docstring: Option<String>,
    pub values: Vec<(String, Option<String>)>,
    pub dynamic: bool,
}

impl EnumRust {
    pub fn first_value(&self) -> &str {
        self.values
            .first()
            .map(|(v, _)| v.as_str())
            .unwrap_or("Unknown")
    }
}

#[derive(Debug)]
pub struct UnionRust {
    pub name: String,
    pub cffi_name: String,
    pub docstring: Option<String>,
    pub variants: Vec<VariantRust>,
}

#[derive(Debug, Clone)]
pub struct VariantRust {
    pub name: String,
    pub cffi_name: String,
    pub literal_repr: Option<String>,
    pub type_: TypeRust,
}

#[derive(Debug)]
pub struct TypeAliasRust {
    pub name: String,
    pub type_: TypeRust,
    pub docstring: Option<String>,
}

// Rendered versions with pre-computed type strings for templates

#[derive(Debug)]
pub struct ClassRustRendered {
    pub name: String,
    pub docstring: Option<String>,
    pub fields: Vec<FieldRustRendered>,
    pub dynamic: bool,
}

impl ClassRustRendered {
    pub fn from_class(class: &ClassRust, pkg: &CurrentRenderPackage) -> Self {
        Self {
            name: class.name.clone(),
            docstring: class.docstring.clone(),
            fields: class
                .fields
                .iter()
                .map(|f| FieldRustRendered::from_field(f, pkg))
                .collect(),
            dynamic: class.dynamic,
        }
    }
}

#[derive(Debug)]
pub struct FieldRustRendered {
    pub name: String,
    pub cffi_name: Option<String>,
    pub raw_name: String,
    pub docstring: Option<String>,
    pub type_str: String,
    pub media_type: Option<MediaTypeRust>,
}

impl FieldRustRendered {
    pub fn from_field(field: &FieldRust, pkg: &CurrentRenderPackage) -> Self {
        let media_type = match field.r#type {
            TypeRust::Media(media_type) => Some(media_type),
            _ => None,
        };
        Self {
            name: field.name.clone(),
            cffi_name: field.cffi_name.clone(),
            raw_name: field.raw_name.clone(),
            docstring: field.docstring.clone(),
            type_str: field.r#type.serialize_type(pkg),
            media_type,
        }
    }
}

#[derive(Debug)]
pub struct UnionRustRendered {
    pub name: String,
    pub cffi_name: String,
    pub docstring: Option<String>,
    pub variants: Vec<VariantRustRendered>,
}

impl UnionRustRendered {
    pub fn from_union(union_rust: &UnionRust, pkg: &CurrentRenderPackage) -> Self {
        Self {
            name: union_rust.name.clone(),
            cffi_name: union_rust.cffi_name.clone(),
            docstring: union_rust.docstring.clone(),
            variants: union_rust
                .variants
                .iter()
                .map(|v| VariantRustRendered::from_variant(v, pkg))
                .collect(),
        }
    }

    pub fn first_variant_name(&self) -> &str {
        self.variants
            .first()
            .map(|v| v.name.as_str())
            .unwrap_or("Unknown")
    }

    pub fn is_all_literals(&self) -> bool {
        self.variants.iter().all(|v| v.literal_repr.is_some())
    }
}

#[derive(Debug)]
pub struct VariantRustRendered {
    pub name: String,
    pub cffi_name: String,
    pub literal_repr: Option<String>,
    pub type_str: String,
    pub media_type: Option<MediaTypeRust>,
}

impl VariantRustRendered {
    pub fn from_variant(variant: &VariantRust, pkg: &CurrentRenderPackage) -> Self {
        let media_type = match variant.type_ {
            TypeRust::Media(media_type) => Some(media_type),
            _ => None,
        };
        Self {
            name: variant.name.clone(),
            cffi_name: variant.cffi_name.clone(),
            literal_repr: variant.literal_repr.clone(),
            type_str: variant.type_.serialize_type(pkg),
            media_type,
        }
    }
}

#[derive(Debug)]
pub struct TypeAliasRustRendered {
    pub name: String,
    pub type_str: String,
    pub docstring: Option<String>,
}

impl TypeAliasRustRendered {
    pub fn from_alias(alias: &TypeAliasRust, pkg: &CurrentRenderPackage) -> Self {
        Self {
            name: alias.name.clone(),
            type_str: alias.type_.serialize_type(pkg),
            docstring: alias.docstring.clone(),
        }
    }
}
