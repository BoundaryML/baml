use std::ops::Deref;

use crate::{
    printer::{WithSerializeableContent, WithStaticRenames},
    types::{DynamicStringAttributes, StaticStringAttributes, ToStringAttributes},
    ParserDatabase,
};

use super::{ClassWalker, VariantWalker, Walker};

use internal_baml_schema_ast::ast::{self, FieldType, Identifier, WithName};
use serde_json::json;

/// A model field, scalar or relation.
pub type FieldWalker<'db> = Walker<'db, (ast::ClassId, ast::FieldId, bool)>;

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

    /// Traverse the field's attributes.
    pub fn attributes(self) -> &'db ToStringAttributes {
        &self.db.types.class_attributes[&self.id.0].field_serilizers[&self.id.1]
    }

    /// Whether the field is dynamic.
    pub fn is_dynamic(self) -> bool {
        self.id.2
    }

    /// Attributes for the field.
    pub fn static_attributes(self) -> &'db StaticStringAttributes {
        match self.attributes() {
            ToStringAttributes::Static(d) => d,
            _ => panic!("Expected static attributes"),
        }
    }

    /// Attributes for the field.
    pub fn dynamic_attributes(self) -> &'db DynamicStringAttributes {
        match self.attributes() {
            ToStringAttributes::Dynamic(d) => d,
            _ => panic!("Expected dynamic attributes"),
        }
    }

    /// The field's alias.
    pub fn code_for_language(self, language: &str) -> Option<&'db str> {
        match self.db.interner.lookup(language) {
            Some(language) => self
                .dynamic_attributes()
                .code
                .get(&language)
                .and_then(|&s| self.db.interner.get(s)),
            None => None,
        }
    }
}

impl<'db> WithName for FieldWalker<'db> {
    /// The field name.
    fn name(&self) -> &'db str {
        self.ast_field().name()
    }
}

impl<'db> WithSerializeableContent for (&ParserDatabase, &FieldType) {
    fn serialize_data(
        &self,
        variant: Option<&VariantWalker<'_>>,
        db: &'_ ParserDatabase,
    ) -> serde_json::Value {
        match self.1 {
            FieldType::Tuple(..) | FieldType::Dictionary(..) => json!({
                "rtype": "unsupported",
                "optional": false,
            }),
            FieldType::Union(arity, fts, _) => json!({
                "rtype": "union",
                "optional": arity.is_optional(),
                "options": fts.iter().map(|ft| (self.0, ft).serialize_data(variant, db)).collect::<Vec<_>>(),
            }),
            FieldType::List(ft, dims, _) => json!({
                "rtype": "list",
                "dims": dims,
                "inner": (self.0, ft.deref()).serialize_data(variant, db),
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
                    Some(either::Either::Left(cls)) => {
                        let mut class_type = cls.serialize_data(variant, db);
                        let Some(obj) = class_type.as_object_mut() else {
                            return class_type;
                        };
                        obj.insert("optional".to_string(), arity.is_optional().into());
                        class_type
                    }
                    Some(either::Either::Right(enm)) => {
                        json!({
                            "rtype": "enum",
                            "optional": arity.is_optional(),
                            "name": enm.alias(variant, db),
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
    fn serialize_data(
        &self,
        variant: Option<&VariantWalker<'_>>,
        db: &'_ ParserDatabase,
    ) -> serde_json::Value {
        json!({
            "name": self.alias(variant, db),
            "meta": self.meta(variant, db),
            "type_meta": (self.db, self.r#type()).serialize_data(variant, db),
        })
    }
}

impl<'db> WithStaticRenames<'db> for FieldWalker<'db> {
    fn get_override(&self, variant: &VariantWalker<'db>) -> Option<&'db ToStringAttributes> {
        variant.find_serializer_field_attributes(self.model().name(), self.name())
    }

    fn get_default_attributes(&self) -> Option<&'db ToStringAttributes> {
        self.db
            .types
            .class_attributes
            .get(&self.id.0)
            .and_then(|f| f.field_serilizers.get(&self.id.1))
    }
}
