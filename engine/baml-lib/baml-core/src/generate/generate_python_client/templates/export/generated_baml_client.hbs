class BAMLClient:
    {{#each functions}}
    {{this}} = BAML{{this}}
    {{/each}}
    {{#each clients}}
    {{this}} = {{this}}
    {{/each}}

    def __init__(self):
        baml_init()

    def configure(
        self,
        project_id: Optional[str] = None,
        secret_key: Optional[str] = None,
        base_url: Optional[str] = None,
        enable_cache: Optional[bool] = None,
        stage: Optional[str] = None,
    ):
        return baml_init(
            project_id=project_id,
            secret_key=secret_key,
            base_url=base_url,
            enable_cache=enable_cache,
            stage=stage,
        )

    def add_before_send_message_hook(self, hook: Callable[[LogSchema], None]):
        add_message_transformer_hook(hook)

    def flush(self):
        flush_trace_logs()


baml = BAMLClient()
