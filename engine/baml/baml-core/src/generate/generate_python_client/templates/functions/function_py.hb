{{> interface}}

class BAML{{name}}(BaseBAMLFunction):
    def __init__(self) -> None:
        super().__init__(
            "{{name}}",
            I{{name}},
            [{{join impls}}],
        )
