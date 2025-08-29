import pytest
from ..baml_client import b
from ..baml_client.sync_client import b as sync_b


@pytest.mark.asyncio
async def test_expose_request_gpt4():
    request = await b.request.ExtractReceiptInfo("test@email.com", "curiosity")

    assert request.body.json() == {
        "model": "gpt-4o",
        "messages": [
            {
                "role": "system",
                "content": [
                    {
                        "type": "text",
                        "text": 'Given the receipt below:\n\n```\ntest@email.com\n```\n\nAnswer in JSON using this schema:\n{\n  items: [\n    {\n      name: string,\n      description: string or null,\n      quantity: int,\n      price: float,\n    }\n  ],\n  total_cost: float or null,\n  venue: "barisa" or "ox_burger",\n}',
                    }
                ],
            }
        ],
    }


@pytest.mark.asyncio
async def test_expose_request_gemini():
    request = await b.request.TestGeminiSystemAsChat("Dr. Pepper")

    assert request.body.json() == {
        "system_instruction": {"parts": [{"text": "You are a helpful assistant"}]},
        "contents": [
            {
                "parts": [
                    {
                        "text": "Write a nice short story about Dr. Pepper. Keep it to 15 words or less."
                    }
                ],
                "role": "user",
            },
        ],
        "safetySettings": {
            "category": "HARM_CATEGORY_HATE_SPEECH",
            "threshold": "BLOCK_LOW_AND_ABOVE",
        },
    }


@pytest.mark.asyncio
async def test_expose_request_fallback():
    # First client in strategy is GPT4Turbo
    request = await b.request.TestFallbackStrategy("Dr. Pepper")

    assert request.body.json() == {
        "model": "gpt-4-turbo",
        "messages": [
            {
                "role": "system",
                "content": [{"type": "text", "text": "You are a helpful assistant."}],
            },
            {
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": "Write a nice short story about Dr. Pepper",
                    }
                ],
            },
        ],
    }


@pytest.mark.asyncio
async def test_expose_request_round_robin():
    # First client in strategy is Claude
    request = await b.request.TestRoundRobinStrategy("Dr. Pepper")

    assert request.body.json() == {
        "model": "claude-3-haiku-20240307",
        "max_tokens": 1000,
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": "Write a nice short story about Dr. Pepper",
                    }
                ],
            }
        ],
        "system": [{"type": "text", "text": "You are a helpful assistant."}],
    }


def test_expose_request_gpt4_sync():
    request = sync_b.request.ExtractReceiptInfo("test@email.com", "curiosity")

    assert request.body.json() == {
        "model": "gpt-4o",
        "messages": [
            {
                "role": "system",
                "content": [
                    {
                        "type": "text",
                        "text": 'Given the receipt below:\n\n```\ntest@email.com\n```\n\nAnswer in JSON using this schema:\n{\n  items: [\n    {\n      name: string,\n      description: string or null,\n      quantity: int,\n      price: float,\n    }\n  ],\n  total_cost: float or null,\n  venue: "barisa" or "ox_burger",\n}',
                    }
                ],
            }
        ],
    }


@pytest.mark.asyncio
async def test_expose_request_gpt4_stream():
    request = await b.stream_request.ExtractReceiptInfo("test@email.com", "curiosity")

    assert request.body.json() == {
        "model": "gpt-4o",
        "stream": True,
        "stream_options": {
            "include_usage": True,
        },
        "messages": [
            {
                "role": "system",
                "content": [
                    {
                        "type": "text",
                        "text": 'Given the receipt below:\n\n```\ntest@email.com\n```\n\nAnswer in JSON using this schema:\n{\n  items: [\n    {\n      name: string,\n      description: string or null,\n      quantity: int,\n      price: float,\n    }\n  ],\n  total_cost: float or null,\n  venue: "barisa" or "ox_burger",\n}',
                    }
                ],
            }
        ],
    }


def test_expose_request_gpt4_stream_sync():
    request = sync_b.stream_request.ExtractReceiptInfo("test@email.com", "curiosity")

    assert request.body.json() == {
        "model": "gpt-4o",
        "stream": True,
        "stream_options": {
            "include_usage": True,
        },
        "messages": [
            {
                "role": "system",
                "content": [
                    {
                        "type": "text",
                        "text": 'Given the receipt below:\n\n```\ntest@email.com\n```\n\nAnswer in JSON using this schema:\n{\n  items: [\n    {\n      name: string,\n      description: string or null,\n      quantity: int,\n      price: float,\n    }\n  ],\n  total_cost: float or null,\n  venue: "barisa" or "ox_burger",\n}',
                    }
                ],
            }
        ],
    }
