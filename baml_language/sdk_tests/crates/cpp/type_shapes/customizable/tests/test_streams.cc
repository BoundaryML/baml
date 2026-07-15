// Roundtrip coverage for the lorem stream-type suite.
// Port of type_shapes/customizable/roundtrip_tests/test_streams.py:
// $stream companion types are normally engine-internal partial values;
// these tests probe that a host-constructed stream_types::* struct can be
// encoded and round-tripped through a $stream-typed parameter.
//
#include <baml_sdk.h>
#include <baml_test.h>

#include <optional>
#include <variant>

using baml_sdk::lorem::Resume;
using StreamResume = baml_sdk::stream_types::lorem::Resume;
using StreamFoo = baml_sdk::stream_types::Foo;

BAML_TEST(round_trip_resume_stream) {
  const StreamResume r{std::string("ada"), std::nullopt};
  BAML_ASSERT(baml_sdk::lorem::round_trip_resume_stream(r) == r);
}

BAML_TEST(round_trip_root_foo_stream) {
  const StreamFoo f{3};
  BAML_ASSERT(baml_sdk::lorem::round_trip_root_foo_stream(f) == f);
}

BAML_TEST(round_trip_box_of_resume_stream) {
  const baml_sdk::lorem::Box<StreamResume> b{
      StreamResume{std::string("grace"), std::nullopt}};
  BAML_ASSERT(baml_sdk::lorem::round_trip_box_of_resume_stream(b) == b);
}

BAML_TEST(round_trip_resume_or_resume_stream) {
  // Union arm Resume (the non-stream side) is host-constructible.
  const std::variant<Resume, StreamResume> u = Resume{"hopper", std::nullopt};
  BAML_ASSERT(baml_sdk::lorem::round_trip_resume_or_resume_stream(u) == u);
}

BAML_TEST(round_trip_resume_or_http_response) {
  // Pass the Resume arm; the baml.http.Response arm isn't
  // host-constructible. Alternative order is canonical (sorted rendered
  // types), not BAML declaration order.
  const std::variant<baml_sdk::baml::http::Response, Resume> u =
      Resume{"lovelace", std::string("a@x.com")};
  const auto result = baml_sdk::lorem::round_trip_resume_or_http_response(u);
  BAML_ASSERT(std::holds_alternative<Resume>(result));
  BAML_ASSERT(std::get<Resume>(result) == std::get<Resume>(u));
}
