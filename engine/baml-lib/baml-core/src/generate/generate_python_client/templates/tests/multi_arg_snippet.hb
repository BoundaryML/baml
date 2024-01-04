@baml.{{function_name}}.test
async def test_{{test_case_name}}({{function_name}}Impl: I{{function_name}}):
    def to_str(item: Any) -> str:
        if isinstance(item, str):
            return item
        return dumps(item)

    case = {{{test_case_input}}}
    {{#each test_case_types}}
    deserializer_{{this.name}} = Deserializer[{{this.type}}]({{this.type}}) # type: ignore
    {{this.name}} = deserializer_{{this.name}}.from_string(to_str(case["{{this.name}}"]))
    {{/each}}
    await {{function_name}}Impl(
        {{#each test_case_types}}
        {{this.name}}={{this.name}}{{#unless @last}},{{/unless}}
        {{/each}}
    )


