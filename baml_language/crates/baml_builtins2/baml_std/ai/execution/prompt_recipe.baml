class PromptRenderRecipe {
    template: ((baml.llm.Context) -> baml.llm.PromptAst throws never)?,
    registered_function: string?,
    arguments: map<string, unknown>,

    /// Builds a recipe from a bare template function. For PROVIDER AUTHORS:
    /// use this when re-rendering a task prompt through a transformed
    /// template (for example, after folding prior turns into the prompt).
    function of(
        template: (baml.llm.Context) -> baml.llm.PromptAst throws never,
    ) -> PromptRenderRecipe throws never {
        PromptRenderRecipe {
            template: template,
            registered_function: null,
            arguments: {},
        }
    }

    function render(
        self,
        provider: Provider,
        output_type: type,
    ) -> baml.llm.PromptAst throws ai.InvalidRequest {
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
    ) -> baml.llm.PromptAst throws ai.InvalidRequest {
        let primitive = provider._render_client();
        match (self.template) {
            let template: ((baml.llm.Context) -> baml.llm.PromptAst throws never) => {
                let context = baml.llm.build_prompt_context(primitive, output_type);
                context.output_format += suffix;
                template(context)
            },
            null => {
                let function_name = self.registered_function ?? baml.sys.panic(
                    "ai: prompt recipe has neither a template nor a registered function",
                );
                let jinja = baml.llm.get_jinja_template(function_name) catch (e) {
                    _ => baml.sys.panic("ai: registered prompt template is missing")
                };
                primitive
                    .render_specialized_prompt(
                        jinja + suffix,
                        self.arguments,
                        output_type,
                        null,
                    )
                    catch (e) {
                        _ => baml.sys.panic("ai: registered prompt could not be rendered")
                    }
            },
        }
    }
}


