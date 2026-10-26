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

#include <cstddef>
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

// `1` inside `levels` single-item lists.
json nested_json_list(std::size_t levels) {
  json value{int64_t{1}};
  for (std::size_t i = 0; i < levels; ++i) {
    JsonList list;
    list.emplace_back(std::move(value));
    value = json{std::move(list)};
  }
  return value;
}

// `1` inside `levels` single-field objects.
json nested_json_object(std::size_t levels) {
  json value{int64_t{1}};
  for (std::size_t i = 0; i < levels; ++i) {
    JsonMap map;
    map.emplace("k", std::move(value));
    value = json{std::move(map)};
  }
  return value;
}

// Runs `call`, which must throw the SDK's refusal of an argument nested past
// what the wire carries: an ordinary baml::error naming the wire, raised
// before anything is sent, never a panic and never by exhausting the stack.
template <typename Call>
void expect_argument_refusal(const Call& call) {
  try {
    call();
  } catch (const baml::panic& e) {
    baml_test::fail(
        std::string("expected the argument refusal, got a panic: ") + e.what());
  } catch (const baml::error& e) {
    if (e.message().find("deeper than the 100 nested messages the wire") ==
        std::string::npos) {
      baml_test::fail(std::string("expected the argument refusal, got: ") +
                      e.what());
    }
    return;
  }
  baml_test::fail("a value nested past the wire limit crossed");
}

// Runs `call`, whose callback returns a value nested past what the wire
// carries: the bridge cannot carry it, so the call fails with the panic BAML
// raises for a bridge failure, `baml.panics.SdkPanic` -- never a host
// contract violation and never a catchable host-callable error.
template <typename Call>
void expect_bridge_failure(const Call& call) {
  try {
    call();
  } catch (const baml::panic& e) {
    if (e.class_name() != "baml.panics.SdkPanic" ||
        e.message().find("deeper than the 100 nested messages the wire") ==
            std::string::npos) {
      baml_test::fail(std::string("expected SdkPanic, got ") + e.class_name() +
                      ": " + e.what());
    }
    return;
  } catch (const baml::error& e) {
    baml_test::fail(std::string("expected SdkPanic, got an error: ") +
                    e.class_name() + ": " + e.what());
  }
  baml_test::fail("a callback result nested past the wire limit crossed");
}

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

BAML_TEST(json_nested_deeper_than_the_wire_carries_is_refused_as_an_argument) {
  // Well inside the bound, the value crosses both ways.
  BAML_ASSERT(baml_sdk::go_json_tests::round_trip_json(nested_json_list(30)) ==
              nested_json_list(30));
  // Just past it, and far past it: refused before anything is sent, with the
  // error an argument that cannot be encoded raises, never by exhausting the
  // stack.
  for (const std::size_t levels : {std::size_t{60}, std::size_t{1000}}) {
    expect_argument_refusal([levels] {
      baml_sdk::go_json_tests::round_trip_json(nested_json_list(levels));
    });
    expect_argument_refusal([levels] {
      baml_sdk::go_json_tests::round_trip_json(nested_json_object(levels));
    });
  }
}

BAML_TEST(
    json_nested_deeper_than_the_wire_carries_is_refused_as_a_callback_result) {
  // Well inside the bound, the callback's result crosses.
  BAML_ASSERT(baml_sdk::go_json_tests::call_json_callback(
                  [](json) { return nested_json_list(30); },
                  json{int64_t{1}}) == nested_json_list(30));
  // Past it, the bridge cannot carry the result: it is refused before it is
  // sent, and the call raises the engine's panic for a bridge failure.
  for (const std::size_t levels : {std::size_t{60}, std::size_t{200}}) {
    expect_bridge_failure([levels] {
      baml_sdk::go_json_tests::call_json_callback(
          [levels](json) { return nested_json_list(levels); },
          json{int64_t{1}});
    });
    expect_bridge_failure([levels] {
      baml_sdk::go_json_tests::call_json_callback(
          [levels](json) { return nested_json_object(levels); },
          json{int64_t{1}});
    });
  }
}

// SDK_PARITY_LINT(skip): pins the C++ encoder's exact wire measure at its cut.
BAML_TEST(json_nested_to_the_wire_limit_crosses_and_one_level_more_does_not) {
  // The wire carries 100 nested messages. A stated json list level costs two
  // (the list and its item), a map level three (the map, its entry, the
  // value), and the outermost statement two more; an argument sits two
  // messages below the call, a callback's result one below the completion.
  // So 48 list levels or 32 map levels cross either way, and one more is
  // refused, as an argument and as a result alike.
  BAML_ASSERT_EQ(baml_sdk::go_json_tests::json_kind(nested_json_list(48)),
                 std::string("array"));
  BAML_ASSERT_EQ(baml_sdk::go_json_tests::json_kind(nested_json_object(32)),
                 std::string("object"));
  expect_argument_refusal(
      [] { baml_sdk::go_json_tests::json_kind(nested_json_list(49)); });
  expect_argument_refusal(
      [] { baml_sdk::go_json_tests::json_kind(nested_json_object(33)); });

  BAML_ASSERT_EQ(
      baml_sdk::go_json_tests::json_callback_kind(
          [](json) { return nested_json_list(48); }, json{int64_t{1}}),
      std::string("array"));
  BAML_ASSERT_EQ(
      baml_sdk::go_json_tests::json_callback_kind(
          [](json) { return nested_json_object(32); }, json{int64_t{1}}),
      std::string("object"));
  expect_bridge_failure([] {
    baml_sdk::go_json_tests::json_callback_kind(
        [](json) { return nested_json_list(49); }, json{int64_t{1}});
  });
  expect_bridge_failure([] {
    baml_sdk::go_json_tests::json_callback_kind(
        [](json) { return nested_json_object(33); }, json{int64_t{1}});
  });
}
