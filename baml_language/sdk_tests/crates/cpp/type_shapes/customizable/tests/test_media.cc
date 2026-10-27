// Roundtrip coverage for baml_sdk::media under a union.
//
// A union member that is media crosses as a media value, not a class or a
// scalar: it must decode into its own alternative of the baml::variant.
// Port of the union cases in java/type_shapes/customizable/roundtrip_tests/
// TestMedia.java.
#include <baml_sdk.h>
#include <baml_test.h>

#include <optional>
#include <string>
#include <variant>

namespace {

const std::string kUrl = "https://example.com/asset";

using AllMedia =
    baml::variant<baml::audio, baml::image, baml::pdf, baml::video>;

// Sends `value` through the four-way media union and checks it comes back as
// the same kind with its URL.
template <typename Kind>
void expect_all_media_round_trip(const Kind& value) {
  const AllMedia back = baml_sdk::media::round_trip_all_media(AllMedia{value});
  BAML_ASSERT(std::holds_alternative<Kind>(back));
  BAML_ASSERT_EQ(std::get<Kind>(back).url(), std::optional<std::string>(kUrl));
}

}  // namespace

BAML_TEST(media_round_trip_all_media) {
  expect_all_media_round_trip(
      baml_sdk::media::return_image(kUrl, std::nullopt));
  expect_all_media_round_trip(
      baml_sdk::media::return_audio(kUrl, std::nullopt));
  expect_all_media_round_trip(
      baml_sdk::media::return_video(kUrl, std::nullopt));
  expect_all_media_round_trip(baml_sdk::media::return_pdf(kUrl, std::nullopt));
}

BAML_TEST(media_round_trip_media_or_image_class) {
  // `image | Image`: the media type and a user class whose name collides
  // with it stay distinct alternatives.
  using ImageOrClass = baml::variant<baml::image, baml_sdk::media::Image>;

  const ImageOrClass media = baml_sdk::media::round_trip_media_or_image_class(
      ImageOrClass{baml_sdk::media::return_image(kUrl, std::nullopt)});
  BAML_ASSERT(std::holds_alternative<baml::image>(media));
  BAML_ASSERT_EQ(std::get<baml::image>(media).url(),
                 std::optional<std::string>(kUrl));

  const baml_sdk::media::Image labelled{"cover"};
  const ImageOrClass cls =
      baml_sdk::media::round_trip_media_or_image_class(ImageOrClass{labelled});
  BAML_ASSERT(std::holds_alternative<baml_sdk::media::Image>(cls));
  BAML_ASSERT(std::get<baml_sdk::media::Image>(cls) == labelled);
}
