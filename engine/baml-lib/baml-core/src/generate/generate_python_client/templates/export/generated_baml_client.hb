class BAMLClient:
    {{#each functions}}
    {{this}} = BAML{{this}}
    {{/each}}
    {{#each clients}}
    {{this}} = {{this}}
    {{/each}}

    def __init__(self):
        baml_init(idempotent=True)

    def configure(
        self,
        project_id: Optional[str] = None,
        secret_key: Optional[str] = None,
        base_url: Optional[str] = None,
        enable_cache: Optional[bool] = None,
        stage: Optional[str] = None,
        message_transformer_hook: Optional[Callable[[LogSchema], None]] = None,
        before_message_export_hook: Optional[Callable[[List[LogSchema]], None]] = None,
    ):
        return baml_init(
            project_id=project_id,
            secret_key=secret_key,
            base_url=base_url,
            enable_cache=enable_cache,
            stage=stage,
            before_message_export_hook=before_message_export_hook,
            message_transformer_hook=message_transformer_hook,
        )

    def flush(self):
        flush_trace_logs()


baml = BAMLClient()
