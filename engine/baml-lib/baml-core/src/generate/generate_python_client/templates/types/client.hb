{{name}} = LLMManager.add_llm(
    name="{{{name}}}",
    {{#each kwargs as |value key|}}
    {{key}}={{{value}}},
    {{/each}}
    options=dict(
        {{#each options}}
        {{key}}={{{value}}},
        {{/each}}
    ),
)
