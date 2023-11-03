@register_deserializer({{{BLOCK_OPEN}}} {{#each fields}}{{#if alias}}"{{alias}}": "{{name}}",{{/if}}{{/each}} {{{BLOCK_CLOSE}}})
class {{name}}(BaseModel):
    {{#each fields}}
    {{name}}: {{type}}{{#if optional}} = None{{/if}}
    {{/each}}
