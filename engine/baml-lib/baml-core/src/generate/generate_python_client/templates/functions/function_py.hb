{{> interface}}

class IBAML{{name}}(BaseBAMLFunction[{{return.0.type}}]):
    def __init__(self) -> None:
        super().__init__(
            "{{name}}",
            I{{name}},
            [{{#each impls}}"{{this}}"{{#unless @last}}, {{/unless}}{{/each}}],
        )

BAML{{name}} = IBAML{{name}}()

__all__ = [ "BAML{{name}}" ]
