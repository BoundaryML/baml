// Bridge-core smoke test: exercises the header-only runtime layer against
// the real cdylib - version, bytecode-init error surface, call registry
// fan-out, arg semantics, and buffer moves. End-to-end typed calls are
// covered by the generated-SDK fixtures (sdk_tests/crates/cpp), which embed
// real bytecode.
#include <baml/baml.h>

#include <chrono>
#include <cstdio>
#include <stdexcept>
#include <string>
#include <type_traits>
#include <vector>

static void Require(bool condition, const char* message) {
  if (!condition) {
    throw std::runtime_error(message);
  }
}

static void TestVersion() {
  const std::string v = baml::version();
  Require(!v.empty(), "runtime version is empty");
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
  Require(threw, "garbage bytecode was accepted");
  std::printf("bytecode init rejects garbage ok\n");
}

static void TestCallRegistryRoundTrip() {
  auto started = baml::detail::call_registry::instance().begin();
  const std::vector<int8_t> payload = {1, 2, 3, 4};
  baml_cpp_result_trampoline(started.correlation_id, payload.data(),
                             payload.size());
  if (!started.state->wait_until(std::chrono::steady_clock::now() +
                                 std::chrono::seconds(1))) {
    throw std::runtime_error("call registry round trip timed out");
  }
  const std::vector<uint8_t>& got = started.state->wait();
  Require(got.size() == 4 && got[0] == 1 && got[3] == 4,
          "call registry returned the wrong payload");
  std::printf("call registry round trip ok\n");
}

static void TestArgTwoState() {
  // Non-nullable optional argument (BAML `count: int = 5`).
  baml::arg<int64_t> unset_arg;
  Require(unset_arg.is_unset() && !unset_arg.is_set(),
          "default argument is not unset");

  baml::arg<int64_t> explicit_unset = baml::unset;
  Require(explicit_unset.is_unset(), "explicit unset argument is set");

  baml::arg<int64_t> value_arg = int64_t{42};
  Require(value_arg.is_set() && value_arg.value() == 42,
          "value argument was not preserved");

  // Null is not in a non-nullable argument's type: rejected at compile time.
  static_assert(
      !std::is_constructible<baml::arg<int64_t>, std::nullopt_t>::value,
      "null must not be passable to a non-nullable argument");
  static_assert(
      !std::is_constructible<baml::arg<int64_t>, std::monostate>::value,
      "null must not be passable to a non-nullable argument");

  // Nullable optional argument (BAML `lang: string? = "en"`).
  baml::arg<std::optional<std::string>> lang_value = std::string("fr");
  Require(lang_value.is_set() && lang_value.value().has_value(),
          "nullable argument lost its value");

  baml::arg<std::optional<std::string>> lang_null = std::nullopt;
  Require(lang_null.is_set() && !lang_null.value().has_value(),
          "null argument is not explicitly set");

  baml::arg<std::optional<std::string>> lang_null2 = std::monostate{};
  Require(lang_null2.is_set() && !lang_null2.value().has_value(),
          "monostate argument is not explicitly null");

  // Bare-null-typed argument: monostate is the VALUE there.
  baml::arg<std::monostate> null_typed_arg = std::monostate{};
  Require(null_typed_arg.is_set(), "bare null argument is not set");

  bool threw = false;
  try {
    (void)unset_arg.value();
  } catch (const std::logic_error&) {
    threw = true;
  }
  Require(threw, "reading an unset argument did not fail");

  // Setters take arg<T> by value; a string literal must convert in one hop.
  struct Opts {
    baml::arg<std::optional<std::string>> lang;
    Opts& set_lang(baml::arg<std::optional<std::string>> v) {
      lang = std::move(v);
      return *this;
    }
  };
  Require(Opts{}.set_lang("fr").lang.value().value() == "fr",
          "string literal argument was not preserved");
  Require(Opts{}.set_lang(std::nullopt).lang.is_set(),
          "nullopt argument is not set");
  Require(!Opts{}.set_lang(std::nullopt).lang.value().has_value(),
          "nullopt argument has a value");
  Require(Opts{}.set_lang(std::monostate{}).lang.is_set(),
          "monostate argument is not set");
  Require(Opts{}.lang.is_unset(), "default options argument is set");
  std::printf("arg two-state ok\n");
}

static void TestOwnedBufferMove() {
  baml::detail::owned_buffer a{baml::detail::api().version()};
  Require(!a.empty(), "runtime returned an empty owned buffer");
  baml::detail::owned_buffer b = std::move(a);
  Require(a.empty() && !b.empty(),
          "owned buffer move did not transfer ownership");
  std::printf("owned buffer move ok\n");
}

static void TestUnhandledSpawnErrorUsesHostDefault() {
  bool threw = false;
  try {
    baml::detail::host_default_unhandled_spawn_error(
        std::make_exception_ptr(baml::error("boom")), false);
  } catch (const baml::error& exception) {
    threw = std::string(exception.what()) == "boom";
  }
  Require(threw, "default unhandled-spawn handler did not rethrow");
}

static void RunTest(const char* name, void (*test)()) {
  std::fprintf(stderr, "running %s\n", name);
  std::fflush(stderr);
  test();
  std::fprintf(stderr, "passed %s\n", name);
  std::fflush(stderr);
}

int main() {
  RunTest("version", TestVersion);
  RunTest("bytecode init", TestBytecodeInitRejectsGarbage);
  RunTest("call registry", TestCallRegistryRoundTrip);
  RunTest("argument states", TestArgTwoState);
  RunTest("owned buffer move", TestOwnedBufferMove);
  RunTest("unhandled_spawn_error_uses_host_default",
          TestUnhandledSpawnErrorUsesHostDefault);
  std::printf("bridge core smoke: all ok\n");
  return 0;
}
