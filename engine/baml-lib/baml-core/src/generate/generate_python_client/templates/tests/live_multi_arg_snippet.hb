@baml.{{function_name}}.test
async def test_{{test_case_name}}({{function_name}}Impl: I{{function_name}}):
    {{#each test_case_input}}
    deserializer_{{this.name}} = Deserializer[{{this.type}}]({{this.type}}) # type: ignore
    {{this.name}} = deserializer_{{this.name}}.from_string("""\
{{{this.value}}}\
""")
    {{/each}}
    await {{function_name}}Impl(
        {{#each test_case_input}}
        {{this.name}}={{this.name}}{{#unless @last}},{{/unless}}
        {{/each}}
    )


