use anyhow::Result;
use baml_types::BamlValue;
use internal_baml_core::{error_unsupported, ir::FunctionWalker};
use internal_baml_jinja::{
    RenderContext, RenderContext_Client, RenderedPrompt, TemplateStringMacro,
};

use crate::RuntimeContext;

pub struct PromptRenderer {
    template_macros: Vec<TemplateStringMacro>,
    pub name: String,
    // TODO: We technically have all the information given output type
    // and we should derive this each time.
    pub output_format: String,
    pub prompt_template: String,
    pub client_name: String,
}

impl PromptRenderer {
    pub fn from_function(function: &FunctionWalker) -> Result<PromptRenderer> {
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
                        template_macros,
                        name: function.name().into(),
                        output_format: c.item.1.output_format.clone(),
                        prompt_template: c.item.1.prompt_template.clone(),

                        client_name: c.item.1.client.clone(),
                    });
                }
                error_unsupported!("function", function.name(), "no valid prompt found")
            }
        }
    }

    pub fn output_format(&self) -> &str {
        &self.output_format
    }

    pub fn prompt_template(&self) -> &str {
        &self.prompt_template
    }

    pub fn template_macros(&self) -> &Vec<TemplateStringMacro> {
        &self.template_macros
    }

    pub fn client_name(&self) -> &str {
        &self.client_name
    }

    pub fn render_prompt(
        &self,
        ctx: &RuntimeContext,
        params: &BamlValue,
        client_ctx: &RenderContext_Client,
    ) -> Result<RenderedPrompt> {
        internal_baml_jinja::render_prompt(
            self.prompt_template(),
            params,
            &RenderContext {
                client: client_ctx.clone(),
                output_format: self.output_format().into(),
                env: ctx.env.clone(),
            },
            self.template_macros(),
        )
    }
}
