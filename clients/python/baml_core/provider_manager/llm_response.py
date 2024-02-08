from pydantic import BaseModel, Field
import typing


class LLMResponse(BaseModel):
    generated: str
    mdl_name: str = Field(alias="model_name")
    meta: typing.Any

    @property
    def ok(self) -> bool:
        if isinstance(self.meta, dict):
            return bool(self.meta.get("baml_is_complete", True))
        return True
