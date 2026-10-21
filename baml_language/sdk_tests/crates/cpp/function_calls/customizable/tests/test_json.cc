// Host-supplied json must materialize with `json` container typing.
// Port of go/function_calls/customizable/test_json_test.go (the typed
// narrowing subset). Inbound json values from the C++ bridge carry no
// element-type annotation on the wire; the engine must re-annotate them
// with the `baml.json.json` alias so typed narrowing inside BAML --
// `match (j) { let m: map<string, json> => ... }`, and therefore
// `baml.json.path` / `path_or` -- treats them exactly like BAML-born
// `baml.json.parse` values.
#include <baml_sdk.h>
#include <baml_test.h>

#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

using baml_sdk::baml::json::json;
using baml_sdk::baml::json::PathError;

namespace {

using JsonMap = std::unordered_map<std::string, ::baml::box<json>>;
using JsonList = std::vector<::baml::box<json>>;

json jstr(const char* s) { return json{std::string(s)}; }

// {"type": "ok", "nested": {"list": [1, {"deep": "found"}]}}
json narrowing_fixture() {
  json deep = json{JsonMap{{"deep", ::baml::box<json>(jstr("found"))}}};
  json list = json{JsonList{::baml::box<json>(json{int64_t{1}}),
                            ::baml::box<json>(std::move(deep))}};
  json nested = json{JsonMap{{"list", ::baml::box<json>(std::move(list))}}};
  return json{JsonMap{{"type", ::baml::box<json>(jstr("ok"))},
                      {"nested", ::baml::box<json>(std::move(nested))}}};
}

}  // namespace

BAML_TEST(host_supplied_json_supports_typed_narrowing) {
  const json object = narrowing_fixture();

  BAML_ASSERT_EQ(baml_sdk::go_json_tests::json_kind(object),
                 std::string("object"));
  BAML_ASSERT_EQ(baml_sdk::go_json_tests::json_kind(
                     json{JsonList{::baml::box<json>(json{int64_t{1}})}}),
                 std::string("array"));
  BAML_ASSERT_EQ(baml_sdk::go_json_tests::json_kind(jstr("text")),
                 std::string("string"));
  BAML_ASSERT_EQ(baml_sdk::go_json_tests::json_kind(json{int64_t{3}}),
                 std::string("other"));

  BAML_ASSERT_EQ(baml_sdk::go_json_tests::json_path_string(object, ".type"),
                 std::string("ok"));
  BAML_ASSERT_EQ(
      baml_sdk::go_json_tests::json_path_string(object, ".nested.list[1].deep"),
      std::string("found"));
  BAML_ASSERT_EQ(baml_sdk::go_json_tests::json_path_string_or(
                     object, ".missing", "fallback"),
                 std::string("fallback"));

  bool threw = false;
  try {
    baml_sdk::go_json_tests::json_path_string(object, ".absent");
    baml_test::fail("json_path_string(.absent) did not throw");
  } catch (const baml::thrown<baml::variant<PathError>>& e) {
    threw = true;
    const PathError decoded = e.get<PathError>();
    BAML_ASSERT(decoded.message.find("missing field") != std::string::npos);
    BAML_ASSERT_EQ(decoded.selector, std::string(".absent"));
  }
  BAML_ASSERT(threw);
}

BAML_TEST(json_returned_from_host_callback_supports_typed_narrowing) {
  // json returned from a host callback converts on the host-return path
  // (no argument coercion pass); it must narrow identically.
  const std::string got = baml_sdk::go_json_tests::json_callback_kind(
      [](json value) {
        return json{JsonMap{{"wrapped", ::baml::box<json>(std::move(value))}}};
      },
      jstr("payload"));
  BAML_ASSERT_EQ(got, std::string("object"));
}
