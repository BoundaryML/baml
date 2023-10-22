{{#if args.unnamed_arg}}
async def {{func_name}}(self, {{> arg_list}}) -> Awaitable[{{return.unnamed_arg.type}}]:
{{else}}
async def {{func_name}}(self, *, {{> arg_list}}) -> Awaitable[{{return.unnamed_arg.type}}]:
{{/if}}