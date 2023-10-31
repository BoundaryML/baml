use std::collections::HashMap;

use internal_baml_diagnostics::{DatamodelError, Span};
use internal_baml_prompt_parser::ast::{PrinterBlock, WithSpan};
use internal_baml_schema_ast::ast::{WithDocumentation, WithName};
use serde_json::json;

use crate::{
    ast,
    template::{
        serialize_with_template, WithSerialize, WithSerializeableContent, WithStaticRenames,
    },
    types::{StaticStringAttributes, ToStringAttributes},
    walkers::Walker,
};

/// An `enum` declaration in the schema.
pub type EnumWalker<'db> = Walker<'db, ast::EnumId>;
/// One value in an `enum` declaration in the schema.
pub type EnumValueWalker<'db> = Walker<'db, (ast::EnumId, ast::EnumValueId)>;

impl<'db> EnumWalker<'db> {
    /// The name of the enum.
    pub fn name(self) -> &'db str {
        &self.ast_enum().name()
    }

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

impl<'db> WithName for EnumWalker<'db> {
    fn name(&self) -> &str {
        self.ast_enum().name()
    }
}

impl<'db> WithSerializeableContent for EnumWalker<'db> {
    fn serialize_data(&self) -> serde_json::Value {
        json!({
            "name": self.alias(),
            "meta": self.meta(),
            "values": self.values().map(|f| f.serialize_data()).collect::<Vec<_>>(),
        })
    }
}

impl<'db> WithStaticRenames for EnumWalker<'db> {
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
            .enum_attributes
            .get(&self.id)
            .and_then(|f| f.serilizer.as_ref())
    }
}

impl<'db> WithSerialize for EnumWalker<'db> {
    fn serialize(&self, block: &PrinterBlock) -> Result<String, DatamodelError> {
        if let Some(template) = self.db.get_enum_template(&block.printer.0) {
            // Eventually we should validate what parameters are in meta.
            match serialize_with_template("print_enum", template, self.serialize_data()) {
                Ok(val) => Ok(val),
                Err(e) => Err(DatamodelError::new_validation_error(
                    &format!("Error serializing enum: {}\n{}", self.name(), e),
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
        &self.r#enum().ast_enum()[self.id.1].name()
    }
}

impl<'db> WithSerializeableContent for EnumValueWalker<'db> {
    fn serialize_data(&self) -> serde_json::Value {
        json!({
            "name": self.alias(),
            "meta": self.meta(),
        })
    }
}

impl<'db> WithStaticRenames for EnumValueWalker<'db> {
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
            .enum_attributes
            .get(&self.id.0)
            .and_then(|f| f.value_serilizers.get(&self.id.1))
    }
}
