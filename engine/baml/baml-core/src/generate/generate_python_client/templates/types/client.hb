{{name}} = llm_provider_factory({{#each kwargs as |value key|}}{{key}}={{value}}, {{/each}}options={{options}})
