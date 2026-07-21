// Bridge-core smoke test: exercises the header-only runtime layer against
// the real cdylib - version, bytecode-init error surface, call registry
// fan-out, arg semantics, and buffer moves. End-to-end typed calls are
// covered by the generated-SDK fixtures (sdk_tests/crates/cpp), which embed
// real bytecode.
#include <baml/baml.h>

#include <cassert>
#include <chrono>
#include <cstdio>
#include <stdexcept>
#include <string>
#include <type_traits>
#include <vector>

static void TestVersion() {
  const std::string v = baml::version();
  assert(!v.empty());
  std::printf("version: %s\n", v.c_str());
}

static void TestBytecodeInitRejectsGarbage() {
  // The happy path is exercised once generated SDKs embed bytecode; here we
  // pin that the export exists and reports failure as a catchable error.
  const uint8_t garbage[] = {0xde, 0xad, 0xbe, 0xef};
  bool threw = false;
  try {
    const std::string v = baml::version();
    baml::initialize_runtime_from_bytecode(garbage, sizeof(garbage), v.c_str());
  } catch (const baml::error&) {
    threw = true;
  }
  assert(threw);
  std::printf("bytecode init rejects garbage ok\n");
}

static void TestCallRegistryRoundTrip() {
  auto started = baml::detail::call_registry::instance().begin();
  const std::vector<int8_t> payload = {1, 2, 3, 4};
  baml_cpp_result_trampoline(started.correlation_id, payload.data(),
                             payload.size());
  auto status = started.envelope.wait_for(std::chrono::seconds(1));
  assert(status == std::future_status::ready);
  const std::vector<uint8_t> got = started.envelope.get();
  assert(got.size() == 4 && got[0] == 1 && got[3] == 4);
  std::printf("call registry round trip ok\n");
}

static void TestArgTwoState() {
  // Non-nullable optional argument (BAML `count: int = 5`).
  baml::arg<int64_t> unset_arg;
  assert(unset_arg.is_unset() && !unset_arg.is_set());

  baml::arg<int64_t> explicit_unset = baml::unset;
  assert(explicit_unset.is_unset());

  baml::arg<int64_t> value_arg = int64_t{42};
  assert(value_arg.is_set() && value_arg.value() == 42);

  // Null is not in a non-nullable argument's type: rejected at compile time.
  static_assert(
      !std::is_constructible<baml::arg<int64_t>, std::nullopt_t>::value,
      "null must not be passable to a non-nullable argument");
  static_assert(
      !std::is_constructible<baml::arg<int64_t>, std::monostate>::value,
      "null must not be passable to a non-nullable argument");

  // Nullable optional argument (BAML `lang: string? = "en"`).
  baml::arg<std::optional<std::string>> lang_value = std::string("fr");
  assert(lang_value.is_set() && lang_value.value().has_value());

  baml::arg<std::optional<std::string>> lang_null = std::nullopt;
  assert(lang_null.is_set() && !lang_null.value().has_value());

  baml::arg<std::optional<std::string>> lang_null2 = std::monostate{};
  assert(lang_null2.is_set() && !lang_null2.value().has_value());

  // Bare-null-typed argument: monostate is the VALUE there.
  baml::arg<std::monostate> null_typed_arg = std::monostate{};
  assert(null_typed_arg.is_set());

  bool threw = false;
  try {
    (void)unset_arg.value();
  } catch (const std::logic_error&) {
    threw = true;
  }
  assert(threw);

  // Setters take arg<T> by value; a string literal must convert in one hop.
  struct Opts {
    baml::arg<std::optional<std::string>> lang;
    Opts& set_lang(baml::arg<std::optional<std::string>> v) {
      lang = std::move(v);
      return *this;
    }
  };
  assert(Opts{}.set_lang("fr").lang.value().value() == "fr");
  assert(Opts{}.set_lang(std::nullopt).lang.is_set());
  assert(!Opts{}.set_lang(std::nullopt).lang.value().has_value());
  assert(Opts{}.set_lang(std::monostate{}).lang.is_set());
  assert(Opts{}.lang.is_unset());
  std::printf("arg two-state ok\n");
}

static void TestOwnedBufferMove() {
  baml::detail::owned_buffer a{baml::detail::api().version()};
  assert(!a.empty());
  baml::detail::owned_buffer b = std::move(a);
  assert(a.empty() && !b.empty());
  std::printf("owned buffer move ok\n");
}

int main() {
  TestVersion();
  TestBytecodeInitRejectsGarbage();
  TestCallRegistryRoundTrip();
  TestArgTwoState();
  TestOwnedBufferMove();
  std::printf("bridge core smoke: all ok\n");
  return 0;
}
