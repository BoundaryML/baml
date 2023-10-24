@register_deserializer()
class {{name}}(str, Enum):
    {{#each values}}
    {{> enum_value name=this}}

    {{/each}}
