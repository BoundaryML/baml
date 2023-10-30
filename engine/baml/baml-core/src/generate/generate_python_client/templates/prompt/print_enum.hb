{{name}}:
{{#each values}}
{{#if this.description}}
{{this.name}}: {{this.description}}
{{else}}
{{this.name}}
{{/if}}
{{/each}}
