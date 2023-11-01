use std::collections::{HashMap, HashSet};

use either::Either;
use internal_baml_diagnostics::DatamodelError;
use internal_baml_prompt_parser::ast::WithSpan as WithPromptSpan;
use serde_json::json;

use crate::{
    ast::{self, WithName, WithSpan},
    printer::{serialize_with_printer, WithSerializeableContent, WithStaticRenames},
    types::ToStringAttributes,
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
}

impl<'db> WithName for ClassWalker<'db> {
    fn name(&self) -> &'db str {
        self.ast_class().name()
    }
}

impl<'db> WithSpan for ClassWalker<'db> {
    fn span(&self) -> &internal_baml_diagnostics::Span {
        self.ast_class().span()
    }
}

impl<'db> WithSerializeableContent for ClassWalker<'db> {
    fn serialize_data(&self, variant: &VariantWalker<'_>) -> serde_json::Value {
        json!({
            "rtype": "class",
            "optional": false,
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
        // Eventually we should validate what parameters are in meta.
        match serialize_with_printer(
            false,
            self.db.find_printer(&block.printer),
            self.serialize_data(variant),
        ) {
            Ok(val) => Ok(val),
            Err(e) => Err(DatamodelError::new_validation_error(
                &format!("Error serializing class: {}\n{}", self.name(), e),
                block.span().clone(),
            )),
        }
    }
}
