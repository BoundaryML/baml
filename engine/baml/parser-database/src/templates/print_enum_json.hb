{{item.name}}:
{{#each item.values}}
{{#if this.meta.description}}
{{this.name}}: {{this.meta.description}}
{{else}}
{{item.name}}
{{/if}}{{/each}}