use std::collections::HashMap;

use internal_baml_schema_ast::ast::FieldId;

use super::to_string_attributes::ToStringAttributes;

#[derive(Debug, Default)]
pub struct EnumAttributes {
    pub value_serilizers: HashMap<FieldId, ToStringAttributes>,

    pub serilizer: Option<ToStringAttributes>,
}

#[derive(Debug, Default)]
pub struct ClassAttributes {
    pub field_serilizers: HashMap<FieldId, ToStringAttributes>,

    pub serilizer: Option<ToStringAttributes>,
}
