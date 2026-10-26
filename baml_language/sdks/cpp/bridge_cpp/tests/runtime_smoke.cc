// Bridge-core smoke test: exercises the header-only runtime layer against
// the real cdylib - version, bytecode-init error surface, call registry
// fan-out, arg semantics, and buffer moves. End-to-end typed calls are
// covered by the generated-SDK fixtures (sdk_tests/crates/cpp), which embed
// real bytecode.
#include <baml/baml.h>

#include <array>
#include <chrono>
#include <cstdio>
#include <functional>
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
    baml::initialize_runtime_from_blob(garbage, sizeof(garbage), v.c_str());
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

static void TestBigintCodec() {
  Require(baml::codec<baml::bigint>::baml_ty().primitive().kind() ==
              baml::detail::pb::BAML_TY_PRIMITIVE_BIGINT,
          "bigint codec reported the wrong BAML type");

  struct Case {
    const char* decimal;
    const char* hex;
  };
  const Case cases[] = {
      {"0", "0"},
      {"10", "a"},
      {"1000000000", "3b9aca00"},
      {"4294967295", "ffffffff"},
      {"4294967296", "100000000"},
      {"18446744073709551615", "ffffffffffffffff"},
      {"123456789012345678901234567890", "18ee90ff6c373e0ee4e3f0ad2"},
      {"-10", "-a"}};
  for (const auto& test_case : cases) {
    const baml::bigint value(test_case.decimal);
    baml::detail::pb::InboundValue encoded;
    baml::codec<baml::bigint>::encode(encoded, value);
    Require(
        encoded.has_bigint_value() && encoded.bigint_value() == test_case.hex,
        "bigint encode did not convert decimal to wire hexadecimal");

    baml::detail::pb::BamlOutboundValue outbound;
    outbound.set_bigint_value(test_case.hex);
    Require(baml::codec<baml::bigint>::decode(outbound) == value,
            "bigint decode did not convert wire hexadecimal to decimal");

    baml::detail::pb::BamlOutboundValue literal;
    literal.mutable_literal_value()->set_bigint_value(test_case.hex);
    Require(baml::codec<baml::bigint>::decode(literal) == value,
            "literal bigint decode did not convert wire hexadecimal to "
            "decimal");
  }

  const char* noncanonical[] = {"+10", "0010", "-0"};
  for (const char* decimal : noncanonical) {
    bool threw = false;
    try {
      baml::detail::pb::InboundValue encoded;
      baml::codec<baml::bigint>::encode(encoded, baml::bigint(decimal));
    } catch (const baml::error&) {
      threw = true;
    }
    Require(threw, "bigint encode accepted noncanonical decimal text");
  }

  const baml::bigint large_value("1" + std::string(100000, '2'));
  baml::detail::pb::InboundValue large_encoded;
  baml::codec<baml::bigint>::encode(large_encoded, large_value);
  baml::detail::pb::BamlOutboundValue large_outbound;
  large_outbound.set_bigint_value(large_encoded.bigint_value());
  Require(baml::codec<baml::bigint>::decode(large_outbound) == large_value,
          "large bigint radix conversion did not round trip");
  std::printf("bigint codec ok\n");
}

static void TestPortableValuesTranscodeWithoutHandles() {
  baml::detail::pb::BamlOutboundValue media;
  media.mutable_media_value()->set_media(
      baml::detail::pb::MediaTypeEnum::IMAGE);
  media.mutable_media_value()->set_url("https://example.test/cat.png");
  baml::detail::pb::InboundValue inbound_media;
  baml::detail::transcode_outbound_to_inbound(inbound_media, media);
  Require(inbound_media.has_media_value(),
          "media did not use the portable inbound arm");
  Require(inbound_media.media_value().url() == "https://example.test/cat.png",
          "portable media payload changed");
  Require(!inbound_media.has_handle(), "portable media became a handle");

  baml::detail::pb::BamlOutboundValue prompt;
  prompt.mutable_prompt_ast_value()->mutable_simple()->set_string("hello");
  baml::detail::pb::InboundValue inbound_prompt;
  baml::detail::transcode_outbound_to_inbound(inbound_prompt, prompt);
  Require(inbound_prompt.has_prompt_ast_value(),
          "prompt did not use the portable inbound arm");
  Require(inbound_prompt.prompt_ast_value().simple().string() == "hello",
          "portable prompt payload changed");
  Require(!inbound_prompt.has_handle(), "portable prompt became a handle");
}

