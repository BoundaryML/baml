import pytest
from hamcrest import assert_that, equal_to
from baml_py import Image, Audio, Pdf
from baml_client import b


@pytest.mark.asyncio
async def test_expose_request_openai_responses_multimodal():
    test_image = Image.from_url(
        "https://upload.wikimedia.org/wikipedia/commons/thumb/d/dd/Gfp-wisconsin-madison-the-nature-boardwalk.jpg/2560px-Gfp-wisconsin-madison-the-nature-boardwalk.jpg"
    )
    request = await b.request.TestOpenAIResponsesImageInput(test_image)

    assert_that(
        request.body.json(),
        equal_to(
            {
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
        ),
    )


@pytest.mark.asyncio
async def test_expose_request_openai_responses_audio():
    test_audio_data = "UklGRnoGAABXQVZFZm10IBAAAAABAAEAQB8AAEAfAAABAAgAZGF0YQoGAACBhYqFbF1fdJivrJBhNjVgodDbq2EcBj+a2/LDciUFLIHO8tiJNwgZaLvt559NEAxQp+PwtmMcBjiR1/LMeSwFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoFJHfH8N2QQAoUYrTp66hVFApGn+DyvmEaBC2Bye/OcyoF"
    test_audio = Audio.from_base64("audio/wav", test_audio_data)
    request = await b.request.TestOpenAIResponsesImageInput(test_audio)

    assert_that(
        request.body.json(),
        equal_to(
            {
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
        ),
    )


@pytest.mark.asyncio
async def test_expose_request_openai_responses_pdf_base64():
    # Test that base64 PDFs are converted to data URLs
    test_pdf_b64 = "JVBERi0xLjQKMSAwIG9iago8PC9UeXBlIC9DYXRhbG9nCi9QYWdlcyAyIDAgUgo+PgplbmRvYmoKMiAwIG9iago8PC9UeXBlIC9QYWdlcwovS2lkcyBbMyAwIFJdCi9Db3VudCAxCj4+CmVuZG9iagozIDAgb2JqCjw8L1R5cGUgL1BhZ2UKL1BhcmVudCAyIDAgUgovTWVkaWFCb3ggWzAgMCA1OTUgODQyXQovQ29udGVudHMgNSAwIFIKL1Jlc291cmNlcyA8PC9Qcm9jU2V0IFsvUERGIC9UZXh0XQovRm9udCA8PC9GMSA0IDAgUj4+Cj4+Cj4+CmVuZG9iago0IDAgb2JqCjw8L1R5cGUgL0ZvbnQKL1N1YnR5cGUgL1R5cGUxCi9OYW1lIC9GMQovQmFzZUZvbnQgL0hlbHZldGljYQovRW5jb2RpbmcgL01hY1JvbWFuRW5jb2RpbmcKPj4KZW5kb2JqCjUgMCBvYmoKPDwvTGVuZ3RoIDUzCj4+CnN0cmVhbQpCVAovRjEgMjAgVGYKMjIwIDQwMCBUZAooRHVtbXkgUERGKSBUagpFVAplbmRzdHJlYW0KZW5kb2JqCnhyZWYKMCA2CjAwMDAwMDAwMDAgNjU1MzUgZgowMDAwMDAwMDA5IDAwMDAwIG4KMDAwMDAwMDA2MyAwMDAwMCBuCjAwMDAwMDAxMjQgMDAwMDAgbgowMDAwMDAwMjc3IDAwMDAwIG4KMDAwMDAwMDM5MiAwMDAwMCBuCnRyYWlsZXIKPDwvU2l6ZSA2Ci9Sb290IDEgMCBSCj4+CnN0YXJ0eHJlZgo0OTUKJSVFT0YK"
    test_pdf = Pdf.from_base64(test_pdf_b64)
    request = await b.request.TestOpenAIResponsesImageInput(test_pdf)

    assert_that(
        request.body.json(),
        equal_to(
            {
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
        ),
    )


@pytest.mark.asyncio
async def test_openai_responses_basic():
    request = await b.request.TestOpenAIResponses("lorem ipsum")

    assert_that(
        request.body.json(),
        equal_to(
            {
                "model": "gpt-4.1",
                "input": [
                    {
                        "role": "user",
                        "content": [
                            {
                                "type": "input_text",
                                "text": "Write a short haiku about lorem ipsum. Make it simple and beautiful.",
                            },
                        ],
                    },
                ],
            }
        ),
    )


@pytest.mark.asyncio
async def test_openai_responses_explicit():
    request = await b.request.TestOpenAIResponsesExplicit("lorem ipsum")

    assert_that(
        request.body.json(),
        equal_to(
            {
                "model": "gpt-4.1",
                "input": [
                    {
                        "role": "user",
                        "content": [
                            {
                                "type": "input_text",
                                "text": "Create a brief poem about lorem ipsum. Keep it under 50 words.",
                            },
                        ],
                    },
                ],
            }
        ),
    )


@pytest.mark.asyncio
async def test_openai_responses_custom_url():
    request = await b.request.TestOpenAIResponsesCustomURL("lorem ipsum")

    assert_that(
        request.body.json(),
        equal_to(
            {
                "model": "gpt-4.1",
                "input": [
                    {
                        "role": "user",
                        "content": [
                            {
                                "type": "input_text",
                                "text": "Tell me an interesting fact about lorem ipsum.",
                            },
                        ],
                    },
                ],
            }
        ),
    )


@pytest.mark.asyncio
async def test_openai_responses_conversation():
    request = await b.request.TestOpenAIResponsesConversation("lorem ipsum")

    assert_that(
        request.body.json(),
        equal_to(
            {
                "model": "gpt-4.1",
                "input": [
                    {
                        "role": "system",
                        "content": [
                            {
                                "type": "input_text",
                                "text": "You are a helpful assistant that provides concise answers.",
                            },
                        ],
                    },
                    {
                        "role": "user",
                        "content": [
                            {
                                "type": "input_text",
                                "text": "What is lorem ipsum?",
                            },
                        ],
                    },
                    {
                        "role": "assistant",
                        "content": [
                            {
                                "type": "output_text",
                                "text": "lorem ipsum is a fascinating subject. Let me explain briefly.",
                            },
                        ],
                    },
                    {
                        "role": "user",
                        "content": [
                            {
                                "type": "input_text",
                                "text": "Can you give me a simple example?",
                            },
                        ],
                    },
                ],
            }
        ),
    )


@pytest.mark.asyncio
async def test_openai_responses_different_model():
    request = await b.request.TestOpenAIResponsesDifferentModel("lorem ipsum")

    assert_that(
        request.body.json(),
        equal_to(
            {
                "model": "gpt-4",
                "input": [
                    {
                        "role": "user",
                        "content": [
                            {
                                "type": "input_text",
                                "text": "Explain lorem ipsum in one sentence.",
                            },
                        ],
                    },
                ],
            }
        ),
    )


@pytest.mark.asyncio
async def test_expose_request_openai_responses_pdf_url():
    # Test that PDF URLs are preserved as URLs (OpenAI Responses API supports file_url with URLs)
    test_pdf = Pdf.from_url(
        "https://www.usenix.org/system/files/conference/nsdi13/nsdi13-final85.pdf"
    )
    request = await b.request.TestOpenAIResponsesImageInput(test_pdf)

    assert_that(
        request.body.json(),
        equal_to(
            {
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
        ),
    )
