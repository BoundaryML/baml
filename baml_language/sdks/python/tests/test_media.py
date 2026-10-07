"""Media type tests for BamlImage, BamlAudio, BamlVideo, BamlPdf.

Tests the PyO3 media constructors (from_url, from_file, from_base64)
and accessors (url(), name(), base64(), mime_type()). A media value built
from a file holds the file's content, read when it is built, and its base
name; never its path.
"""

import pytest

from baml_bridge import BamlError, BamlRuntime
from baml_bridge.baml_py import BamlImage, BamlAudio, BamlVideo, BamlPdf


@pytest.fixture(scope="module", autouse=True)
def runtime():
    """`from_file` is BAML's `from_file`: it needs a runtime, with any program."""
    return BamlRuntime.initialize_runtime(
        ".", {"main.baml": "function media_test_program() -> int { 1 }"}
    )


# ---------------------------------------------------------------------------
# BamlImage
# ---------------------------------------------------------------------------


class TestBamlImage:
    def test_from_url(self):
        img = BamlImage.from_url("https://example.com/cat.png")
        assert img.url() == "https://example.com/cat.png"
        assert img.name() is None
        assert img.mime_type() is None

    def test_from_url_with_mime(self):
        img = BamlImage.from_url("https://example.com/cat.png", mime_type="image/png")
        assert img.url() == "https://example.com/cat.png"
        assert img.mime_type() == "image/png"

    def test_from_file(self, tmp_path):
        path = tmp_path / "cat.png"
        path.write_bytes(b"hello")
        img = BamlImage.from_file(str(path))
        # The file can go: the value holds its content.
        path.unlink()
        assert img.base64() == "aGVsbG8="
        assert img.name() == "cat.png"
        assert img.mime_type() == "image/png"
        assert img.url() is None

    def test_from_file_with_mime(self, tmp_path):
        path = tmp_path / "cat.png"
        path.write_bytes(b"hello")
        img = BamlImage.from_file(str(path), mime_type="image/x-custom")
        assert img.mime_type() == "image/x-custom"

    def test_from_missing_file_raises_when_built(self, tmp_path):
        # The error BAML code would get. Its class, `baml.errors.Io`, is only
        # known with a generated SDK: the SDK test crates assert it.
        with pytest.raises(BamlError):
            BamlImage.from_file(str(tmp_path / "missing.png"))

    def test_from_base64(self):
        img = BamlImage.from_base64("aGVsbG8=")
        assert img.base64() == "aGVsbG8="
        assert img.url() is None
        assert img.name() is None

    def test_from_file_content_is_named_and_reads_nothing(self):
        img = BamlImage.from_file_content("/nowhere/cat.jpg", "aGVsbG8=")
        assert img.name() == "cat.jpg"
        assert img.mime_type() == "image/jpeg"
        assert img.base64() == "aGVsbG8="

    def test_from_base64_with_mime(self):
        img = BamlImage.from_base64("aGVsbG8=", mime_type="image/jpeg")
        assert img.base64() == "aGVsbG8="
        assert img.mime_type() == "image/jpeg"


# ---------------------------------------------------------------------------
# BamlAudio
# ---------------------------------------------------------------------------


class TestBamlAudio:
    def test_from_url(self):
        audio = BamlAudio.from_url("https://example.com/song.mp3")
        assert audio.url() == "https://example.com/song.mp3"
        assert audio.name() is None

    def test_from_file(self, tmp_path):
        path = tmp_path / "song.mp3"
        path.write_bytes(b"audio")
        audio = BamlAudio.from_file(str(path))
        assert audio.base64() == "YXVkaW8="
        assert audio.name() == "song.mp3"
        assert audio.mime_type() == "audio/mpeg"
        assert audio.url() is None

    def test_from_base64(self):
        audio = BamlAudio.from_base64("YXVkaW8=", mime_type="audio/mpeg")
        assert audio.base64() == "YXVkaW8="
        assert audio.mime_type() == "audio/mpeg"


# ---------------------------------------------------------------------------
# BamlVideo
# ---------------------------------------------------------------------------


class TestBamlVideo:
    def test_from_url(self):
        video = BamlVideo.from_url("https://example.com/clip.mp4")
        assert video.url() == "https://example.com/clip.mp4"

    def test_from_file(self, tmp_path):
        path = tmp_path / "clip.mp4"
        path.write_bytes(b"video")
        video = BamlVideo.from_file(str(path))
        assert video.name() == "clip.mp4"
        assert video.mime_type() == "video/mp4"

    def test_from_base64(self):
        video = BamlVideo.from_base64("dmlkZW8=", mime_type="video/mp4")
        assert video.base64() == "dmlkZW8="
        assert video.mime_type() == "video/mp4"


# ---------------------------------------------------------------------------
# BamlPdf
# ---------------------------------------------------------------------------


class TestBamlPdf:
    def test_from_url(self):
        pdf = BamlPdf.from_url("https://example.com/doc.pdf")
        assert pdf.url() == "https://example.com/doc.pdf"
        assert pdf.name() is None

    def test_from_url_with_mime(self):
        pdf = BamlPdf.from_url("https://example.com/doc.pdf", mime_type="application/pdf")
        assert pdf.mime_type() == "application/pdf"

    def test_from_file(self, tmp_path):
        path = tmp_path / "doc.pdf"
        path.write_bytes(b"%PDF-1.7")
        pdf = BamlPdf.from_file(str(path))
        assert pdf.base64() == "JVBERi0xLjc="
        assert pdf.name() == "doc.pdf"
        assert pdf.mime_type() == "application/pdf"
        assert pdf.url() is None

    def test_from_base64(self):
        pdf = BamlPdf.from_base64("cGRm", mime_type="application/pdf")
        assert pdf.base64() == "cGRm"
        assert pdf.mime_type() == "application/pdf"