static void TestTypedSpecStreamAndPortableValueCodecs() {
  using Spec = baml::function_spec<std::string>;
  using Stream = baml::stream<std::string>;

  const auto spec_ty = baml::codec<Spec>::baml_ty();
  Require(spec_ty.class_ty().name() == "ai.FunctionSpec",
          "FunctionSpec codec lost nominal identity");
  Require(spec_ty.class_ty().type_args_size() == 1,
          "FunctionSpec codec lost its output type argument");

  const auto stream_ty = baml::codec<Stream>::baml_ty();
  Require(stream_ty.class_ty().name() == "ai.stream.Stream",
          "Stream codec lost nominal identity");
  Require(stream_ty.class_ty().type_args_size() == 1,
          "Stream codec lost its value type argument");

  baml::detail::pb::BamlOutboundValue done;
  done.mutable_class_value()->set_name("ai.stream.Done");
  const auto finished =
      baml::codec<baml::stream_item<std::optional<std::string>>>::decode(done);
  Require(finished.done(), "stream Done decoded as a partial");

  baml::detail::pb::BamlOutboundValue null_partial;
  null_partial.mutable_null_value();
  const auto partial =
      baml::codec<baml::stream_item<std::optional<std::string>>>::decode(
          null_partial);
  Require(!partial.done() && !partial.value().has_value(),
          "nullable partial was confused with stream Done");

  baml::detail::pb::BamlOutboundValue prompt_value;
  prompt_value.mutable_prompt_ast_value()->mutable_simple()->set_string(
      "portable");
  const baml::prompt prompt = baml::codec<baml::prompt>::decode(prompt_value);
  const baml::prompt equal_prompt =
      baml::codec<baml::prompt>::decode(prompt_value);
  Require(prompt == equal_prompt, "Prompt equality was not structural");
  baml::detail::pb::InboundValue encoded_prompt;
  baml::codec<baml::prompt>::encode(encoded_prompt, prompt);
  Require(encoded_prompt.has_prompt_ast_value() &&
              encoded_prompt.prompt_ast_value().simple().string() == "portable",
          "Prompt codec did not preserve portable data");
  Require(!encoded_prompt.has_value_type(),
          "a prompt implies its own type; the engine refuses a stated one");

  const baml::image image = baml::image::from_url(
      "https://example.test/cat.png", std::string("image/png"));
  baml::detail::pb::InboundValue encoded_image;
  baml::codec<baml::image>::encode(encoded_image, image);
  Require(
      encoded_image.has_media_value() &&
          encoded_image.media_value().url() == "https://example.test/cat.png",
      "Media codec did not preserve portable data");
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

static void TestTypeValuesCrossAsTheirOneSpelling() {
  namespace pb = baml::detail::pb;
  pb::BamlOutboundValue value;
  pb::BamlTyClass* box = value.mutable_ty_value()->mutable_class_ty();
  box->set_name("user.Box");
  box->add_type_args()
      ->mutable_list()
      ->mutable_item()
      ->mutable_enum_()
      ->set_name("user.Color");

  bool decode_threw = false;
  try {
    (void)baml::codec<int64_t>::decode(value);
  } catch (const baml::error& exception) {
    decode_threw =
        std::string(exception.what()).find("got type") != std::string::npos;
  }
  Require(decode_threw, "a type value decoded as an int");

  pb::InboundValue named;
  baml::detail::transcode_outbound_to_inbound(named, value);
  Require(named.has_ty_value() && named.ty_value().SerializeAsString() ==
                                      value.ty_value().SerializeAsString(),
          "a type of compiled declarations was not restated as it came");

  // A live head is a reference: restating one clones it, and a key the
  // engine does not hold cannot be cloned.
  box->add_type_args()->mutable_enum_()->set_live(~uint64_t{0});
  bool restate_threw = false;
  try {
    pb::InboundValue live;
    baml::detail::transcode_outbound_to_inbound(live, value);
  } catch (const baml::error& exception) {
    restate_threw =
        std::string(exception.what()) == "type reference clone failed";
  }
  Require(restate_threw, "a live head was restated without a clone");
}

static void TestEveryLiveHeadIsVisitedOncePerOccurrence() {
  namespace pb = baml::detail::pb;
  pb::BamlTy ty;
  pb::BamlTyFunction* function = ty.mutable_function();
  function->add_params()->mutable_ty()->mutable_class_ty()->set_live(1);
  pb::BamlTyInterface* interface_ty =
      function->mutable_ret()->mutable_interface();
  interface_ty->set_live(2);
  interface_ty->add_bindings()->mutable_ty()->mutable_enum_variant()->set_live(
      3);
  pb::BamlTyFuture* future = function->mutable_throws()->mutable_future();
  future->mutable_value()->mutable_type_alias()->set_name("user.Alias");
  future->mutable_error()
      ->mutable_list()
      ->mutable_item()
      ->mutable_enum_()
      ->set_live(1);
  const std::string before = ty.SerializeAsString();

  std::vector<uint64_t> keys;
  baml::detail::for_each_nominal(ty, [&](const auto& nominal) {
    if (nominal.has_live()) keys.push_back(nominal.live());
  });
  Require((keys == std::vector<uint64_t>{1, 2, 3, 1}),
          "live heads were not visited once per occurrence");
  Require(ty.SerializeAsString() == before, "visiting heads changed the type");
}

static void TestTranscodeKeepsTheTypesTheEngineGave() {
  namespace pb = baml::detail::pb;
  pb::BamlOutboundValue list;
  list.mutable_list_value()->mutable_item_type()->mutable_primitive()->set_kind(
      pb::BamlTyPrimitiveKind::BAML_TY_PRIMITIVE_INT);
  list.mutable_list_value()->add_items()->set_int_value(1);
  pb::InboundValue inbound_list;
  baml::detail::transcode_outbound_to_inbound(inbound_list, list);
  Require(inbound_list.has_value_type() && inbound_list.value_type().has_list(),
          "list lost its element type");
  Require(inbound_list.value_type().list().item().primitive().kind() ==
              pb::BamlTyPrimitiveKind::BAML_TY_PRIMITIVE_INT,
          "list stated the wrong element type");

  pb::BamlOutboundValue map;
  map.mutable_map_value()->mutable_key_type()->mutable_primitive()->set_kind(
      pb::BamlTyPrimitiveKind::BAML_TY_PRIMITIVE_STRING);
  map.mutable_map_value()->mutable_value_type()->mutable_primitive()->set_kind(
      pb::BamlTyPrimitiveKind::BAML_TY_PRIMITIVE_BOOL);
  pb::InboundValue inbound_map;
  baml::detail::transcode_outbound_to_inbound(inbound_map, map);
  Require(inbound_map.has_value_type() && inbound_map.value_type().has_map(),
          "map lost its key and value types");

  // A scalar inside a union envelope goes back as the scalar, which implies
  // its type; the envelope states nothing.
  pb::BamlOutboundValue variant;
  pb::BamlValueUnionVariant* envelope = variant.mutable_union_variant_value();
  envelope->mutable_self_type()
      ->mutable_union_()
      ->add_options()
      ->mutable_primitive()
      ->set_kind(pb::BamlTyPrimitiveKind::BAML_TY_PRIMITIVE_INT);
  envelope->mutable_self_type()
      ->mutable_union_()
      ->add_options()
      ->mutable_primitive()
      ->set_kind(pb::BamlTyPrimitiveKind::BAML_TY_PRIMITIVE_STRING);
  envelope->set_selected_option_index(1);
  envelope->mutable_value()->set_string_value("x");
  pb::InboundValue inbound_variant;
  baml::detail::transcode_outbound_to_inbound(inbound_variant, variant);
  Require(inbound_variant.string_value() == "x", "union payload changed");
  Require(!inbound_variant.has_value_type(),
          "a scalar implies its type; the union envelope states nothing");

  pb::BamlOutboundValue literal;
  literal.mutable_literal_value()->set_int_value(7);
  pb::InboundValue inbound_literal;
  baml::detail::transcode_outbound_to_inbound(inbound_literal, literal);
  Require(inbound_literal.int_value() == 7, "literal payload changed");
  Require(!inbound_literal.has_value_type(),
          "a literal is implied by its scalar; nothing is stated");
}

// A media value holds content, never a path. Content that was read from a
// file keeps the base name of that file.
static void TestMediaContentReadFromAFileKeepsItsName() {
  const baml::image named =
      baml::image::from_file_content("/nowhere/cat.jpg", "aGk=");
  Require(named.name() == std::optional<std::string>("cat.jpg"),
          "named content lost its name");
  Require(named.mime_type() == std::optional<std::string>("image/jpeg"),
          "named content did not infer its MIME type");
  Require(named.base64() == std::optional<std::string>("aGk="),
          "named content lost its content");
  Require(!named.url(), "named content has a URL");
  Require(named == baml::image::from_file_content("/elsewhere/cat.jpg", "aGk="),
          "equal named content compared unequal");
  Require(named != baml::image::from_base64("aGk=", "image/jpeg"),
          "content with and without a name compared equal");
  Require(!baml::image::from_base64("aGk=").name(), "base64 media has a name");
}

static void RunTest(const char* name, void (*test)()) {
  std::fprintf(stderr, "running %s\n", name);
  std::fflush(stderr);
  test();
  std::fprintf(stderr, "passed %s\n", name);
  std::fflush(stderr);
}

namespace callbacks {

namespace pb = baml::detail::pb;

// One argument of a BamlToHostCall: positional, or optional by `name`.
struct call_arg {
  explicit call_arg(pb::BamlOutboundValue value, std::string name = "")
      : value(std::move(value)), name(std::move(name)) {}
  pb::BamlOutboundValue value;
  std::string name;
};

std::vector<uint8_t> call_bytes(const std::vector<call_arg>& args) {
  pb::BamlToHostCall call;
  for (const call_arg& given : args) {
    pb::BamlToHostArg* arg = call.add_args();
    *arg->mutable_value() = given.value;
    if (!given.name.empty()) {
      arg->set_is_optional_arg(true);
      arg->set_arg_name(given.name);
    }
  }
  const std::string bytes = call.SerializeAsString();
  return std::vector<uint8_t>(bytes.begin(), bytes.end());
}

pb::BamlOutboundValue int_value(int64_t value) {
  pb::BamlOutboundValue out;
  out.set_int_value(value);
  return out;
}

pb::BamlOutboundValue string_value(const std::string& value) {
  pb::BamlOutboundValue out;
  out.set_string_value(value);
  return out;
}

// `1` inside `levels` lists, each stating its element type.
pb::BamlOutboundValue nested_list(std::size_t levels) {
  pb::BamlOutboundValue value = int_value(1);
  for (std::size_t i = 0; i < levels; ++i) {
    pb::BamlOutboundValue list;
    list.mutable_list_value()->mutable_item_type()->mutable_unknown();
    *list.mutable_list_value()->add_items() = std::move(value);
    value = std::move(list);
  }
  return value;
}

// A BAML error a nested call raised, carrying `value`.
baml::error nested_error(pb::BamlOutboundValue value) {
  return baml::error("nested", "user.Nested", "",
                     baml::detail::take_thrown_value(value));
}

using Outcome = pb::HostCallCompletion::OutcomeCase;

void RequireOutcome(const pb::HostCallCompletion& completion, Outcome outcome,
                    const char* what) {
  if (completion.outcome_case() != outcome) {
    std::fprintf(stderr, "outcome %d, expected %d (%s)\n",
                 static_cast<int>(completion.outcome_case()),
                 static_cast<int>(outcome), what);
    throw std::runtime_error(what);
  }
}

}  // namespace callbacks

// A callback's call completes with the one outcome each fault is: what the
// callable did, its breach of contract, or the bridge's own failure.
static void TestCallbackCompletionsClassifyEachFault() {
  namespace pb = baml::detail::pb;
  using baml::detail::host_call_completion;
  using callbacks::call_arg;
  using callbacks::call_bytes;
  using callbacks::int_value;
  using callbacks::nested_error;
  using callbacks::nested_list;
  using callbacks::RequireOutcome;
  using callbacks::string_value;
  const std::array<std::string, 1> positional{{""}};

  const std::function<int64_t(int64_t)> plus_one = [](int64_t x) {
    return x + 1;
  };
  pb::HostCallCompletion completion = host_call_completion(
      plus_one, positional, call_bytes({call_arg(int_value(1))}));
  RequireOutcome(completion, pb::HostCallCompletion::kReturned, "returned");
  Require(completion.returned().int_value() == 2, "returned the wrong value");

  // Arguments the bridge cannot decode, or that do not match the callable's
  // parameters, are the bridge's own failure, never the callable's.
  RequireOutcome(host_call_completion(plus_one, positional,
                                      std::vector<uint8_t>{0xff, 0xff, 0xff}),
                 pb::HostCallCompletion::kBridgeFailure, "malformed payload");
  RequireOutcome(host_call_completion(plus_one, positional, call_bytes({})),
                 pb::HostCallCompletion::kBridgeFailure, "missing argument");
  RequireOutcome(host_call_completion(plus_one, positional,
                                      call_bytes({call_arg(int_value(1)),
                                                  call_arg(int_value(2))})),
                 pb::HostCallCompletion::kBridgeFailure, "extra argument");
  RequireOutcome(
      host_call_completion(plus_one, positional,
                           call_bytes({call_arg(string_value("one"))})),
      pb::HostCallCompletion::kBridgeFailure, "undecodable argument");
  const std::function<int64_t(int64_t, baml::arg<int64_t>)> with_optional =
      [](int64_t x, baml::arg<int64_t> y) {
        return x + (y.is_set() ? y.value() : 0);
      };
  const std::array<std::string, 2> optional_names{{"", "y"}};
  RequireOutcome(
      host_call_completion(
          with_optional, optional_names,
          call_bytes({call_arg(int_value(1)), call_arg(int_value(2), "z")})),
      pb::HostCallCompletion::kBridgeFailure, "unknown optional argument");
  RequireOutcome(
      host_call_completion(
          with_optional, optional_names,
          call_bytes({call_arg(int_value(1)), call_arg(int_value(2), "y"),
                      call_arg(int_value(3), "y")})),
      pb::HostCallCompletion::kBridgeFailure, "repeated optional argument");
  completion = host_call_completion(
      with_optional, optional_names,
      call_bytes({call_arg(int_value(1)), call_arg(int_value(2), "y")}));
  RequireOutcome(completion, pb::HostCallCompletion::kReturned,
                 "optional argument by name");
  Require(completion.returned().int_value() == 3, "optional argument lost");

  // An exception of the callable's own is raised, registered for identity.
  const std::function<int64_t(int64_t)> fails = [](int64_t) -> int64_t {
    throw std::out_of_range("missing");
  };
  completion = host_call_completion(fails, positional,
                                    call_bytes({call_arg(int_value(1))}));
  RequireOutcome(completion, pb::HostCallCompletion::kRaised, "raised");
  Require(completion.raised().message() == "missing" &&
              completion.raised().handle() != 0,
          "raised lost the exception");
  // So is an SDK error from a nested call that the callable let through.
  const std::function<int64_t(int64_t)> sdk_failure = [](int64_t) -> int64_t {
    throw baml::bridge_error("a nested call's argument was refused");
  };
  RequireOutcome(host_call_completion(sdk_failure, positional,
                                      call_bytes({call_arg(int_value(1))})),
                 pb::HostCallCompletion::kRaised,
                 "a nested call's SDK error is the callable's own");

  // A BAML value the callable throws goes back thrown; one too deep for the
  // wire is the bridge's own failure, reported as such.
  const std::function<int64_t(int64_t)> throws_value = [](int64_t) -> int64_t {
    throw baml::host_throw<std::string>("bad");
  };
  completion = host_call_completion(throws_value, positional,
                                    call_bytes({call_arg(int_value(1))}));
  RequireOutcome(completion, pb::HostCallCompletion::kThrown, "thrown");
  Require(completion.thrown().string_value() == "bad", "thrown value lost");
  const std::function<int64_t(int64_t)> throws_deep = [](int64_t) -> int64_t {
    throw baml::host_throw<baml::value>(
        baml::codec<baml::value>::decode(nested_list(60)));
  };
  completion = host_call_completion(throws_deep, positional,
                                    call_bytes({call_arg(int_value(1))}));
  RequireOutcome(completion, pb::HostCallCompletion::kBridgeFailure,
                 "thrown value too deep");
  Require(
      completion.bridge_failure().find("the wire carries") != std::string::npos,
      "the bridge failure does not say why");

  // A result: too deep for the wire is the bridge's own failure, measured
  // exactly (48 stated list levels nest 99 messages under the completion,
  // 49 nest 101); one the encoder finds does not fit its type is the
  // callable's breach.
  const auto returning = [&](std::size_t levels) {
    const std::function<baml::value(int64_t)> deep =
        [levels](int64_t) -> baml::value {
      return baml::codec<baml::value>::decode(nested_list(levels));
    };
    return host_call_completion(deep, positional,
                                call_bytes({call_arg(int_value(1))}));
  };
  RequireOutcome(returning(48), pb::HostCallCompletion::kReturned,
                 "a result at the wire's limit");
  RequireOutcome(returning(49), pb::HostCallCompletion::kBridgeFailure,
                 "a result one message past the wire's limit");
  RequireOutcome(returning(1000), pb::HostCallCompletion::kBridgeFailure,
                 "a result far past the wire's limit");
  const std::function<baml::bigint(int64_t)> malformed = [](int64_t) {
    return baml::bigint("twelve");
  };
  RequireOutcome(host_call_completion(malformed, positional,
                                      call_bytes({call_arg(int_value(1))})),
                 pb::HostCallCompletion::kContractViolation, "unfit result");

  // A BAML error or panic from a nested call the callable lets through goes
  // back as the value BAML threw, stated as the engine gave it, as often as
  // it is let through.
  pb::BamlOutboundValue nested;
  nested.mutable_class_value()->set_name("user.Nested");
  pb::BamlOutboundMapEntry* field = nested.mutable_class_value()->add_fields();
  field->set_key("message");
  field->mutable_value()->set_string_value("boom");
  const baml::error forwarded = nested_error(nested);
  const std::function<int64_t(int64_t)> forwards = [&](int64_t) -> int64_t {
    throw forwarded;
  };
  for (int round = 0; round < 2; ++round) {
    completion = host_call_completion(forwards, positional,
                                      call_bytes({call_arg(int_value(1))}));
    RequireOutcome(completion, pb::HostCallCompletion::kThrown,
                   "a forwarded BAML error");
    Require(completion.thrown().value_type().class_ty().name() == "user.Nested",
            "a forwarded BAML error lost its class");
  }
  // A forwarded value the host cannot state back, or whose references it
  // cannot clone, is the bridge's own failure.
  pb::BamlOutboundValue unstatable;
  unstatable.mutable_list_value()
      ->mutable_item_type()
      ->mutable_class_ty()
      ->set_anonymous_label("<runtime declaration>");
  const baml::error unstatable_error = nested_error(unstatable);
  const std::function<int64_t(int64_t)> forwards_unstatable =
      [&](int64_t) -> int64_t { throw unstatable_error; };
  RequireOutcome(host_call_completion(forwards_unstatable, positional,
                                      call_bytes({call_arg(int_value(1))})),
                 pb::HostCallCompletion::kBridgeFailure,
                 "a forwarded value naming an unnameable declaration");
  pb::BamlOutboundValue unknown_handle;
  unknown_handle.mutable_handle_value()->set_key(~uint64_t{0} - 1);
  unknown_handle.mutable_handle_value()->set_handle_type(
      pb::UNTAGGED_RUST_DATA);
  const baml::error unclonable_error = nested_error(unknown_handle);
  const std::function<int64_t(int64_t)> forwards_unclonable =
      [&](int64_t) -> int64_t { throw unclonable_error; };
  RequireOutcome(host_call_completion(forwards_unclonable, positional,
                                      call_bytes({call_arg(int_value(1))})),
                 pb::HostCallCompletion::kBridgeFailure,
                 "a forwarded value whose handle cannot be cloned");
}

// A host callable BAML hands back is the C++ callable this process
// registered: it decodes to that callable, called directly, and goes back
// stated under the function type it was registered with.
static void TestAHostCallableComesBack() {
  namespace pb = baml::detail::pb;
  pb::InboundValue sent;
  baml::detail::encode_callable(
      sent, std::function<int64_t(int64_t)>([](int64_t x) { return x * 2; }),
      std::array<std::string, 1>{{""}}, baml::detail::unknown_baml_ty());
  pb::BamlOutboundValue received;
  received.mutable_handle_value()->set_key(sent.handle().key());
  received.mutable_handle_value()->set_handle_type(pb::HOST_VALUE_CALLABLE);

  const auto direct =
      baml::codec<std::function<int64_t(int64_t)>>::decode(received);
  Require(direct(21) == 42, "a returned host callable is not the original");
  const auto function =
      baml::codec<baml::function<int64_t(int64_t)>>::decode(received);
  Require(function(4) == 8, "a host-backed function did not call directly");
  Require(function.call_async(5).get() == 10,
          "a host-backed function's future lost its result");

  bool refused = false;
  try {
    (void)baml::codec<std::function<std::string(int64_t)>>::decode(received);
  } catch (const baml::error&) {
    refused = true;
  }
  Require(refused, "a host callable decoded under another C++ signature");

  pb::InboundValue restated;
  baml::detail::transcode_outbound_to_inbound(restated, received);
  Require(restated.has_handle() &&
              restated.handle().key() == sent.handle().key() &&
              restated.value_type().SerializeAsString() ==
                  sent.value_type().SerializeAsString(),
          "a host callable went back without its function type");
  baml::detail::host_value_registry::instance().release(sent.handle().key());
}

int main() {
  RunTest("version", TestVersion);
  RunTest("bytecode init", TestBytecodeInitRejectsGarbage);
  RunTest("call registry", TestCallRegistryRoundTrip);
  RunTest("argument states", TestArgTwoState);
  RunTest("owned buffer move", TestOwnedBufferMove);
  RunTest("bigint codec", TestBigintCodec);
  RunTest("transcode keeps engine types",
          TestTranscodeKeepsTheTypesTheEngineGave);
  RunTest("portable prompt and media",
          TestPortableValuesTranscodeWithoutHandles);
  RunTest("typed spec, stream, prompt, and media codecs",
          TestTypedSpecStreamAndPortableValueCodecs);
  RunTest("media reads files now and keeps their names",
          TestMediaContentReadFromAFileKeepsItsName);
  RunTest("unhandled_spawn_error_uses_host_default",
          TestUnhandledSpawnErrorUsesHostDefault);
  RunTest("type values cross as their one spelling",
          TestTypeValuesCrossAsTheirOneSpelling);
  RunTest("every live head is visited once per occurrence",
          TestEveryLiveHeadIsVisitedOncePerOccurrence);
  RunTest("callback completions classify each fault",
          TestCallbackCompletionsClassifyEachFault);
  RunTest("a host callable comes back", TestAHostCallableComesBack);
  std::printf("bridge core smoke: all ok\n");
  return 0;
}
