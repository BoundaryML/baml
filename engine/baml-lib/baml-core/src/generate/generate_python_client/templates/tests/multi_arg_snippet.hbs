@baml.{{function_name}}.test(stream={{#if is_streaming_supported}}True{{else}}False{{/if}})
async def test_{{test_case_name}}({{function_name}}Impl: I{{function_name}}{{#if is_streaming_supported}}Stream{{/if}}, baml_ipc_channel: BaseIPCChannel):
    def to_str(item: Any) -> str:
        if isinstance(item, str):
            return item
        return dumps(item)

    case = {{{test_case_input}}}
    {{#each test_case_types}}
    deserializer_{{this.name}} = Deserializer[{{this.type}}]({{this.type}}) # type: ignore
    {{this.name}} = deserializer_{{this.name}}.from_string(to_str(case["{{this.name}}"]))
    {{/each}}
    {{#if is_streaming_supported}}
    async with {{function_name}}Impl(
        {{#each test_case_types}}
        {{this.name}}={{this.name}}{{#unless @last}},{{/unless}}
        {{/each}}
    ) as stream:
        async for response in stream.parsed_stream:
            baml_ipc_channel.send("partial_response", response.json())

        await stream.get_final_response()
    {{else}}
    await {{function_name}}Impl(
        {{#each test_case_types}}
        {{this.name}}={{this.name}}{{#unless @last}},{{/unless}}
        {{/each}}
    )
    {{/if}}
