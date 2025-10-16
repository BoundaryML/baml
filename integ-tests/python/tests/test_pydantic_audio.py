import baml_py
import pydantic


class AudioWrapper(pydantic.BaseModel):
    content: baml_py.Audio


def test_model_accepts_audio_instance_url_and_b64():
    # instance via URL
    url = "https://example.com/test.mp3"
    obj = AudioWrapper(content=baml_py.Audio.from_url(url))
    assert obj.model_dump() == {"content": {"url": url}}

    # instance via base64
    obj2 = AudioWrapper(content=baml_py.Audio.from_base64("audio/mpeg", "BBB"))
    assert obj2.model_dump() == {
        "content": {"base64": "BBB", "media_type": "audio/mpeg"}
    }


def test_model_validate_audio_from_dict():
    obj = AudioWrapper.model_validate({"content": {"url": "https://example.com/test.mp3"}})
    assert isinstance(obj.content, baml_py.Audio)

    obj2 = AudioWrapper.model_validate({"content": {"base64": "BBB", "media_type": "audio/mpeg"}})
    assert isinstance(obj2.content, baml_py.Audio)


