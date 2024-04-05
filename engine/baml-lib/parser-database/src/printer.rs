use std::collections::HashMap;

use internal_baml_diagnostics::DatamodelError;
use internal_baml_prompt_parser::ast::PrinterBlock;
use internal_baml_schema_ast::ast::WithName;

mod print_enum_default;
mod print_type_default;

use crate::{
    interner::StringId,
    types::{StaticStringAttributes, ToStringAttributes},
    walkers::VariantWalker,
    ParserDatabase,
};

/// Trait
pub trait WithSerializeableContent {
    /// Trait to render an object.
    fn serialize_data(&self, variant: &VariantWalker<'_>) -> serde_json::Value;
}

/// Trait
pub trait WithStaticRenames<'db>: WithName {
    /// Overrides for local names.
    fn get_override(&self, variant: &VariantWalker<'db>) -> Option<&'_ ToStringAttributes>;
    /// Overrides for local names.
    fn get_default_attributes(&self) -> Option<&'db ToStringAttributes>;

    /// Overrides for local names.
    fn alias(&'db self, variant: &VariantWalker<'db>) -> String {
        let (overrides, defaults) = self.get_attributes(variant);

        let override_alias = overrides.and_then(|o| *o.alias());
        let base_alias = defaults.and_then(|a| *a.alias());
        match (override_alias, base_alias) {
            (Some(id), _) => variant.db[id].to_string(),
            (None, Some(id)) => variant.db[id].to_string(),
            (None, None) => self.name().to_string(),
        }
    }

    /// Overrides for local names.
    fn maybe_alias(&'db self, db: &'db ParserDatabase) -> Option<String> {
        let defaults = match self.get_default_attributes() {
            Some(ToStringAttributes::Static(refs)) => Some(refs),
            _ => None,
        };
        let base_alias = defaults.and_then(|a| *a.alias());
        base_alias.map(|id| db[id].to_string())
    }

    /// Overrides for local names.
    fn meta(&'db self, variant: &VariantWalker<'db>) -> HashMap<String, String> {
        let (overrides, defaults) = self.get_attributes(variant);

        let mut meta: HashMap<StringId, StringId> = Default::default();
        match defaults {
            Some(a) => {
                meta.extend(a.meta());
            }
            None => {}
        };

        if let Some(o) = overrides {
            meta.extend(o.meta());
        }

        meta.iter()
            .map(|(&k, &v)| (variant.db[k].to_string(), variant.db[v].to_string()))
            .collect::<HashMap<_, _>>()
    }

    /// Overrides for skip.
    fn skip(&'db self, variant: &VariantWalker<'db>) -> bool {
        let (overrides, defaults) = self.get_attributes(variant);

        let override_alias = overrides.and_then(|o| *o.skip());
        let base_alias = defaults.and_then(|a| *a.skip());
        match (override_alias, base_alias) {
            (Some(id), _) => id,
            (None, Some(id)) => id,
            (None, None) => false,
        }
    }

    /// Overrides for local names.
    fn get_attributes(
        &'db self,
        variant: &VariantWalker<'db>,
    ) -> (
        Option<&'db StaticStringAttributes>,
        Option<&'db StaticStringAttributes>,
    ) {
        let defaults = match self.get_default_attributes() {
            Some(ToStringAttributes::Static(refs)) => Some(refs),
            _ => None,
        };
        let overrides = match self.get_override(variant) {
            Some(ToStringAttributes::Static(refs)) => Some(refs),
            _ => None,
        };

        (overrides, defaults)
    }
}

/// Trait
pub trait WithSerialize: WithSerializeableContent {
    /// Trait to render an object.
    fn serialize(
        &self,
        variant: &VariantWalker<'_>,
        block: &PrinterBlock,
    ) -> Result<String, DatamodelError>;
}

pub fn serialize_with_printer(
    is_enum: bool,
    template: Option<String>,
    json: serde_json::Value,
) -> Result<String, String> {
    if template.is_some() {
        return Err("printer keyword is not yet supported".to_string());
    }
    if is_enum {
        Ok(print_enum_default::print_enum(json))
    } else {
        Ok(print_type_default::print_entry(json))
    }
}
