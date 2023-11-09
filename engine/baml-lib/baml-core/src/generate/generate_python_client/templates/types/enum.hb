@register_deserializer({{{BLOCK_OPEN}}} {{#each values}}{{#if alias}}"{{alias}}": "{{name}}",{{/if}}{{/each}} {{{BLOCK_CLOSE}}})
class {{name}}(str, Enum):
    {{#each values}}
    {{> enum_value name=this.name}}

    {{/each}}
