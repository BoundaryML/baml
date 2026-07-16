// Bridge-core smoke test: exercises the header-only runtime layer against the
// real cdylib - version, runtime init, end-to-end typed calls through the
// codec (containers, error envelope), call registry fan-out, Arg semantics,
// and buffer moves.
#include <baml/baml.h>

#include <cassert>
#include <chrono>
#include <cstdio>
#include <map>
#include <stdexcept>
#include <string>
#include <type_traits>
#include <vector>

static void TestVersion() {
  const std::string v = baml::Version();
  assert(!v.empty());
  std::printf("version: %s\n", v.c_str());
}

static void TestInitializeRuntime() {
  const std::map<std::string, std::string> files = {
      {"main.baml",
       "function ReturnOne() -> int {\n  1\n}\n"
       "function Identity(s: string) -> string {\n  s\n}\n"
       "function Twice(xs: int[]) -> int[] {\n  xs.map((x) -> { x * 2 })\n}\n"},
  };
  baml::InitializeRuntime(".", files);
  std::printf("runtime initialized\n");
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

static void TestCallFunctionEndToEnd() {
  {
    baml::detail::ArgsEncoder args;
    const int64_t got =
        baml::detail::CallSync<int64_t>("ReturnOne", std::move(args));
    assert(got == 1);
  }
  {
    baml::detail::ArgsEncoder args;
    args.AddArg("s", [](baml::detail::wire::Writer& w) {
      baml::Codec<std::string>::Encode(w, "hello");
    });
    const std::string got =
        baml::detail::CallSync<std::string>("Identity", std::move(args));
    assert(got == "hello");
  }
  {
    baml::detail::ArgsEncoder args;
    args.AddArg("xs", [](baml::detail::wire::Writer& w) {
      baml::Codec<std::vector<int64_t>>::Encode(w, {1, 2, 3});
    });
    const std::vector<int64_t> got =
        baml::detail::CallSync<std::vector<int64_t>>("Twice", std::move(args));
    assert((got == std::vector<int64_t>{2, 4, 6}));
  }
  {
    // Pre-call failure (unknown function) arrives as a BamlError envelope.
    bool threw = false;
    try {
      baml::detail::ArgsEncoder args;
      baml::detail::CallSync<int64_t>("NoSuchFunction", std::move(args));
    } catch (const baml::BamlError&) {
      threw = true;
    }
    assert(threw);
  }
  std::printf("call function end to end ok\n");
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
  assert(unset_arg.is_unset() && !unset_arg.is_set());

  baml::Arg<int64_t> explicit_unset = baml::unset;
  assert(explicit_unset.is_unset());

  baml::Arg<int64_t> value_arg = int64_t{42};
  assert(value_arg.is_set() && value_arg.value() == 42);

  // Null is not in a non-nullable argument's type: rejected at compile time.
  static_assert(
      !std::is_constructible<baml::Arg<int64_t>, std::nullopt_t>::value,
      "null must not be passable to a non-nullable argument");
  static_assert(
      !std::is_constructible<baml::Arg<int64_t>, std::monostate>::value,
      "null must not be passable to a non-nullable argument");

  // Nullable optional argument (BAML `lang: string? = "en"`).
  baml::Arg<std::optional<std::string>> lang_value = std::string("fr");
  assert(lang_value.is_set() && lang_value.value().has_value());

  baml::Arg<std::optional<std::string>> lang_null = std::nullopt;
  assert(lang_null.is_set() && !lang_null.value().has_value());

  baml::Arg<std::optional<std::string>> lang_null2 = std::monostate{};
  assert(lang_null2.is_set() && !lang_null2.value().has_value());

  // Bare-null-typed argument: monostate is the VALUE there.
  baml::Arg<std::monostate> null_typed_arg = std::monostate{};
  assert(null_typed_arg.is_set());

  bool threw = false;
  try {
    (void)unset_arg.value();
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
  assert(Opts{}.set_lang("fr").lang.value().value() == "fr");
  assert(Opts{}.set_lang(std::nullopt).lang.is_set());
  assert(!Opts{}.set_lang(std::nullopt).lang.value().has_value());
  assert(Opts{}.set_lang(std::monostate{}).lang.is_set());
  assert(Opts{}.lang.is_unset());
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
  TestInitializeRuntime();
  TestBytecodeInitRejectsGarbage();
  TestCallFunctionEndToEnd();
  TestCallRegistryRoundTrip();
  TestArgTwoState();
  TestOwnedBufferMove();
  std::printf("bridge core smoke: all ok\n");
  return 0;
}
