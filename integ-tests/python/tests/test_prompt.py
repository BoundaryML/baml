import pytest
from dotenv import load_dotenv

load_dotenv()
from ..baml_client import b
from ..baml_client.sync_client import b as sync_b


@pytest.mark.asyncio
async def test_expose_prompt_gpt4():
    prompt = await b.prompt.ExtractReceiptInfo("test@email.com", "curiosity")

    assert prompt == {
        'messages': [
            {
                'role': 'system',
                'content': [
                    {
                        'type': 'text',
                        'text': 'Given the receipt below:\n\n```\ntest@email.com\n```\n\nAnswer in JSON using this schema:\n{\n  items: [\n    {\n      name: string,\n      description: string or null,\n      quantity: int,\n      price: float,\n    }\n  ],\n  total_cost: float or null,\n  venue: "barisa" or "ox_burger",\n}'
                    }
                ]
            }
        ]
    }

@pytest.mark.asyncio
async def test_expose_prompt_gemini():
    prompt = await b.prompt.TestGeminiSystemAsChat("Dr. Pepper")

    assert prompt == {
        'system_instruction': {
            'parts': [{'text': 'You are a helpful assistant'}]
        },
        'contents': [
            {
                'parts': [{'text': 'Write a nice short story about Dr. Pepper'}],
                'role': 'user'
            },
        ]
    }

@pytest.mark.asyncio
async def test_expose_prompt_fallback():
    # First client in strategy is GPT4Turbo
    prompt = await b.prompt.TestFallbackStrategy("Dr. Pepper")

    assert prompt == {
        'messages': [
            {
                'role': 'system',
                'content': [{
                    'type': 'text',
                    'text': 'You are a helpful assistant.'
                }]
            },
            {
                'role': 'user',
                'content': [{
                    'type': 'text',
                    'text': 'Write a nice short story about Dr. Pepper'
                }]
            }
        ]
    }

@pytest.mark.asyncio
async def test_expose_prompt_round_robin():
    # First client in strategy is Claude
    prompt = await b.prompt.TestRoundRobinStrategy("Dr. Pepper")

    assert prompt == {
        'messages': [
            {
                'role': 'user',
                'content': [
                    {
                        'type': 'text',
                        'text': 'Write a nice short story about Dr. Pepper'
                    }
                ]
            }
        ],
        'system': [
            {
                'type': 'text',
                'text': 'You are a helpful assistant.'
            }
        ]
    }

@pytest.mark.asyncio
async def test_expose_prompt_gpt4_sync():
    prompt = sync_b.prompt.ExtractReceiptInfo("test@email.com", "curiosity")

    assert prompt == {
        'messages': [
            {
                'role': 'system',
                'content': [
                    {
                        'type': 'text',
                        'text': 'Given the receipt below:\n\n```\ntest@email.com\n```\n\nAnswer in JSON using this schema:\n{\n  items: [\n    {\n      name: string,\n      description: string or null,\n      quantity: int,\n      price: float,\n    }\n  ],\n  total_cost: float or null,\n  venue: "barisa" or "ox_burger",\n}'
                    }
                ]
            }
        ]
    }
