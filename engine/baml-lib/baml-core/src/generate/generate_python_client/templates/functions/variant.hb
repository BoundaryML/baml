# Impl: {{name}}
# Client: {{client}}
# An implementation of {{function.name}}.

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

# Add a deserializer that handles stream responses, which are all Partial types
{{#if output_adapter}}
__partial_deserializer = Deserializer[{{output_adapter.type_partial}}]({{output_adapter.type_partial}})  # type: ignore
{{else}}
__partial_deserializer = Deserializer[{{function.return.0.type_partial}}]({{function.return.0.type_partial}})  # type: ignore
{{/if}}
{{#each overrides}}
__partial_deserializer.overload("{{{name}}}", {{BLOCK_OPEN}}{{#each aliases}}"{{{alias}}}": "{{value}}"{{#unless @last}}, {{/unless}}{{/each}}{{BLOCK_CLOSE}})
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


def {{name}}_stream({{> func_params unnamed_args=this.function.unnamed_args args=this.function.args}}) -> AsyncStream[{{function.return.0.type}}, {{function.return.0.type_partial}}]:
    {{#if output_adapter}}
    raise NotImplementedError("Stream functions do not support output adapters")
    {{else}}
    def run_prompt() -> typing.AsyncIterator[LLMResponse]:
        {{#if input_adapter}}
        adapted_input = input_adapter({{> arg_values unnamed_args=function.unnamed_args args=function.args}})
        raw_stream = {{client}}.run_prompt_template_stream(template=__prompt_template, replacers=__input_replacers, params=dict(arg=adapted_input))
        {{else}}
        raw_stream = {{client}}.run_prompt_template_stream(template=__prompt_template, replacers=__input_replacers, params=dict({{> arg_values unnamed_args=function.unnamed_args args=function.args}}))
        {{/if}}
        return raw_stream
    stream = AsyncStream(stream_cb=run_prompt, partial_deserializer=__partial_deserializer, final_deserializer=__deserializer)
    return stream
    {{/if}}

BAML{{function.name}}.register_impl("{{name}}")({{name}}, {{name}}_stream)