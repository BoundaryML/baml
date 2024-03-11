{{{name}}} = create_retry_policy_{{strategy.type}}(
  max_retries={{max_retries}},
  {{#each strategy.params as |value key|}}
  {{key}}={{value}}{{#unless @last}},{{/unless}}
  {{/each}}
)
