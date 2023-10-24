@register_deserializer()
class {{name}}(BaseModel):
    {{#each fields}}
    {{name}}: {{type}}{{#if optional}} = None{{/if}}
    {{/each}}
