use std::collections::HashMap;

use internal_baml_schema_ast::ast::FieldId;

use super::Attributes;

#[derive(Debug, Default, Clone)]
pub struct EnumAttributes {
    pub value_serilizers: HashMap<FieldId, Attributes>,

    pub serilizer: Option<Attributes>,
}

#[derive(Debug, Default, Clone)]
pub struct ClassAttributes {
    pub field_serilizers: HashMap<FieldId, Attributes>,

    pub serilizer: Option<Attributes>,
}
