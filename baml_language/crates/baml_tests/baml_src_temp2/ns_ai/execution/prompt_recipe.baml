class PromptRenderRecipe {
    template: (baml.llm.Context) -> baml.llm.PromptAst throws never,

    function render(
        self,
        provider: Provider,
        output_type: type,
    ) -> baml.llm.PromptAst throws never {
        self.render_with_output_format(provider, output_type, "")
    }

    // Runners that add another model-visible result branch extend
    // ctx.output_format here. Full prompt/SAP tool fallback renders the
    // combined T | ToolCalls type, then appends the active tool catalog. The
    // LLM function still owns where that complete contract appears.
    function render_with_output_format(
        self,
        provider: Provider,
        output_type: type,
        suffix: string,
    ) -> baml.llm.PromptAst throws never {
        let primitive = baml.llm.from_shorthand(provider.render_shorthand()) catch (e) {
            _ => baml.sys.panic("ai: invalid provider render shorthand"),
        };
        let base = baml.llm.build_prompt_context(primitive, output_type);
        base.output_format += suffix;
        self.template(base)
    }
}
