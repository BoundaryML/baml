# Impl: {{name}}
# Client: {{client}}
# An implementation of {{function.name}}.

parser = JSONParser()
__prompt_template = """\
{{{prompt}}}\
"""

__input_replacers = {
    {{#each inputs}}
    "{{BLOCK_OPEN}}{{{this}}}{{BLOCK_CLOSE}}"{{#unless @last}},{{/unless}}
    {{/each}}
}


# We ignore the type here because baml does some type magic to make this work
# for inline SpecialForms like Optional, Union, List.
{{#if output_adapter}}
__deserializer = Deserializer[{{output_adapter.type}}]({{output_adapter.type}})  # type: ignore
{{else}}
__deserializer = Deserializer[{{function.return.0.type}}]({{function.return.0.type}})  # type: ignore
{{/if}}
{{#each overrides}}
__deserializer.overload("{{{name}}}", {{BLOCK_OPEN}}{{#each aliases}}"{{{alias}}}": "{{value}}"{{#unless @last}}, {{/unless}}{{/each}}{{BLOCK_CLOSE}})
{{/each}}

# Now add a deserializer that handles stream responses, which are all Partial types
{{#if output_adapter}}
__partial_deserializer = Deserializer[Partial{{output_adapter.type}}]({{output_adapter.type}})  # type: ignore
{{else}}
__partial_deserializer = Deserializer[Partial{{function.return.0.type}}](Partial{{function.return.0.type}})  # type: ignore
{{/if}}
{{#each overrides}}
__deserializer.overload("{{{name}}}", {{BLOCK_OPEN}}{{#each aliases}}"{{{alias}}}": "{{value}}"{{#unless @last}}, {{/unless}}{{/each}}{{BLOCK_CLOSE}})
{{/each}}


{{#if output_adapter}}
def output_adapter(arg: {{output_adapter.type}}) -> {{function.return.0.type}}:
    {{> print_code code=output_adapter.code}}
{{/if}}


{{#if input_adapter}}
{{> func_def func_name="input_adapter" unnamed_args=function.unnamed_args args=function.args return=input_adapter.type}}
    {{> print_code code=input_adapter.code}}
{{/if}}



{{> func_def func_name=name unnamed_args=function.unnamed_args args=function.args return=function.return}}
    {{#if input_adapter}}
    adapted_input = input_adapter({{> arg_values unnamed_args=function.unnamed_args args=function.args}})
    response = await {{client}}.run_prompt_template(template=__prompt_template, replacers=__input_replacers, params=dict(arg=adapted_input))
    {{else}}
    response = await {{client}}.run_prompt_template(template=__prompt_template, replacers=__input_replacers, params=dict({{> arg_values unnamed_args=function.unnamed_args args=function.args}}))
    {{/if}}
    deserialized = __deserializer.from_string(response.generated)
    {{#if output_adapter}}
    return output_adapter(deserialized)
    {{else}}
    return deserialized
    {{/if}}


async def {{name}}_stream({{> func_params unnamed_args=this.function.unnamed_args args=this.function.args}}) -> typing.AsyncIterator[BAMLStreamResponse[{{function.return.0.type}}, Partial{{function.return.0.type}}]]:
    {{#if output_adapter}}
    raise NotImplementedError("Stream functions do not support output adapters")
    {{else}}

    {{#if input_adapter}}
    adapted_input = input_adapter({{> arg_values unnamed_args=function.unnamed_args args=function.args}})
    response = {{client}}.run_prompt_template_stream(template=__prompt_template, replacers=__input_replacers, params=dict(arg=adapted_input))
    {{else}}
    response = {{client}}.run_prompt_template_stream(template=__prompt_template, replacers=__input_replacers, params=dict({{> arg_values unnamed_args=function.unnamed_args args=function.args}}))
    {{/if}}
    total_generated = ""
    async for response in response:
        total_generated += response.generated
        parsed = parser.parse(total_generated)
        try:
            deserialized = __partial_deserializer.from_string(parsed)
            yield BAMLStreamResponse.from_parsed_partial(
                partial=deserialized,
                delta=response.generated,
            )
        except Exception as e:
            yield BAMLStreamResponse.from_failed_partial(
                delta=response.generated,
            )

    final_response = __deserializer.from_string(total_generated)
    yield BAMLStreamResponse.from_final_response(
        response=final_response
    )
    {{/if}}

BAML{{function.name}}.register_impl("{{name}}")({{name}}, {{name}}_stream)