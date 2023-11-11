class BAMLClient:
    {{#each functions}}
    {{this}} = BAML{{this}}
    {{/each}}
    {{#each clients}}
    {{this}} = {{this}}
    {{/each}}

    def __init__(self):
        baml_init(idempotent=True)

baml = BAMLClient()
