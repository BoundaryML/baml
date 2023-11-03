use std::borrow::BorrowMut;

use internal_baml_prompt_parser::ast::PrinterBlock;
use internal_baml_schema_ast::ast::{Identifier, WithName};

use crate::{
    ast::{self, WithIdentifier},
    types::{SerializerAttributes, ToStringAttributes, VariantProperties},
};

use super::{ClientWalker, FunctionWalker, Walker};

/// A `function` declaration in the Prisma schema.
pub type VariantWalker<'db> = Walker<'db, ast::VariantConfigId>;

impl<'db> VariantWalker<'db> {
    /// The name of the function.
    pub fn identifier(self) -> &'db Identifier {
        self.ast_variant().identifier()
    }

    /// The name of the function.
    pub fn function_identifier(self) -> &'db Identifier {
        self.ast_variant().function_name()
    }

    /// The ID of the function in the db
    pub fn variant_id(self) -> ast::VariantConfigId {
        self.id
    }

    /// Helper to access every client.
    pub fn client(self) -> Option<ClientWalker<'db>> {
        self.db
            .types
            .variant_properties
            .get(&self.id)
            .and_then(|f| self.db.find_client(&f.client.value))
    }

    /// The function node.
    pub fn walk_function(self) -> Option<FunctionWalker<'db>> {
        self.db.find_function(self.function_identifier())
    }

    /// The AST node.
    pub fn ast_variant(self) -> &'db ast::Variant {
        &self.db.ast[self.id]
    }

    /// Finds a serializer by name
    pub fn find_serializer_attributes(self, name: &str) -> Option<&'db ToStringAttributes> {
        self.ast_variant()
            .iter_serializers()
            .find(|(_, s)| s.name() == name)
            .and_then(|(idx, s)| {
                self.db.types.variant_attributes[&self.id]
                    .serializers
                    .get(&idx)
                    .and_then(|f| f.serilizer.as_ref())
            })
    }

    /// Finds a serializer for a field by name
    pub fn find_serializer_field_attributes(
        self,
        name: &str,
        field_name: &str,
    ) -> Option<&'db ToStringAttributes> {
        self.ast_variant()
            .iter_serializers()
            .find(|(_, s)| s.name() == name)
            .and_then(|(idx, s)| {
                let fid = s.field_id_for(field_name);

                if let Some(fid) = fid {
                    self.db.types.variant_attributes[&self.id]
                        .serializers
                        .get(&idx)
                        .and_then(|s| s.field_serilizers.get(&fid))
                } else {
                    None
                }
            })
    }

    /// The properties of the variant.
    pub fn properties(self) -> &'db VariantProperties {
        &self.db.types.variant_properties[&self.id]
    }
}
