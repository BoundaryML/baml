import baml_py
import pydantic


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
    obj = VideoWrapper.model_validate({"content": {"url": "https://example.com/test.mp4"}})
    assert isinstance(obj.content, baml_py.Video)

    obj2 = VideoWrapper.model_validate({"content": {"base64": "DDD", "media_type": "video/mp4"}})
    assert isinstance(obj2.content, baml_py.Video)


