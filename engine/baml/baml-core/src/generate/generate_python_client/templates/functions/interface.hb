@runtime_checkable
class I{{name}}(Protocol):
    """
    This is the interface for a function.

    Args:
        {{#if args.unnamed_arg}}
        arg: {{args.unnamed_arg.type}}
        {{else}}
        {{#each args.named_args}}
        {{this.name}}: {{this.type}}
        {{/each}}
        {{/if}}

    Returns:
        {{return.unnamed_arg.type}}
    """

    {{> func_def func_name="__call__" args=this.args return=this.return}}
        ...

