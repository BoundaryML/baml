use anyhow::Result;
use internal_baml_core::{
    error_unsupported,
    ir::{repr::FunctionConfig, FunctionWalker},
};
use internal_baml_jinja::{
    RenderContext, RenderContext_Client, RenderedPrompt, TemplateStringMacro,
};

use crate::RuntimeContext;

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

    pub fn output_format(&self) -> &str {
        &self.config.output_format
    }

    pub fn prompt_template(&self) -> &str {
        &self.config.prompt_template
    }

    pub fn template_macros(&self) -> &Vec<TemplateStringMacro> {
        &self.template_macros
    }

    pub fn client_name(&self) -> &str {
        &self.config.client
    }

    pub fn render_prompt(
        &self,
        ctx: &RuntimeContext,
        params: &serde_json::Value,
        client_ctx: RenderContext_Client,
    ) -> Result<RenderedPrompt> {
        internal_baml_jinja::render_prompt(
            self.prompt_template(),
            params,
            &RenderContext {
                client: client_ctx,
                output_format: self.output_format().into(),
                env: ctx.env.clone(),
            },
            self.template_macros(),
        )
    }
}
