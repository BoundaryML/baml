use internal_baml_diagnostics::DatamodelError;
use internal_baml_prompt_parser::ast::{PrinterBlock, WithSpan};
use internal_baml_schema_ast::ast::{WithDocumentation, WithIdentifier, WithName};
use serde_json::json;

use crate::{
    ast,
    printer::{serialize_with_printer, WithSerialize, WithSerializeableContent, WithStaticRenames},
    types::ToStringAttributes,
    walkers::Walker,
    ParserDatabase,
};

use super::VariantWalker;

/// An `enum` declaration in the schema.
pub type EnumWalker<'db> = Walker<'db, ast::EnumId>;
/// One value in an `enum` declaration in the schema.
pub type EnumValueWalker<'db> = Walker<'db, (ast::EnumId, ast::EnumValueId)>;

impl<'db> EnumWalker<'db> {
    /// The name of the enum.

    /// The AST node.
    pub fn ast_enum(self) -> &'db ast::Enum {
        &self.db.ast()[self.id]
    }

    /// The values of the enum.
    pub fn values(self) -> impl ExactSizeIterator<Item = EnumValueWalker<'db>> {
        self.ast_enum()
            .iter_values()
            .filter_map(move |(valid_id, _)| {
                self.db
                    .types
                    .refine_enum_value((self.id, valid_id))
                    .left()
                    .map(|_id| self.walk((self.id, valid_id)))
            })
            .collect::<Vec<_>>()
            .into_iter()
    }
}

impl<'db> WithIdentifier for EnumWalker<'db> {
    fn identifier(&self) -> &ast::Identifier {
        self.ast_enum().identifier()
    }
}

impl<'db> WithSerializeableContent for EnumWalker<'db> {
    fn serialize_data(
        &self,
        variant: Option<&VariantWalker<'_>>,
        db: &'_ ParserDatabase,
    ) -> serde_json::Value {
        json!({
            "name": self.alias(variant, db),
            "meta": self.meta(variant, db),
            "values": self.values().filter(|f| !f.skip(variant)).map(|f| f.serialize_data(variant, db)).collect::<Vec<_>>(),
        })
    }
}

impl<'db> WithStaticRenames<'db> for EnumWalker<'db> {
    fn get_override(&self, variant: &VariantWalker<'db>) -> Option<&'db ToStringAttributes> {
        variant.find_serializer_attributes(self.name())
    }

    fn get_default_attributes(&self) -> Option<&'db ToStringAttributes> {
        self.db
            .types
            .enum_attributes
            .get(&self.id)
            .and_then(|f| f.serilizer.as_ref())
    }
}

impl<'db> WithSerialize for EnumWalker<'db> {
    fn serialize(
        &self,
        db: &'_ ParserDatabase,
        variant: Option<&VariantWalker<'_>>,
        block: Option<&internal_baml_prompt_parser::ast::PrinterBlock>,
        span: &internal_baml_diagnostics::Span,
    ) -> Result<String, DatamodelError> {
        let printer_template = match block.map(|b| b.printer.as_ref()).flatten() {
            Some((p, _)) => self
                .db
                .find_printer(p)
                .map(|w| w.printer().template().to_string()),
            _ => None,
        };
        // let printer = self.db.find_printer(&block.printer);
        // Eventually we should validate what parameters are in meta.
        match serialize_with_printer(true, printer_template, self.serialize_data(variant, db)) {
            Ok(val) => Ok(val),
            Err(e) => Err(DatamodelError::new_validation_error(
                &format!("Error serializing enum: {}\n{}", self.name(), e),
                span.clone(),
            )),
        }
    }

    fn output_schema(
        &self,
        db: &'_ ParserDatabase,
        variant: Option<&VariantWalker<'_>>,
        block: Option<&internal_baml_prompt_parser::ast::PrinterBlock>,
        span: &internal_baml_diagnostics::Span,
    ) -> Result<String, internal_baml_diagnostics::DatamodelError> {
        self.serialize(db, variant, block, span)
    }
}

impl<'db> EnumValueWalker<'db> {
    fn r#enum(self) -> EnumWalker<'db> {
        self.walk(self.id.0)
    }

    /// The enum documentation
    pub fn documentation(self) -> Option<&'db str> {
        self.r#enum().ast_enum()[self.id.1].documentation()
    }
}

impl<'db> WithName for EnumValueWalker<'db> {
    fn name(&self) -> &str {
        self.r#enum().ast_enum()[self.id.1].name()
    }
}

impl<'db> WithSerializeableContent for EnumValueWalker<'db> {
    fn serialize_data(
        &self,
        variant: Option<&VariantWalker<'_>>,
        db: &'_ ParserDatabase,
    ) -> serde_json::Value {
        json!({
            "name": self.alias(variant, db),
            "meta": self.meta(variant, db),
            "skip": self.skip(variant),
        })
    }
}

impl<'db> WithStaticRenames<'db> for EnumValueWalker<'db> {
    fn get_override(&self, variant: &VariantWalker<'db>) -> Option<&'db ToStringAttributes> {
        variant.find_serializer_field_attributes(self.r#enum().name(), self.name())
    }

    fn get_default_attributes(&self) -> Option<&'db ToStringAttributes> {
        self.db
            .types
            .enum_attributes
            .get(&self.id.0)
            .and_then(|f| f.value_serilizers.get(&self.id.1))
    }
}
