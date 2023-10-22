{{> interface}}

class BAML{{name}}(BaseBAMLFunction[{{return.0.type}}]):
    def __init__(self) -> None:
        super().__init__(
            "{{name}}",
            I{{name}},
            [{{#each impls}}"{{this}}"{{#unless @last}}, {{/unless}}{{/each}}],
        )
