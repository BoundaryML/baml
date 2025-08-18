# pyright: ignore-all
import pytest

from baml_py import Image, Audio, Pdf, Video


@pytest.mark.parametrize(
    "cls,url,mime",
    [
        (Image, "https://example.com/test.png", "image/png"),
        (Audio, "https://example.com/test.mp3", "audio/mpeg"),
        (Pdf, "https://example.com/test.pdf", "application/pdf"),
        (Video, "https://example.com/test.mp4", "video/mp4"),
    ],
)
def test_media_url_roundtrip(cls, url, mime):
    """Ensure `from_url` constructs objects that report URL correctly."""
    obj = cls.from_url(url)
    assert obj.is_url()
    assert obj.as_url() == url


@pytest.mark.parametrize(
    "cls,mime,base64_stub",
    [
        (Image, "image/png", "AAA"),
        (Audio, "audio/mpeg", "BBB"),
        (Pdf, "application/pdf", "CCC"),
        (Video, "video/mp4", "DDD"),
    ],
)
def test_media_base64_roundtrip(cls, mime, base64_stub):
    """Ensure `from_base64` stores mime-type and base64 string, and `as_base64` returns them."""
    obj = cls.from_base64(mime, base64_stub)
    assert not obj.is_url()
    base64_out, mime_out = obj.as_base64()
    assert base64_out == base64_stub
    assert mime_out == mime  # mime should be preserved
