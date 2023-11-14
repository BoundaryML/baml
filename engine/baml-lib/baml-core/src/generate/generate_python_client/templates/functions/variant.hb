# Impl: {{name}}
# Client: {{client}}
# An implementation of {{function_name}}.


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
__deserializer = Deserializer[{{function.return.0.type}}]({{function.return.0.type}})  # type: ignore
{{#each overrides}}
__deserializer.overload("{{{name}}}", {{BLOCK_OPEN}}{{#each aliases}}"{{{alias}}}": "{{value}}"{{#unless @last}}, {{/unless}}{{/each}}{{BLOCK_CLOSE}})
{{/each}}


{{#if pre_deserializer}}
def pre_deserializer(raw: str) -> str:
    {{> print_code code=pre_deserializer}}
{{/if}}


@BAML{{function.name}}.register_impl("{{name}}")
{{> func_def func_name=name unnamed_args=function.unnamed_args args=function.args return=function.return}}
    response = await {{client}}.run_prompt_template(template=__prompt_template, replacers=__input_replacers, params=dict({{> arg_values unnamed_args=function.unnamed_args args=function.args}}))
    {{#if pre_deserializer}}
    pre_processed = pre_deserializer(response.generated)
    return __deserializer.from_string(pre_processed)
    {{else}}
    return __deserializer.from_string(response.generated)
    {{/if}}
