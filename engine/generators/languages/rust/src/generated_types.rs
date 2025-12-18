use crate::r#type::TypeRust;

#[derive(Debug)]
pub struct ClassRust {
    pub name: String,
    pub docstring: Option<String>,
    pub fields: Vec<FieldRust>,
    pub dynamic: bool,
}

#[derive(Debug)]
pub struct FieldRust {
    pub name: String,
    pub docstring: Option<String>,
    pub r#type: TypeRust,
}

#[derive(Debug)]
pub struct EnumRust {
    pub name: String,
    pub docstring: Option<String>,
    pub values: Vec<(String, Option<String>)>,
    pub dynamic: bool,
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
