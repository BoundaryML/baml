class BAMLClient:
    {{#each functions}}
    {{this}} = BAML{{this}}()
    {{/each}}

baml = BAMLClient()
