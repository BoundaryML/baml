// Bridge-core smoke test: exercises the header-only runtime layer against
// the real cdylib - version, bytecode-init error surface, call registry
// fan-out, Arg semantics, and buffer moves. End-to-end typed calls are
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
  const std::string v = baml::Version();
  assert(!v.empty());
  std::printf("version: %s\n", v.c_str());
}

static void TestBytecodeInitRejectsGarbage() {
  // The happy path is exercised once generated SDKs embed bytecode; here we
  // pin that the export exists and reports failure as a catchable error.
  const uint8_t garbage[] = {0xde, 0xad, 0xbe, 0xef};
  bool threw = false;
  try {
    const std::string v = baml::Version();
    baml::InitializeRuntimeFromBytecode(garbage, sizeof(garbage), v.c_str());
  } catch (const baml::BamlError&) {
    threw = true;
  }
  assert(threw);
  std::printf("bytecode init rejects garbage ok\n");
}

static void TestCallRegistryRoundTrip() {
  auto started = baml::detail::CallRegistry::Instance().Begin();
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
  baml::Arg<int64_t> unset_arg;
  assert(unset_arg.IsUnset() && !unset_arg.IsSet());

  baml::Arg<int64_t> explicit_unset = baml::kUnset;
  assert(explicit_unset.IsUnset());

  baml::Arg<int64_t> value_arg = int64_t{42};
  assert(value_arg.IsSet() && value_arg.Value() == 42);

  // Null is not in a non-nullable argument's type: rejected at compile time.
  static_assert(
      !std::is_constructible<baml::Arg<int64_t>, std::nullopt_t>::value,
      "null must not be passable to a non-nullable argument");
  static_assert(
      !std::is_constructible<baml::Arg<int64_t>, std::monostate>::value,
      "null must not be passable to a non-nullable argument");

  // Nullable optional argument (BAML `lang: string? = "en"`).
  baml::Arg<std::optional<std::string>> lang_value = std::string("fr");
  assert(lang_value.IsSet() && lang_value.Value().has_value());

  baml::Arg<std::optional<std::string>> lang_null = std::nullopt;
  assert(lang_null.IsSet() && !lang_null.Value().has_value());

  baml::Arg<std::optional<std::string>> lang_null2 = std::monostate{};
  assert(lang_null2.IsSet() && !lang_null2.Value().has_value());

  // Bare-null-typed argument: monostate is the VALUE there.
  baml::Arg<std::monostate> null_typed_arg = std::monostate{};
  assert(null_typed_arg.IsSet());

  bool threw = false;
  try {
    (void)unset_arg.Value();
  } catch (const std::logic_error&) {
    threw = true;
  }
  assert(threw);

  // Setters take Arg<T> by value; a string literal must convert in one hop.
  struct Opts {
    baml::Arg<std::optional<std::string>> lang;
    Opts& set_lang(baml::Arg<std::optional<std::string>> v) {
      lang = std::move(v);
      return *this;
    }
  };
  assert(Opts{}.set_lang("fr").lang.Value().value() == "fr");
  assert(Opts{}.set_lang(std::nullopt).lang.IsSet());
  assert(!Opts{}.set_lang(std::nullopt).lang.Value().has_value());
  assert(Opts{}.set_lang(std::monostate{}).lang.IsSet());
  assert(Opts{}.lang.IsUnset());
  std::printf("arg two-state ok\n");
}

static void TestOwnedBufferMove() {
  baml::detail::OwnedBuffer a{baml::detail::Api().version()};
  assert(!a.empty());
  baml::detail::OwnedBuffer b = std::move(a);
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
