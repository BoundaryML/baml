// Roundtrip coverage for the media suite.
// Port of type_shapes/customizable/roundtrip_tests/test_media.py. Media
// values can't be hand-built as plain structs with meaningful content, so
// each value is sourced from the matching return_* function (which builds
// it engine-side via image.from_url(...) etc.); the decode path yields a
// baml.media.* class wrapping a _data handle, and the encode path passes
// that value back into a round_trip_* function.
#include <baml_sdk.h>
#include <baml_test.h>

#include <optional>
#include <string>

namespace media_ns = baml_sdk::media;
namespace baml_media = baml_sdk::baml::media;

static const char kUrl[] = "https://example.com/asset";

// --- decode path (return_*) works -----------------------------------------

BAML_TEST(return_image) {
  const baml_media::Image img = media_ns::return_image(kUrl, std::nullopt);
  BAML_ASSERT(!img._data.empty());
  BAML_ASSERT(img.url() == std::optional<std::string>(kUrl));
}

BAML_TEST(return_audio) {
  const baml_media::Audio aud = media_ns::return_audio(kUrl, std::nullopt);
  BAML_ASSERT(!aud._data.empty());
}

BAML_TEST(return_video) {
  const baml_media::Video vid = media_ns::return_video(kUrl, std::nullopt);
  BAML_ASSERT(!vid._data.empty());
}

BAML_TEST(return_pdf) {
  const baml_media::Pdf pdf = media_ns::return_pdf(kUrl, std::nullopt);
  BAML_ASSERT(!pdf._data.empty());
}

// --- encode path (round_trip_*) --------------------------------------------

BAML_TEST(round_trip_image) {
  const baml_media::Image img = media_ns::return_image(kUrl, std::nullopt);
  const baml_media::Image back = media_ns::round_trip_image(img);
  BAML_ASSERT(!back._data.empty());
  BAML_ASSERT(back.url() == std::optional<std::string>(kUrl));
}

BAML_TEST(round_trip_audio) {
  const baml_media::Audio aud = media_ns::return_audio(kUrl, std::nullopt);
  BAML_ASSERT(!media_ns::round_trip_audio(aud)._data.empty());
}

BAML_TEST(round_trip_video) {
  const baml_media::Video vid = media_ns::return_video(kUrl, std::nullopt);
  BAML_ASSERT(!media_ns::round_trip_video(vid)._data.empty());
}

BAML_TEST(round_trip_pdf) {
  const baml_media::Pdf pdf = media_ns::return_pdf(kUrl, std::nullopt);
  BAML_ASSERT(!media_ns::round_trip_pdf(pdf)._data.empty());
}

BAML_TEST(round_trip_media) {
  const media_ns::Media m{
      media_ns::return_image(kUrl, std::nullopt),
      media_ns::return_audio(kUrl, std::nullopt),
      media_ns::return_video(kUrl, std::nullopt),
      media_ns::return_pdf(kUrl, std::nullopt),
  };
  const media_ns::Media back = media_ns::round_trip_media(m);
  BAML_ASSERT(!back.image_field._data.empty());
  BAML_ASSERT(!back.audio_field._data.empty());
  BAML_ASSERT(!back.video_field._data.empty());
  BAML_ASSERT(!back.pdf_field._data.empty());
}
