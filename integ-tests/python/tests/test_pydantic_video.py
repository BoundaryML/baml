import base64
import baml_py
import pydantic
from baml_client import b
import pytest


def load_base64_from_file(file_path):
    with open(file_path, "rb") as f:
        return base64.b64encode(f.read()).decode("utf-8")


class VideoWrapper(pydantic.BaseModel):
    content: baml_py.Video


def test_model_accepts_video_instance_url_and_b64():
    # instance via URL
    url = "https://example.com/test.mp4"
    obj = VideoWrapper(content=baml_py.Video.from_url(url))
    assert obj.model_dump() == {"content": {"url": url}}

    # instance via base64
    obj2 = VideoWrapper(content=baml_py.Video.from_base64("video/mp4", "DDD"))
    assert obj2.model_dump() == {
        "content": {"base64": "DDD", "media_type": "video/mp4"}
    }


def test_model_validate_video_from_dict():
    obj = VideoWrapper.model_validate(
        {"content": {"url": "https://example.com/test.mp4"}}
    )
    assert isinstance(obj.content, baml_py.Video)

    obj2 = VideoWrapper.model_validate(
        {"content": {"base64": "DDD", "media_type": "video/mp4"}}
    )
    assert isinstance(obj2.content, baml_py.Video)


@pytest.mark.asyncio
async def test_video_input_gemini():
    res = await b.VideoInputGemini(
        vid=baml_py.Video.from_url("https://www.youtube.com/watch?v=pH41OBv9JqU")
    )
    print(res)


@pytest.mark.asyncio
async def test_video_input_vertex_gemini():
    res = await b.VideoInputVertex(
        vid=baml_py.Video.from_url("https://www.youtube.com/watch?v=pH41OBv9JqU")
    )
    print(res)


@pytest.mark.asyncio
async def test_video_input_vertex_base64():

    base64 = load_base64_from_file("../baml_src/sample-5s.mp4")
    print(f"Base64 first 20 chars: {base64[:20]}")
    print(f"Base64 last 20 chars: {base64[-20:]}")
    print(f"Base64 size in kilobytes: {len(base64.encode('utf-8')) / 1024:.2f}")
    res = await b.VideoInputVertex(vid=baml_py.Video.from_base64("video/mp4", base64))
    print(res)
    # await asyncio.sleep(5)
    print("test ended")


@pytest.fixture(scope="session", autouse=True)
def flush_traces():
    """Ensure traces are flushed when pytest exits."""
    yield
    print("[python] Flushing traces")
    from baml_client.tracing import flush

    print("Flushing traces (after import)")
    flush()
    print("[python]Traces flushed")
