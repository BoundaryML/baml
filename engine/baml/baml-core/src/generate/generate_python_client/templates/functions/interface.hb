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

    {{> func_def func_name="__call__" unnamed_args=this.unnamed_args args=this.args return=this.return}}
        ...

