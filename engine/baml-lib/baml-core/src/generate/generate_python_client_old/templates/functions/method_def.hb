{{#if unnamed_args}}
async def {{func_name}}(self, {{> arg_list}}) -> {{return.0.type}}:
{{else}}
async def {{func_name}}(self, *, {{> arg_list}}) -> {{return.0.type}}:
{{/if}}