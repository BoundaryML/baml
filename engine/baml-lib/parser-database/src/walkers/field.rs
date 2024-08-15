use std::ops::Deref;

use crate::{
    printer::WithSerializeableContent,
    types::{DynamicStringAttributes, StaticStringAttributes, ToStringAttributes},
    ParserDatabase,
};

use super::{ClassWalker, Walker};

use baml_types::{BamlMediaType, TypeValue};
use internal_baml_schema_ast::ast::{self, FieldType, Identifier, WithName, WithSpan};
use serde_json::json;

/// A model field, scalar or relation.
pub type FieldWalker<'db> = Walker<'db, (ast::TypeExpId, ast::FieldId, bool)>;

impl<'db> FieldWalker<'db> {
    /// The AST node for the field.
    pub fn ast_field(self) -> &'db ast::Field<FieldType> {
        &self.db.ast[self.id.0][self.id.1]
    }

    /// The field type.
    pub fn r#type(self) -> &'db Option<FieldType> {
        &self.ast_field().expr
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

    /// The field's default attributes.
    pub fn get_default_attributes(&self) -> Option<&'db ToStringAttributes> {
        let result = self
            .db
            .types
            .class_attributes
            .get(&self.id.0)
            .and_then(|f| f.field_serilizers.get(&self.id.1));

        result
    }
}

impl<'db> WithName for FieldWalker<'db> {
    /// The field name.
    fn name(&self) -> &'db str {
        self.ast_field().name()
    }
}

impl<'db> WithSpan for FieldWalker<'db> {
    fn span(&self) -> &internal_baml_diagnostics::Span {
        self.ast_field().span()
    }
}

impl<'db> WithSerializeableContent for (&ParserDatabase, &FieldType) {
    fn serialize_data(&self, db: &'_ ParserDatabase) -> serde_json::Value {
        match self.1 {
            FieldType::Tuple(..) | FieldType::Map(..) => json!({
                "rtype": "unsupported",
                "optional": false,
            }),
            FieldType::Union(arity, fts, ..) => json!({
                "rtype": "union",
                "optional": arity.is_optional(),
                "options": fts.iter().map(|ft| (self.0, ft).serialize_data( db)).collect::<Vec<_>>(),
            }),
            FieldType::List(ft, dims, ..) => json!({
                "rtype": "list",
                "dims": dims,
                "inner": (self.0, ft.deref()).serialize_data( db),
            }),
            FieldType::Primitive(arity, t, ..) => json!({
                "rtype": match t {
                    TypeValue::String => "string",
                    TypeValue::Int => "int",
                    TypeValue::Float => "float",
                    TypeValue::Bool => "bool",
                    TypeValue::Media(BamlMediaType::Image) => "image",
                    TypeValue::Media(BamlMediaType::Audio) => "audio",
                    TypeValue::Null => "null",

                },
                "optional": arity.is_optional(),
            }),
            FieldType::Symbol(arity, name, ..) => match self.0.find_type(name) {
                Some(either::Either::Left(cls)) => {
                    let mut class_type = cls.serialize_data(db);
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
                        "name": enm.name(),
                    })
                }
                None => json!({
                    "rtype": "unsupported",
                    "optional": false,
                }),
            },
        }
    }
}

impl<'db> WithSerializeableContent for FieldWalker<'db> {
    fn serialize_data(&self, db: &'_ ParserDatabase) -> serde_json::Value {
        let type_meta = match self.r#type() {
            Some(field_type) => (self.db, field_type).serialize_data(db),
            None => json!(null),
        };

        json!({
            "name": self.name(),
            "type_meta": type_meta,
        })
    }
}
