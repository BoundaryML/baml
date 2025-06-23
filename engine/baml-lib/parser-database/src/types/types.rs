use std::collections::HashMap;

use internal_baml_ast::ast::FieldId;

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

impl ClassAttributes {
    pub fn extend_serializer(&mut self, other: &Option<Attributes>) {
        let new_serializer = match (self.serilizer.as_mut(), other) {
            (Some(self_attrs), Some(other_attrs)) => Some(self_attrs.combine(other_attrs)),
            (Some(self_attrs), None) => Some(self_attrs.clone()),
            (None, Some(other_attrs)) => Some(other_attrs.clone()),
            (None, None) => None,
        };
        self.serilizer = new_serializer;
    }
}
