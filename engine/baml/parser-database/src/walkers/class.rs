use std::collections::HashMap;

use either::Either;
use internal_baml_diagnostics::DatamodelError;
use internal_baml_prompt_parser::ast::WithSpan;
use serde_json::json;

use crate::{
    ast::{self, WithName},
    template::{serialize_with_template, WithSerializeableContent, WithStaticRenames},
    types::{StaticStringAttributes, ToStringAttributes},
    WithSerialize,
};

use super::{field::FieldWalker, EnumWalker, VariantWalker};

/// A `class` declaration in the Prisma schema.
pub type ClassWalker<'db> = super::Walker<'db, ast::ClassId>;

impl<'db> ClassWalker<'db> {
    /// The ID of the class in the db
    pub fn class_id(self) -> ast::ClassId {
        self.id
    }

    /// The AST node.
    pub fn ast_class(self) -> &'db ast::Class {
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
                    .map(|_id| self.walk((self.id, field_id)))
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
                    .left()
                    .map(|_id| self.walk((self.id, field_id)))
            })
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// Find all enums used by this class and any of its fields.
    pub fn required_enums(self) -> impl Iterator<Item = EnumWalker<'db>> {
        self.static_fields()
            .flat_map(|f| f.r#type().flat_idns())
            .flat_map(|idn| match self.db.find_type(idn) {
                Some(Either::Right(walker)) => vec![walker],
                Some(Either::Left(walker)) => walker.required_enums().collect(),
                None => vec![],
            })
    }

    /// Find all classes used by this class and any of its fields.
    pub fn required_classes(self) -> impl Iterator<Item = ClassWalker<'db>> {
        self.static_fields()
            .flat_map(|f| f.r#type().flat_idns())
            .flat_map(|idn| match self.db.find_type(idn) {
                Some(Either::Left(walker)) => {
                    let mut classes = walker.required_classes().collect::<Vec<_>>();
                    classes.push(walker);
                    classes
                }
                Some(Either::Right(_)) => vec![],
                None => vec![],
            })
    }
}

impl<'db> WithName for ClassWalker<'db> {
    fn name(&self) -> &'db str {
        self.ast_class().name()
    }
}

impl<'db> WithSerializeableContent for ClassWalker<'db> {
    fn serialize_data(&self, variant: &VariantWalker<'_>) -> serde_json::Value {
        json!({
            "type": "class",
            "name": self.alias(),
            "meta": self.meta(),
            "fields": self.static_fields().map(|f| f.serialize_data(variant)).collect::<Vec<_>>(),
        })
    }
}

impl<'db> WithStaticRenames for ClassWalker<'db> {
    fn alias(&self) -> String {
        match self.alias_raw() {
            Some(id) => self.db[*id].to_string(),
            None => self.name().to_string(),
        }
    }

    fn meta(&self) -> HashMap<String, String> {
        match self.meta_raw() {
            Some(map) => map
                .iter()
                .map(|(k, v)| (self.db[*k].to_string(), self.db[*v].to_string()))
                .collect::<HashMap<_, _>>(),
            None => HashMap::new(),
        }
    }

    fn attributes(&self) -> Option<&ToStringAttributes> {
        self.db
            .types
            .class_attributes
            .get(&self.id)
            .and_then(|f| f.serilizer.as_ref())
    }
}

impl<'db> WithSerialize for ClassWalker<'db> {
    fn serialize(
        &self,
        variant: &VariantWalker<'_>,
        block: &internal_baml_prompt_parser::ast::PrinterBlock,
    ) -> Result<String, internal_baml_diagnostics::DatamodelError> {
        if let Some(template) = self.db.get_class_template(&block.printer.0) {
            // Eventually we should validate what parameters are in meta.
            match serialize_with_template("print_type", template, self.serialize_data(variant)) {
                Ok(val) => Ok(val),
                Err(e) => Err(DatamodelError::new_validation_error(
                    &format!("Error serializing class: {}\n{}", self.name(), e),
                    block.span().clone(),
                )),
            }
        } else {
            let span = match block.printer.1 {
                Some(ref span) => span,
                None => block.span(),
            };
            Err(DatamodelError::new_validation_error(
                &format!("No such serializer template: {}", block.printer.0),
                span.clone(),
            ))
        }
    }
}
