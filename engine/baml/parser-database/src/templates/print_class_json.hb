{{#if item.type}}
{{#if (eq item.type "output")}}
{{>print_type item=item.type_meta}}
{{/if}}
{{#if (eq item.type "inline")}}
{{>print_type item=item.value}}
{{/if}}
{{#if (eq item.type "union")}}
{{#each item.options}}
{{~> print_type item=this~}}
{{~#unless @last}} | {{/unless~}}
{{>print_type item=item.value}}
{{/each}}
{{/if}}
{{#if (eq item.type "list")}}
{{>print_type item=item.inner}}[]
{{/if}}
{{#if (eq item.type "primitive")}}
{{item.value}}{{#if item.optional}} | null{{/if}}
{{/if}}
{{#if (eq item.type "class")~}}
{{~BLOCK_OPEN}}
{{~#each item.fields}}
	{{#if this.meta.description}}// {{this.meta.description}}{{/if}}
    "{{this.name}}": {{> print_type item=this.type_meta}}
    {{~#unless @last}},{{/unless~}}
{{/each~}}
{{BLOCK_CLOSE~}}
{{/if}}
{{#if (eq item.type "enum")}}
"{{item.name}} as string"{{#if item.optional}} | null{{/if}}
{{~/if}}
{{/if}}