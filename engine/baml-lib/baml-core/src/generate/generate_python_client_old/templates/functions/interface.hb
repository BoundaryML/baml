I{{name}}Output = {{this.return.0.type}}

@runtime_checkable
class I{{name}}(Protocol):
    """
    This is the interface for a function.

    Args:
        {{#if unnamed_args}}
        arg: {{args.0.type}}
        {{else}}
        {{#each args}}
        {{this.name}}: {{this.type}}
        {{/each}}
        {{/if}}

    Returns:
        {{return.0.type}}
    """

    {{> method_def func_name="__call__" unnamed_args=this.unnamed_args args=this.args return=this.return}}
        ...

   

@runtime_checkable
class I{{name}}Stream(Protocol):
    """
    This is the interface for a stream function.

    Args:
        {{#if unnamed_args}}
        arg: {{args.0.type}}
        {{else}}
        {{#each args}}
        {{this.name}}: {{this.type}}
        {{/each}}
        {{/if}}

    Returns:
        AsyncStream[{{return.0.type}}, {{return.0.type_partial}}]
    """

    def __call__(self, {{> func_params unnamed_args=this.unnamed_args args=this.args}}) -> AsyncStream[{{return.0.type}}, {{return.0.type_partial}}]:
        ...