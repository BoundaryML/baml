class PromptRenderRecipe {
    template: (baml.llm.Context) -> baml.llm.PromptAst throws never,

    function render(
        self,
        provider: Provider,
        output_type: type,
    ) -> baml.llm.PromptAst throws never {
        self.render_with_output_format(provider, output_type, "")
    }

    // Runners that add another model-visible result branch, such as the
    // prompt/SAP tool fallback, extend ctx.output_format here. The LLM function
    // still owns where that contract appears in its prompt.
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
