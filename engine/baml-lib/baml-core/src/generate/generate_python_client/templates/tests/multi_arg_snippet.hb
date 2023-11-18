@baml.{{function_name}}.test
async def test_{{test_case_name}}({{function_name}}Impl: I{{function_name}}):
    {{#each test_case_input as |input|}}
    deserializer_{{input.name}} = Deserializer[{{input.type}}]({{input.type}})
    {{input.name}} = deserializer_{{input.name}}.from_string("""{{{input.value}}}""")
    {{else}}
    await {{function_name}}Impl(
        {{#each test_case_input as |input|}}
        {{input.name}}={{input.name}}{{#unless @last}},{{/unless}}
        {{/each}}
    )
