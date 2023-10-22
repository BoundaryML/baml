{{#if args.unnamed_arg}}
arg: {{args.unnamed_arg.type}}, /{{else}}
{{#each args.named_args}}{{this.name}}: {{this.type}}{{#unless @last}}, {{/unless}}{{/each}}{{/if}}