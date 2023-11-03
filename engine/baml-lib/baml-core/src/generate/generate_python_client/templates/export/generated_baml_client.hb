class BAMLClient:
    {{#each functions}}
    {{this}} = BAML{{this}}
    {{/each}}
    {{#each clients}}
    {{this}} = {{this}}
    {{/each}}

baml = BAMLClient()
