# Impl: {{name}}
# Client: {{client}}
# An implementation of {{function.name}}.

__prompt_template = """\
{{{prompt}}}\
"""

# We ignore the type here because baml does some type magic to make this work
# for inline SpecialForms like Optional, Union, List.
__deserializer = Deserializer[{{function.return.0.type}}]({{function.return.0.type}})  # type: ignore

# Add a deserializer that handles stream responses, which are all Partial types
__partial_deserializer = Deserializer[{{function.return.0.type_partial}}]({{function.return.0.type_partial}})  # type: ignore

__output_schema = """
{{output_schema}}
""".strip()

__template_macros = [
{{#each template_macros}}
    RenderData.template_string_macro(
        name="{{name}}",
        args=[
            {{#each args}}
            ("{{name}}", "{{type}}"),
            {{/each}}
        ],
        template="""{{{template}}}""",
    ),
{{/each}}
]


{{> func_def func_name=name unnamed_args=function.unnamed_args args=function.args return=function.return}}
    response = await {{client}}.run_jinja_template(
        jinja_template=__prompt_template,
        output_schema=__output_schema, template_macros=__template_macros,
        args=dict({{> arg_values unnamed_args=function.unnamed_args args=function.args}})
    )
    deserialized = __deserializer.from_string(response.generated)
    return deserialized


def {{name}}_stream({{> func_params unnamed_args=this.function.unnamed_args args=this.function.args}}) -> AsyncStream[{{function.return.0.type}}, {{function.return.0.type_partial}}]:
    def run_prompt() -> typing.AsyncIterator[LLMResponse]:
        raw_stream = {{client}}.run_jinja_template_stream(
            jinja_template=__prompt_template,
            output_schema=__output_schema, template_macros=__template_macros,
            args=dict({{> arg_values unnamed_args=function.unnamed_args args=function.args}})
        )
        return raw_stream
    stream = AsyncStream(stream_cb=run_prompt, partial_deserializer=__partial_deserializer, final_deserializer=__deserializer)
    return stream

BAML{{function.name}}.register_impl("{{name}}")({{name}}, {{name}}_stream)