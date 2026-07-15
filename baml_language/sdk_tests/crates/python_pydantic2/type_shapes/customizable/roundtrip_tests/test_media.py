"""Exact bridge coverage for all media kinds and source forms.

The URL and base64 return functions build values in BAML, exercising the
BAML-to-Python decoder. The round-trip functions receive Python-created media,
exercising Python-to-BAML encoding before decoding the same value back. The
aggregate case additionally verifies file-backed media nested in a class.
"""

import base64
from typing import Any

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.baml.media import Audio, Image, Pdf, Video
from baml_sdk.media import (
    Media,
    return_audio,
    return_audio_base64,
    return_image,
    return_image_base64,
    return_pdf,
    return_pdf_base64,
    return_video,
    return_video_base64,
    round_trip_audio,
    round_trip_image,
    round_trip_pdf,
    round_trip_video,
    round_trip_media,
)

URL = "https://example.com/asset"
IMAGE_MIME = "image/png"
AUDIO_MIME = "audio/mpeg"
VIDEO_MIME = "video/mp4"
PDF_MIME = "application/pdf"

IMAGE_PAYLOAD = b"image-payload"
AUDIO_PAYLOAD = b"audio-payload"
VIDEO_PAYLOAD = b"video-payload"
PDF_PAYLOAD = b"pdf-payload"


def _encoded(payload: bytes) -> str:
    return base64.b64encode(payload).decode("ascii")


def _assert_url_media(
    value: Any, expected_type: type[Any], url: str, mime: str
) -> None:
    assert isinstance(value, expected_type)
    assert value.url() == url
    assert value.file() is None
    assert value.base64() == ""
    assert value.mime_type() == mime


def _assert_base64_media(
    value: Any, expected_type: type[Any], payload: bytes, mime: str
) -> None:
    encoded = _encoded(payload)
    assert isinstance(value, expected_type)
    assert value.url() is None
    assert value.file() is None
    assert value.base64() == encoded
    assert base64.b64decode(value.base64(), validate=True) == payload
    assert value.mime_type() == mime


def _assert_file_media(
    value: Any, expected_type: type[Any], file: str, mime: str
) -> None:
    assert isinstance(value, expected_type)
    assert value.url() is None
    assert value.file() == file
    assert value.base64() == ""
    assert value.mime_type() == mime


# --- decode path (return_*) works -----------------------------------------


def test_return_image():
    value = return_image(url=URL, mime=IMAGE_MIME)
    _assert_url_media(value, Image, URL, IMAGE_MIME)


def test_return_audio():
    value = return_audio(url=URL, mime=AUDIO_MIME)
    _assert_url_media(value, Audio, URL, AUDIO_MIME)


def test_return_video():
    value = return_video(url=URL, mime=VIDEO_MIME)
    _assert_url_media(value, Video, URL, VIDEO_MIME)


def test_return_pdf():
    value = return_pdf(url=URL, mime=PDF_MIME)
    _assert_url_media(value, Pdf, URL, PDF_MIME)


def test_return_image_base64():
    value = return_image_base64(base64=_encoded(IMAGE_PAYLOAD), mime=IMAGE_MIME)
    _assert_base64_media(value, Image, IMAGE_PAYLOAD, IMAGE_MIME)


def test_return_audio_base64():
    value = return_audio_base64(base64=_encoded(AUDIO_PAYLOAD), mime=AUDIO_MIME)
    _assert_base64_media(value, Audio, AUDIO_PAYLOAD, AUDIO_MIME)


def test_return_video_base64():
    value = return_video_base64(base64=_encoded(VIDEO_PAYLOAD), mime=VIDEO_MIME)
    _assert_base64_media(value, Video, VIDEO_PAYLOAD, VIDEO_MIME)


def test_return_pdf_base64():
    value = return_pdf_base64(base64=_encoded(PDF_PAYLOAD), mime=PDF_MIME)
    _assert_base64_media(value, Pdf, PDF_PAYLOAD, PDF_MIME)


# --- encode path (round_trip_*) ------------------------------------------


def test_round_trip_image():
    value = Image.from_url(URL, IMAGE_MIME)
    result = round_trip_image(x=value)
    _assert_url_media(result, Image, URL, IMAGE_MIME)


def test_round_trip_audio():
    value = Audio.from_url(URL, AUDIO_MIME)
    result = round_trip_audio(x=value)
    _assert_url_media(result, Audio, URL, AUDIO_MIME)


def test_round_trip_video():
    value = Video.from_url(URL, VIDEO_MIME)
    result = round_trip_video(x=value)
    _assert_url_media(result, Video, URL, VIDEO_MIME)


def test_round_trip_pdf():
    value = Pdf.from_url(URL, PDF_MIME)
    result = round_trip_pdf(x=value)
    _assert_url_media(result, Pdf, URL, PDF_MIME)


def test_round_trip_image_base64():
    value = Image.from_base64(_encoded(IMAGE_PAYLOAD), IMAGE_MIME)
    result = round_trip_image(x=value)
    _assert_base64_media(result, Image, IMAGE_PAYLOAD, IMAGE_MIME)


def test_round_trip_audio_base64():
    value = Audio.from_base64(_encoded(AUDIO_PAYLOAD), AUDIO_MIME)
    result = round_trip_audio(x=value)
    _assert_base64_media(result, Audio, AUDIO_PAYLOAD, AUDIO_MIME)


def test_round_trip_video_base64():
    value = Video.from_base64(_encoded(VIDEO_PAYLOAD), VIDEO_MIME)
    result = round_trip_video(x=value)
    _assert_base64_media(result, Video, VIDEO_PAYLOAD, VIDEO_MIME)


def test_round_trip_pdf_base64():
    value = Pdf.from_base64(_encoded(PDF_PAYLOAD), PDF_MIME)
    result = round_trip_pdf(x=value)
    _assert_base64_media(result, Pdf, PDF_PAYLOAD, PDF_MIME)


def test_round_trip_media():
    m = Media(
        image_field=Image.from_file("/tmp/asset.png", IMAGE_MIME),
        audio_field=Audio.from_file("/tmp/asset.mp3", AUDIO_MIME),
        video_field=Video.from_file("/tmp/asset.mp4", VIDEO_MIME),
        pdf_field=Pdf.from_file("/tmp/asset.pdf", PDF_MIME),
    )
    result = round_trip_media(m=m)

    assert isinstance(result, Media)
    _assert_file_media(result.image_field, Image, "/tmp/asset.png", IMAGE_MIME)
    _assert_file_media(result.audio_field, Audio, "/tmp/asset.mp3", AUDIO_MIME)
    _assert_file_media(result.video_field, Video, "/tmp/asset.mp4", VIDEO_MIME)
    _assert_file_media(result.pdf_field, Pdf, "/tmp/asset.pdf", PDF_MIME)
