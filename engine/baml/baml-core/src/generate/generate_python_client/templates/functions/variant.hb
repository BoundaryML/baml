# Impl: {{name}}
# Client: {{client}}
# An implementation of {{function_name}}.


__prompt_template = """\
{{prompt}}\
"""

__output_replacer = {
    {{#each output_replacers}}
    "{{key}}": {{#if (eq value.type "enum")}} """\
{{> print_enum name=value.name values=value.values}}\
""",
    {{else}}
    {{value.type}},
    {{/if}}
    {{/each}}
}

# We ignore the type here because baml does some type magic to make this work
# for inline SpecialForms like Optional, Union, List.
__deserializer = Deserializer[{{function.return.0.type}}]({{function.return.0.type}})  # type: ignore


@BAML{{function.name}}.register_impl("{{name}}")
{{> func_def func_name=name unnamed_args=function.unnamed_args args=function.args return=function.return}}
    prompt = __prompt_template.format({{> arg_values unnamed_args=function.unnamed_args args=function.args}})
    response = await {{client}}.run_prompt(prompt)
    return __deserializer.from_string(response.generated)
