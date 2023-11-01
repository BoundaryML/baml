use std::ops::Deref;

use crate::{
    printer::{WithSerializeableContent, WithStaticRenames},
    types::ToStringAttributes,
    ParserDatabase,
};

use super::{ClassWalker, VariantWalker, Walker};
use internal_baml_schema_ast::ast::{self, FieldType, Identifier, WithName};
use serde_json::json;

/// A model field, scalar or relation.
pub type FieldWalker<'db> = Walker<'db, (ast::ClassId, ast::FieldId)>;

impl<'db> FieldWalker<'db> {
    /// The AST node for the field.
    pub fn ast_field(self) -> &'db ast::Field {
        &self.db.ast[self.id.0][self.id.1]
    }

    /// The field type.
    pub fn r#type(self) -> &'db FieldType {
        &self.ast_field().field_type
    }

    /// Traverse the field's parent model.
    pub fn model(self) -> ClassWalker<'db> {
        self.walk(self.id.0)
    }
}

impl<'db> WithName for FieldWalker<'db> {
    /// The field name.
    fn name(&self) -> &'db str {
        self.ast_field().name()
    }
}

impl<'db> WithSerializeableContent for (&ParserDatabase, &FieldType) {
    fn serialize_data(&self, variant: &VariantWalker<'_>) -> serde_json::Value {
        match self.1 {
            FieldType::Tuple(..) | FieldType::Dictionary(..) => json!({
                "rtype": "unsupported",
                "optional": false,
            }),
            FieldType::Union(airty, fts, _) => json!({
                "rtype": "union",
                "optional": airty.is_optional(),
                "options": fts.iter().map(|ft| (self.0, ft).serialize_data(variant)).collect::<Vec<_>>(),
            }),
            FieldType::List(ft, dims, _) => json!({
                "rtype": "list",
                "dims": dims,
                "inner": (self.0, ft.deref()).serialize_data(variant),
            }),
            FieldType::Identifier(arity, Identifier::Primitive(name, ..)) => {
                json!({
                    "rtype": "primitive",
                    "optional": arity.is_optional(),
                    "value": match name {
                        ast::TypeValue::Bool => "bool",
                        ast::TypeValue::Int => "int",
                        ast::TypeValue::Float => "float",
                        ast::TypeValue::Char => "char",
                        ast::TypeValue::String => "string",
                        ast::TypeValue::Null => "null",
                    }
                })
            }
            FieldType::Identifier(arity, Identifier::Local(name, ..)) => {
                match self.0.find_type_by_str(name) {
                    Some(either::Either::Left(cls)) => cls.serialize_data(variant),
                    Some(either::Either::Right(enm)) => {
                        json!({
                            "rtype": "enum",
                            "optional": arity.is_optional(),
                            "name": enm.alias(),
                        })
                    }
                    None => json!({
                        "rtype": "unsupported",
                        "optional": false,
                    }),
                }
            }
            FieldType::Identifier(..) => serde_json::Value::Null,
        }
    }
}

impl<'db> WithSerializeableContent for FieldWalker<'db> {
    fn serialize_data(&self, variant: &VariantWalker<'_>) -> serde_json::Value {
        json!({
            "name": self.alias(),
            "meta": self.meta(),
            "type_meta": (self.db, self.r#type()).serialize_data(variant),
        })
    }
}

impl<'db> WithStaticRenames for FieldWalker<'db> {
    fn alias(&self) -> String {
        match self.alias_raw() {
            Some(id) => self.db[*id].to_string(),
            None => self.name().to_string(),
        }
    }

    fn meta(&self) -> std::collections::HashMap<String, String> {
        match self.meta_raw() {
            Some(map) => map
                .iter()
                .map(|(k, v)| (self.db[*k].to_string(), self.db[*v].to_string()))
                .collect(),
            None => std::collections::HashMap::new(),
        }
    }

    /// The parsed attributes.
    fn attributes(&self) -> Option<&ToStringAttributes> {
        self.db
            .types
            .class_attributes
            .get(&self.id.0)
            .and_then(|f| f.field_serilizers.get(&self.id.1))
    }
}
