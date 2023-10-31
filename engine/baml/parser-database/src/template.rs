use std::collections::HashMap;

use handlebars::handlebars_helper;
use internal_baml_diagnostics::DatamodelError;
use internal_baml_prompt_parser::ast::PrinterBlock;
use internal_baml_schema_ast::ast::WithName;
use log::info;

use crate::{
    interner::StringId,
    types::{StaticStringAttributes, ToStringAttributes},
    walkers::VariantWalker,
};

/// Trait
pub trait WithSerializeableContent {
    /// Trait to render an object.
    fn serialize_data(&self, variant: &VariantWalker<'_>) -> serde_json::Value;
}
pub trait WithStaticRenames: WithName {
    fn alias(&self) -> String;

    fn meta(&self) -> HashMap<String, String>;

    fn alias_raw(&self) -> Option<&StringId> {
        match self.static_string_attributes() {
            Some(a) => match a.alias() {
                Some(id) => Some(id),
                None => None,
            },
            None => None,
        }
    }
    fn meta_raw(&self) -> Option<&HashMap<StringId, StringId>> {
        match self.static_string_attributes() {
            Some(a) => Some(a.meta()),
            None => None,
        }
    }
    fn static_string_attributes(&self) -> Option<&StaticStringAttributes> {
        match self.attributes() {
            Some(ToStringAttributes::Static(refs)) => Some(refs),
            _ => None,
        }
    }

    fn attributes(&self) -> Option<&ToStringAttributes>;
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

handlebars_helper!(BLOCK_OPEN: |*_args| "{");
handlebars_helper!(BLOCK_CLOSE: |*_args| "}");
fn init_hs() -> handlebars::Handlebars<'static> {
    let mut reg = handlebars::Handlebars::new();
    reg.register_helper("BLOCK_OPEN", Box::new(BLOCK_OPEN));
    reg.register_helper("BLOCK_CLOSE", Box::new(BLOCK_CLOSE));

    reg
}

pub fn serialize_with_template(
    helper_name: &'static str,
    template: &str,
    json: serde_json::Value,
) -> Result<String, handlebars::RenderError> {
    let mut handlebars = init_hs();
    handlebars.register_partial(helper_name, template)?;

    #[cfg(debug_assertions)]
    {
        info!("Rendering template: {}", helper_name);
        info!("---\n{}\n", serde_json::to_string_pretty(&json).unwrap());
    }

    handlebars.render_template(&format!("{{{{> {} item=this}}}}", helper_name), &json)
}
