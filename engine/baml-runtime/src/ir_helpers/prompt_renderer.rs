use anyhow::Result;
use internal_baml_core::ir::repr::FunctionConfig;
use internal_baml_jinja::TemplateStringMacro;

use crate::error_unsupported;

use super::{FunctionWalker, TemplateStringWalker};

pub struct PromptRenderer<'ir> {
    template_macros: Vec<TemplateStringMacro>,
    config: &'ir FunctionConfig,
}

impl PromptRenderer<'_> {
    pub fn from_function<'ir>(function: &'ir FunctionWalker) -> Result<PromptRenderer<'ir>> {
        // Generate the prompt.
        match function.walk_impls() {
            either::Either::Left(_) => {
                error_unsupported!(
                    "function",
                    function.name(),
                    "legacy functions are not supported in the runtime"
                )
            }
            either::Either::Right(configs) => {
                let template_macros = function
                    .db
                    .walk_template_strings()
                    .map(|t| TemplateStringMacro {
                        name: t.name().into(),
                        args: t
                            .inputs()
                            .iter()
                            .map(|i| (i.name.clone(), i.r#type.elem.to_string()))
                            .collect(),
                        template: t.template().into(),
                    })
                    .collect();
                for c in configs {
                    return Ok(PromptRenderer {
                        config: c.item.1,
                        template_macros,
                    });
                }
                error_unsupported!("function", function.name(), "no valid prompt found")
            }
        }
    }

    pub fn prompt_template(&self) -> &str {
        &self.config.prompt_template
    }

    pub fn template_macros(&self) -> &Vec<TemplateStringMacro> {
        &self.template_macros
    }
}
