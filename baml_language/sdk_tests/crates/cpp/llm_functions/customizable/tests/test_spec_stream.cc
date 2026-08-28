// LLM API shape: authored call, bound spec, and the flat sync/async stream
// shortcuts. The `$stream` spelling remains only in BAML type identity; C++
// projects those PPIR schemas under stream_types.
#include <baml_sdk.h>
#include <baml_test.h>

#include <fstream>
#include <iterator>
#include <optional>
#include <string>
#include <type_traits>
#include <utility>

namespace {

using string_spec = decltype(baml_sdk::lorem::stream_e2e_extract_spec(
    std::declval<const std::string&>()));
using expected_string_spec = baml::function_spec<std::string>;
static_assert(std::is_same<string_spec, expected_string_spec>::value,
              "Fn_spec must retain the final output type");

using string_stream = decltype(baml_sdk::lorem::stream_e2e_extract_stream(
    std::declval<const std::string&>()));
using expected_string_stream =
    baml::stream<std::optional<std::string>, std::string>;
static_assert(std::is_same<string_stream, expected_string_stream>::value,
              "Fn_stream must expose the PPIR partial type");

using async_string_spec =
    decltype(baml_sdk::lorem::stream_e2e_extract_spec_async(
        std::declval<const std::string&>()));
static_assert(
    std::is_same<async_string_spec, baml::future<expected_string_spec>>::value,
    "the bound spec itself remains available asynchronously");

using async_string_stream =
    decltype(baml_sdk::lorem::stream_e2e_extract_stream_async(
        std::declval<const std::string&>()));
static_assert(
    std::is_same<async_string_stream,
                 baml::future<expected_string_stream>>::value,
    "Fn_stream must remain available asynchronously");

using rendered_prompt = decltype(std::declval<const string_spec&>().prompt());
static_assert(std::is_same<rendered_prompt, baml::prompt>::value,
              "FunctionSpec.prompt must return portable prompt data");

using media_spec = decltype(baml_sdk::lorem::InspectMedia_spec(
    std::declval<const baml::image&>()));
static_assert(std::is_same<decltype(std::declval<const media_spec&>().prompt()),
                           baml::prompt>::value,
              "media-bearing specs must render through the same Prompt API");

using doc_partial = baml_sdk::stream_types::lorem::StreamingDoc;
using doc_final = baml_sdk::lorem::StreamingDoc;
using doc_stream = decltype(baml_sdk::lorem::stream_e2e_extract_doc_stream(
    std::declval<const std::string&>()));
static_assert(
    std::is_same<doc_stream,
                 baml::stream<std::optional<doc_partial>, doc_final>>::value,
    "Out$stream must remain a generated partial schema");

}  // namespace

BAML_TEST(flat_stream_calls_private_projection_through_authored_identity) {
  std::ifstream input("baml_sdk/src/bindings.cc");
  const std::string bindings((std::istreambuf_iterator<char>(input)),
                             std::istreambuf_iterator<char>());
  BAML_ASSERT(bindings.find("\"user.lorem.stream_e2e_extract\"") !=
              std::string::npos);
  BAML_ASSERT(bindings.find("::baml::function_operation::stream") !=
              std::string::npos);
  BAML_ASSERT(bindings.find("return bound_spec.stream();") ==
              std::string::npos);
  BAML_ASSERT(bindings.find("stream_e2e_extract$spec") == std::string::npos);
  BAML_ASSERT(bindings.find("stream_e2e_extract$stream") ==
              std::string::npos);
  BAML_ASSERT(bindings.find("$render_prompt") == std::string::npos);
  BAML_ASSERT(bindings.find("$build_request") == std::string::npos);
  BAML_ASSERT(bindings.find("$parse") == std::string::npos);
}

BAML_TEST_MAIN()
