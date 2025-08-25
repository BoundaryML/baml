import pytest
from baml_py import Image, Audio, Pdf
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


@pytest.mark.asyncio
async def test_expose_request_openai_responses_multimodal():
    test_image = Image.from_url(
        "https://upload.wikimedia.org/wikipedia/commons/thumb/d/dd/Gfp-wisconsin-madison-the-nature-boardwalk.jpg/2560px-Gfp-wisconsin-madison-the-nature-boardwalk.jpg"
    )
    request = await b.request.TestOpenAIResponsesImageInput(test_image)

    assert request.body.json() == {
        "model": "o1-mini",
        "input": [
            {
                "role": "user",
                "content": [
                    {"type": "input_text", "text": "what is in this content?"},
                    {
                        "type": "input_image",
                        "image_url": "https://upload.wikimedia.org/wikipedia/commons/thumb/d/dd/Gfp-wisconsin-madison-the-nature-boardwalk.jpg/2560px-Gfp-wisconsin-madison-the-nature-boardwalk.jpg",
                    },
                ],
            }
        ],
    }


@pytest.mark.asyncio
async def test_expose_request_openai_responses_audio():
    test_audio_data = "UklGRnoGAABXQVZFZm10IBAAAAABAAEAQB8AAEAfAAABAAgAZGF0YQoGAACBhYqFbF1fdJivrJBhNjVgodDbq2EcBj+a2/LDciUFLIHO8tiJNwgZaLvt559NEAxQp+PwtmMcBjiR1/LMeSwFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoF"
    test_audio = Audio.from_base64("audio/wav", test_audio_data)
    request = await b.request.TestOpenAIResponsesImageInput(test_audio)

    assert request.body.json() == {
        "model": "o1-mini",
        "input": [
            {
                "role": "user",
                "content": [
                    {"type": "input_text", "text": "what is in this content?"},
                    {
                        "type": "input_audio",
                        "input_audio": {
                            "data": test_audio_data,
                            "format": "wav",
                        },
                    },
                ],
            }
        ],
    }


@pytest.mark.asyncio
async def test_expose_request_openai_responses_pdf_base64():
    # Test that base64 PDFs are converted to data URLs
    test_pdf_b64 = "JVBERi0xLjQKMSAwIG9iago8PC9UeXBlIC9DYXRhbG9nCi9QYWdlcyAyIDAgUgo+PgplbmRvYmoKMiAwIG9iago8PC9UeXBlIC9QYWdlcwovS2lkcyBbMyAwIFJdCi9Db3VudCAxCj4+CmVuZG9iagozIDAgb2JqCjw8L1R5cGUgL1BhZ2UKL1BhcmVudCAyIDAgUgovTWVkaWFCb3ggWzAgMCA1OTUgODQyXQovQ29udGVudHMgNSAwIFIKL1Jlc291cmNlcyA8PC9Qcm9jU2V0IFsvUERGIC9UZXh0XQovRm9udCA8PC9GMSA0IDAgUj4+Cj4+Cj4+CmVuZG9iago0IDAgb2JqCjw8L1R5cGUgL0ZvbnQKL1N1YnR5cGUgL1R5cGUxCi9OYW1lIC9GMQovQmFzZUZvbnQgL0hlbHZldGljYQovRW5jb2RpbmcgL01hY1JvbWFuRW5jb2RpbmcKPj4KZW5kb2JqCjUgMCBvYmoKPDwvTGVuZ3RoIDUzCj4+CnN0cmVhbQpCVAovRjEgMjAgVGYKMjIwIDQwMCBUZAooRHVtbXkgUERGKSBUagpFVAplbmRzdHJlYW0KZW5kb2JqCnhyZWYKMCA2CjAwMDAwMDAwMDAgNjU1MzUgZgowMDAwMDAwMDA5IDAwMDAwIG4KMDAwMDAwMDA2MyAwMDAwMCBuCjAwMDAwMDAxMjQgMDAwMDAgbgowMDAwMDAwMjc3IDAwMDAwIG4KMDAwMDAwMDM5MiAwMDAwMCBuCnRyYWlsZXIKPDwvU2l6ZSA2Ci9Sb290IDEgMCBSCj4+CnN0YXJ0eHJlZgo0OTUKJSVFT0YK"
    test_pdf = Pdf.from_base64(test_pdf_b64)
    request = await b.request.TestOpenAIResponsesImageInput(test_pdf)

    assert request.body.json() == {
        "model": "o1-mini",
        "input": [
            {
                "role": "user",
                "content": [
                    {"type": "input_text", "text": "what is in this content?"},
                    {
                        "type": "input_file",
                        "file_url": f"data:application/pdf;base64,{test_pdf_b64}",
                    },
                ],
            }
        ],
    }


@pytest.mark.asyncio
async def test_expose_request_openai_responses_pdf_url():
    # Test that PDF URLs are preserved as URLs (OpenAI Responses API supports file_url with URLs)
    test_pdf = Pdf.from_url(
        "https://www.usenix.org/system/files/conference/nsdi13/nsdi13-final85.pdf"
    )
    request = await b.request.TestOpenAIResponsesImageInput(test_pdf)

    assert request.body.json() == {
        "model": "o1-mini",
        "input": [
            {
                "role": "user",
                "content": [
                    {"type": "input_text", "text": "what is in this content?"},
                    {
                        "type": "input_file",
                        "file_url": "https://www.usenix.org/system/files/conference/nsdi13/nsdi13-final85.pdf",
                    },
                ],
            }
        ],
    }
