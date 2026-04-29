from __future__ import annotations

import pydantic


class ExecutionContext(pydantic.BaseModel): ...


class OrchestrationStep(pydantic.BaseModel): ...


class PlannerState(pydantic.BaseModel): ...


class AnthropicOptions(pydantic.BaseModel): ...


class AzureOpenAiOptions(pydantic.BaseModel): ...


class BedrockOptions(pydantic.BaseModel): ...


class Client(pydantic.BaseModel): ...


class MediaUrlHandler(pydantic.BaseModel): ...


class PrimitiveClient(pydantic.BaseModel): ...


class PrimitiveClientOptions(pydantic.BaseModel): ...


class PromptAst(pydantic.BaseModel): ...


class RetryPolicy(pydantic.BaseModel): ...


class Stream(pydantic.BaseModel): ...


class StreamAccumulator(pydantic.BaseModel): ...


class StreamCache(pydantic.BaseModel): ...


class VertexAiOptions(pydantic.BaseModel): ...


__all__ = [
    "ExecutionContext",
    "OrchestrationStep",
    "PlannerState",
    "AnthropicOptions",
    "AzureOpenAiOptions",
    "BedrockOptions",
    "Client",
    "MediaUrlHandler",
    "PrimitiveClient",
    "PrimitiveClientOptions",
    "PromptAst",
    "RetryPolicy",
    "Stream",
    "StreamAccumulator",
    "StreamCache",
    "VertexAiOptions",
]
