@register_deserializer({{{BLOCK_OPEN}}}
{{{alias_pairs}}}
{{{BLOCK_CLOSE}}})
class {{name}}(str, Enum):
    {{#if values}}
    {{#each values}}
    {{> enum_value name=this.name}}

    {{/each}}
    {{else}}
    pass
    {{/if}}