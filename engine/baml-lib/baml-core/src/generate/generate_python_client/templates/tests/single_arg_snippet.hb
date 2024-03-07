@baml.{{function_name}}.test(stream={{#if is_streaming_supported}}True{{else}}False{{/if}})
async def test_{{test_case_name}}({{function_name}}Impl: I{{function_name}}{{#if is_streaming_supported}}Stream{{/if}}, baml_ipc_channel: BaseIPCChannel):
    def to_str(item: Any) -> str:
        if isinstance(item, str):
            return item
        return dumps(item)

    content = to_str({{{test_case_input}}})
    deserializer = Deserializer[{{test_case_type}}]({{test_case_type}}) # type: ignore
    param = deserializer.from_string(content)
    {{#if is_streaming_supported}}
    async with {{function_name}}Impl(param) as stream:
        async for response in stream.parsed_stream:
            baml_ipc_channel.send("partial_response", response.json())

        await stream.get_final_response()
    {{else}}
    await {{function_name}}Impl(param)
    {{/if}}