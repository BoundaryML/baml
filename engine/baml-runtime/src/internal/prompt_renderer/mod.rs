mod render_output_format;
use jsonish::BamlValueWithFlags;
use render_output_format::render_output_format;

use anyhow::Result;
use baml_types::{BamlValue, FieldType};
use internal_baml_core::{
    error_unsupported,
    ir::{repr::IntermediateRepr, Expression, FunctionWalker, IRHelper},
};
use internal_baml_jinja::{
    types::OutputFormatContent, RenderContext, RenderContext_Client, RenderedPrompt,
    TemplateStringMacro,
};

use crate::RuntimeContext;

pub struct PromptRenderer {
    function_name: String,
    client_name: String,
    output_defs: OutputFormatContent,
    output_type: FieldType,
}

impl PromptRenderer {
    pub fn from_function(
        function: &FunctionWalker,
        ir: &IntermediateRepr,
        ctx: &RuntimeContext,
    ) -> Result<PromptRenderer> {
        let Some(func_v2) = function.as_v2() else {
            error_unsupported!(
                "function",
                function.name(),
                "legacy functions are not supported in the runtime"
            )
        };
        let Some(config) = func_v2.configs.first() else {
            error_unsupported!("function", function.name(), "no valid prompt found")
        };

        Ok(PromptRenderer {
            function_name: function.name().into(),
            client_name: config.client.clone(),
            output_defs: render_output_format(ir, ctx, &func_v2.output.elem)?,
            output_type: func_v2.output.elem.clone(),
        })
    }

    pub fn client_name(&self) -> &str {
        &self.client_name
    }

    pub fn parse(&self, raw_string: &str, allow_partials: bool) -> Result<BamlValueWithFlags> {
        jsonish::from_str(
            &self.output_defs,
            &self.output_type,
            raw_string,
            allow_partials,
        )
    }

    pub fn render_prompt(
        &self,
        ir: &IntermediateRepr,
        ctx: &RuntimeContext,
        params: &BamlValue,
        client_ctx: &RenderContext_Client,
    ) -> Result<RenderedPrompt> {
        let func = ir.find_function(&self.function_name)?;

        let Some(func_v2) = func.as_v2() else {
            error_unsupported!(
                "function",
                self.function_name,
                "legacy functions are not supported in the runtime"
            )
        };

        let Some(config) = func_v2.configs.first() else {
            error_unsupported!("function", self.function_name, "no valid prompt found")
        };

        internal_baml_jinja::render_prompt(
            &config.prompt_template,
            params,
            RenderContext {
                client: client_ctx.clone(),
                tags: ctx.tags.clone(),
                output_format: self.output_defs.clone(),
            },
            &ir.walk_template_strings()
                .map(|t| TemplateStringMacro {
                    name: t.name().into(),
                    args: t
                        .inputs()
                        .iter()
                        .map(|i| (i.name.clone(), i.r#type.elem.to_string()))
                        .collect(),
                    template: t.template().into(),
                })
                .collect::<Vec<_>>(),
        )
    }
}
