@register_deserializer({{{BLOCK_OPEN}}} {{#each fields}}{{#if alias}}"{{alias}}": "{{name}}",{{/if}}{{/each}} {{{BLOCK_CLOSE}}})
class Partial{{name}}(BaseModel):
    {{#if (eq num_fields 0)}}
    pass
    {{/if}}
    {{#each fields}}
    {{name}}: {{type_partial}}{{#if can_be_null}} = None{{/if}}
    {{/each}}
    {{#each properties}}
    @property
    def {{name}}(self) -> {{type}}:
        {{> print_code code=this.code}}

    {{/each}}
