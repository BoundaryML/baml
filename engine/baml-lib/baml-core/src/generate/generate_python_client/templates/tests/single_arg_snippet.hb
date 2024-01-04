@baml.{{function_name}}.test
async def test_{{test_case_name}}({{function_name}}Impl: I{{function_name}}):
    def to_str(item: Any) -> str:
        if isinstance(item, str):
            return item
        return dumps(item)

    content = to_str({{{test_case_input}}})
    deserializer = Deserializer[{{test_case_type}}]({{test_case_type}}) # type: ignore
    param = deserializer.from_string(content)
    await {{function_name}}Impl(param)


