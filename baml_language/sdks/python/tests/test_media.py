"""Media type tests for BamlImage, BamlAudio, BamlVideo, BamlPdf.

Tests the PyO3 media constructors (from_url, from_file, from_base64)
and accessors (url(), file(), base64(), mime_type()).
"""

from baml_bridge.baml_py import BamlImage, BamlAudio, BamlVideo, BamlPdf


# ---------------------------------------------------------------------------
# BamlImage
# ---------------------------------------------------------------------------


class TestBamlImage:
    def test_from_url(self):
        img = BamlImage.from_url("https://example.com/cat.png")
        assert img.url() == "https://example.com/cat.png"
        assert img.file() is None
        assert img.mime_type() is None

    def test_from_url_with_mime(self):
        img = BamlImage.from_url("https://example.com/cat.png", mime_type="image/png")
        assert img.url() == "https://example.com/cat.png"
        assert img.mime_type() == "image/png"

    def test_from_file(self):
        img = BamlImage.from_file("/tmp/cat.png")
        assert img.file() == "/tmp/cat.png"
        assert img.url() is None

    def test_from_file_with_mime(self):
        img = BamlImage.from_file("/tmp/cat.png", mime_type="image/png")
        assert img.file() == "/tmp/cat.png"
        assert img.mime_type() == "image/png"

    def test_from_base64(self):
        img = BamlImage.from_base64("aGVsbG8=")
        assert img.base64() == "aGVsbG8="
        assert img.url() is None
        assert img.file() is None

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
        assert audio.file() is None

    def test_from_file(self):
        audio = BamlAudio.from_file("/tmp/song.mp3")
        assert audio.file() == "/tmp/song.mp3"
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

    def test_from_file(self):
        video = BamlVideo.from_file("/tmp/clip.mp4")
        assert video.file() == "/tmp/clip.mp4"

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
        assert pdf.file() is None

    def test_from_url_with_mime(self):
        pdf = BamlPdf.from_url("https://example.com/doc.pdf", mime_type="application/pdf")
        assert pdf.mime_type() == "application/pdf"

    def test_from_file(self):
        pdf = BamlPdf.from_file("/tmp/doc.pdf")
        assert pdf.file() == "/tmp/doc.pdf"
        assert pdf.url() is None

    def test_from_base64(self):
        pdf = BamlPdf.from_base64("cGRm", mime_type="application/pdf")
        assert pdf.base64() == "cGRm"
        assert pdf.mime_type() == "application/pdf"
