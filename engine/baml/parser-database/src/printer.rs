use std::collections::HashMap;

use internal_baml_diagnostics::DatamodelError;
use internal_baml_prompt_parser::ast::PrinterBlock;
use internal_baml_schema_ast::ast::WithName;

#[cfg(feature = "use-pyo3")]
use pyo3::Python;
#[cfg(feature = "use-pyo3")]
mod enum_printer;
#[cfg(feature = "use-pyo3")]
mod printer;
#[cfg(feature = "use-pyo3")]
mod type_printer;

#[cfg(feature = "use-pyo3")]
use enum_printer::setup_printer as setup_enum_printer;
#[cfg(feature = "use-pyo3")]
use type_printer::setup_printer as setup_type_printer;

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

#[cfg(feature = "use-pyo3")]
pub fn serialize_with_printer(
    is_enum: bool,
    template: Option<&str>,
    json: serde_json::Value,
) -> Result<String, String> {
    pyo3::prepare_freethreaded_python();

    Python::with_gil(|py| {
        let printer = if is_enum {
            setup_enum_printer(py, template)
                .map_err(|e| format!("Failed to create enum printer: {}", e))?
        } else {
            setup_type_printer(py, template)
                .map_err(|e| format!("Failed to create type printer: {}", e))?
        };

        printer.print(py, json)
    })
}

#[cfg(not(feature = "use-pyo3"))]
pub fn serialize_with_printer(
    is_enum: bool,
    template: Option<&str>,
    json: serde_json::Value,
) -> Result<String, String> {
    Err("Serializers aren't supported here".to_string())
}
