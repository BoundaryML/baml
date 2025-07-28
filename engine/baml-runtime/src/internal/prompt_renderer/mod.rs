mod render_output_format;
pub(crate) mod scoped_ir;
use anyhow::Result;
use baml_types::{BamlValue, StreamingMode, TypeIR, TypeValue};
use internal_baml_core::{
    error_unsupported,
    ir::{
        repr::IntermediateRepr, FunctionWalker, IRHelper, IRHelperExtended,
        IRSemanticStreamingHelper,
    },
};
use internal_baml_jinja::{
    types::OutputFormatContent, RenderContext, RenderContext_Client, RenderedPrompt,
    TemplateStringMacro,
};
use internal_llm_client::ClientSpec;
use jsonish::{BamlValueWithFlags, ResponseBamlValue};
use render_output_format::render_output_format;
use scoped_ir::ScopedIr;

use super::llm_client::parsed_value_to_response;
use crate::{runtime_context::RuntimeClassOverride, RuntimeContext};

#[derive(Debug)]
pub struct PromptRenderer {
    pub function_name: String,
    pub client_spec: ClientSpec,
    non_streaming: TypeDefinitionWrapper,
    streaming: TypeDefinitionWrapper,
}

#[derive(Debug)]
struct TypeDefinitionWrapper {
    defintions: OutputFormatContent,
    target: TypeIR,
}

impl PromptRenderer {
    pub fn from_function(
        function: &FunctionWalker,
        ir: &IntermediateRepr,
        ctx: &RuntimeContext,
    ) -> Result<PromptRenderer> {
        let func_v2 = function.elem();
        let Some(config) = func_v2.configs.first() else {
            error_unsupported!("function", function.name(), "no valid prompt found")
        };

        Ok(PromptRenderer {
            function_name: function.name().into(),
            client_spec: match &ctx.client_overrides {
                Some((Some(client), _)) => ClientSpec::new_from_id(client)?,
                _ => config.client.clone(),
            },
            non_streaming: TypeDefinitionWrapper {
                defintions: render_output_format(
                    ir,
                    ctx,
                    &func_v2.output,
                    StreamingMode::NonStreaming,
                )?,
                target: func_v2.output.clone(),
            },
            streaming: TypeDefinitionWrapper {
                defintions: render_output_format(
                    ir,
                    ctx,
                    &func_v2.output,
                    StreamingMode::Streaming,
                )?,
                target: func_v2.output.to_streaming_type(ir).to_ir_type(),
            },
        })
    }

    /// A temporary function used to generate a fake prompt renderer, for cases
    /// when we call BamlRuntime's `call` API with Expression fns, which
    /// don't have a prompt.
    pub fn mk_fake() -> PromptRenderer {
        PromptRenderer {
            function_name: "fake".into(),
            client_spec: ClientSpec::Named("fake".into()),
            non_streaming: TypeDefinitionWrapper {
                defintions: OutputFormatContent::mk_fake(),
                target: TypeIR::Primitive(TypeValue::String, Default::default()),
            },
            streaming: TypeDefinitionWrapper {
                defintions: OutputFormatContent::mk_fake(),
                target: TypeIR::Primitive(TypeValue::String, Default::default()),
            },
        }
    }

    pub fn client_spec(&self) -> &ClientSpec {
        &self.client_spec
    }

    pub fn parse(
        &self,
        ir: &IntermediateRepr,
        ctx: &RuntimeContext,
        raw_string: &str,
        allow_partials: bool,
    ) -> Result<ResponseBamlValue> {
        let (def, target) = if allow_partials {
            (&self.streaming.defintions, &self.streaming.target)
        } else {
            (&self.non_streaming.defintions, &self.non_streaming.target)
        };

        let parsed = jsonish::from_str(def, target, raw_string, !allow_partials)?;

        // TODO(vbv): We should consider using def here instead of (ir / ctx)
        // since def has all the context for the mode (streaming / non-streaming)
        let scoped_ir = ScopedIr::new(ir, ctx);

        parsed_value_to_response(
            &scoped_ir,
            parsed,
            if allow_partials {
                baml_types::StreamingMode::Streaming
            } else {
                baml_types::StreamingMode::NonStreaming
            },
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

        let func_v2 = func.elem();

        let Some(config) = func_v2.configs.first() else {
            error_unsupported!("function", self.function_name, "no valid prompt found")
        };

        internal_baml_jinja::render_prompt(
            &config.prompt_template,
            params,
            RenderContext {
                client: client_ctx.clone(),
                tags: ctx.tags.clone(),
                output_format: self.non_streaming.defintions.clone(),
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
            ir,
            ctx.env_vars(),
        )
    }
}
