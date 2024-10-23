use std::collections::HashMap;

use internal_baml_schema_ast::ast::FieldId;

use super::Attributes;

#[derive(Debug, Default)]
pub struct EnumAttributes {
    pub value_serializers: HashMap<FieldId, Attributes>,

    pub serializer: Option<Attributes>,
}

#[derive(Debug, Default)]
pub struct ClassAttributes {
    pub field_serializers: HashMap<FieldId, Attributes>,

    pub serializer: Option<Attributes>,
}
