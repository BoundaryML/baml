@register_deserializer({{{BLOCK_OPEN}}} {{#each fields}}{{#if alias}}"{{alias}}": "{{name}}",{{/if}}{{/each}} {{{BLOCK_CLOSE}}})
class {{name}}(BaseModel):
    {{#if (eq num_fields 0)}}
    pass
    {{/if}}
    {{#each fields}}
    {{name}}: {{type}}{{#if optional}} = None{{/if}}
    {{/each}}
    {{#each properties}}
    @property
    def {{name}}(self) -> {{type}}:
        {{> print_code code=this.code}}

    {{/each}}
