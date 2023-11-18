@baml.{{function_name}}.test
async def test_{{test_case_name}}({{function_name}}Impl: I{{function_name}}):
    deserializer = Deserializer[{{test_case_type}}]({{test_case_type}})
    param = deserializer.from_string("""{{{test_case_input}}}""")
    await {{function_name}}Impl(param)
