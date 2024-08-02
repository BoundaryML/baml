use std::collections::HashSet;

use crate::{
    printer::{serialize_with_printer, WithSerializeableContent},
    types::ToStringAttributes,
    ParserDatabase, WithSerialize,
};
use either::Either;
use internal_baml_diagnostics::DatamodelError;
use internal_baml_schema_ast::ast::SubType;
use internal_baml_schema_ast::ast::{self, WithIdentifier, WithName, WithSpan};
use serde_json::json;
use std::collections::HashMap;

use super::{field::FieldWalker, EnumWalker};

/// A `class` declaration in the Prisma schema.
pub type ClassWalker<'db> = super::Walker<'db, ast::TypeExpId>;

impl<'db> ClassWalker<'db> {
    /// The ID of the class in the db
    pub fn class_id(self) -> ast::TypeExpId {
        self.id
    }

    /// The AST node.
    pub fn ast_class(self) -> &'db ast::TypeExpressionBlock {
        &self.db.ast[self.id]
    }

    /// Iterate all the scalar fields in a given class in the order they were defined.
    pub fn static_fields(self) -> impl ExactSizeIterator<Item = FieldWalker<'db>> {
        self.ast_class()
            .iter_fields()
            .filter_map(move |(field_id, _)| {
                self.db
                    .types
                    .refine_class_field((self.id, field_id))
                    .left()
                    .map(|_id| self.walk((self.id, field_id, false)))
            })
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// Iterate all the scalar fields in a given class in the order they were defined.
    pub fn dynamic_fields(self) -> impl ExactSizeIterator<Item = FieldWalker<'db>> {
        self.ast_class()
            .iter_fields()
            .filter_map(move |(field_id, _)| {
                self.db
                    .types
                    .refine_class_field((self.id, field_id))
                    .right()
                    .map(|_id| self.walk((self.id, field_id, true)))
            })
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// Iterate all the scalar fields in a given class in the order they were defined.
    pub fn dependencies(self) -> &'db HashSet<String> {
        &self.db.types.class_dependencies[&self.id]
    }

    /// Find all enums used by this class and any of its fields.
    pub fn required_enums(self) -> impl Iterator<Item = EnumWalker<'db>> {
        self.db.types.class_dependencies[&self.class_id()]
            .iter()
            .filter_map(|f| match self.db.find_type_by_str(f) {
                Some(Either::Left(_cls)) => None,
                Some(Either::Right(walker)) => Some(walker),
                None => None,
            })
    }

    /// Find all classes used by this class and any of its fields.
    pub fn required_classes(self) -> impl Iterator<Item = ClassWalker<'db>> {
        self.db.types.class_dependencies[&self.class_id()]
            .iter()
            .filter_map(|f| match self.db.find_type_by_str(f) {
                Some(Either::Left(walker)) => Some(walker),
                Some(Either::Right(_enm)) => None,
                None => None,
            })
    }

    /// The name of the template string.
    pub fn add_to_types(self, types: &mut internal_baml_jinja::PredefinedTypes) {
        types.add_class(
            self.name(),
            self.static_fields()
                .filter_map(|f| {
                    f.r#type()
                        .as_ref()
                        .map(|field_type| (f.name().to_string(), self.db.to_jinja_type(field_type)))
                })
                .collect::<HashMap<_, _>>(),
        )
    }
    /// get gejoge
    pub fn get_default_attributes(&self, sub_type: SubType) -> Option<&'db ToStringAttributes> {
        match sub_type {
            SubType::Enum => self
                .db
                .types
                .enum_attributes
                .get(&self.id)
                .and_then(|f| f.serilizer.as_ref()),
            _ => self
                .db
                .types
                .class_attributes
                .get(&self.id)
                .and_then(|f| f.serilizer.as_ref()),
        }
    }
}

impl<'db> WithIdentifier for ClassWalker<'db> {
    fn identifier(&self) -> &ast::Identifier {
        self.ast_class().identifier()
    }
}

impl<'db> WithSpan for ClassWalker<'db> {
    fn span(&self) -> &internal_baml_diagnostics::Span {
        self.ast_class().span()
    }
}

impl<'db> WithSerializeableContent for ClassWalker<'db> {
    fn serialize_data(&self, db: &'_ ParserDatabase) -> serde_json::Value {
        json!({
            "rtype": "class",
            "optional": false,
            "name": self.name(),
            "fields": self.static_fields().map(|f| f.serialize_data(db)).collect::<Vec<_>>(),
            // Dynamic fields are not serialized.
        })
    }
}

impl<'db> WithSerialize for ClassWalker<'db> {
    fn serialize(
        &self,
        db: &'_ ParserDatabase,
        span: &internal_baml_diagnostics::Span,
    ) -> Result<String, internal_baml_diagnostics::DatamodelError> {
        // Eventually we should validate what parameters are in meta.
        match serialize_with_printer(false, self.serialize_data(db)) {
            Ok(val) => Ok(val),
            Err(e) => Err(DatamodelError::new_validation_error(
                &format!("Error serializing class: {}\n{}", self.name(), e),
                span.clone(),
            )),
        }
    }

    fn output_format(
        &self,
        db: &'_ ParserDatabase,
        span: &internal_baml_diagnostics::Span,
    ) -> Result<String, internal_baml_diagnostics::DatamodelError> {
        let class_schema = self.serialize(db, span)?;

        let mut enum_schemas = self
            .required_enums()
            // TODO(sam) - if enum serialization fails, then we do not surface the error to the user.
            // That is bad!!!!!!!
            .filter_map(|e| match e.serialize(db, e.identifier().span()) {
                Ok(enum_schema) => Some((e.name().to_string(), enum_schema)),
                Err(_) => None,
            })
            .collect::<Vec<_>>();
        // Enforce a stable order on enum schemas. Without this, the order is actually unstable, and the order can ping-pong
        // when the vscode ext re-renders the live preview
        enum_schemas.sort_by_key(|(name, _)| name.to_string());
        let enum_schemas = enum_schemas
            .into_iter()
            .map(|(_, enum_schema)| enum_schema)
            .collect::<Vec<_>>();

        let enum_schemas = match enum_schemas.len() {
            0 => "".to_string(),
            1 => format!(
                "\n\nUse this enum for the output:\n{}",
                enum_schemas.join("")
            ),
            _ => format!(
                "\n\nUse these enums for the output:\n{}",
                enum_schemas
                    .into_iter()
                    .map(|enum_schema| format!("{enum_schema}\n---"))
                    .collect::<Vec<_>>()
                    .join("\n\n")
            ),
        };

        Ok(format!(
            "Use this output format:\n{}{}",
            class_schema, enum_schemas
        ))
    }
}
