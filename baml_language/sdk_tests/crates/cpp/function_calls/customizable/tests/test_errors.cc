// BamlError / BamlPanic delivery contract.
// Port of function_calls/customizable/test_errors.py. Deviations from the
// Python file: the extra-kwarg InvalidArgument case is a compile error in
// C++, and Python-traceback splicing has no C++ analog (the wire trace is
// asserted directly instead).
#include <chrono>
#include <cstdio>
#include <regex>
#include <string>
#include <thread>

#ifndef _WIN32
#include <sys/wait.h>
#endif

#include <baml_sdk.h>
#include <baml_test.h>

using baml_sdk::raises_test::ParseError;
using baml_sdk::throws_test::MyError;

BAML_TEST(user_throw_surfaces_declared_instance) {
  // A user throw of a declared error -> BamlError carrying the declared
  // user error instance, reachable via the typed accessors.
  try {
    baml_sdk::throws_test::ThrowMyError();
    baml_test::Fail("ThrowMyError did not throw");
  } catch (const baml::BamlError& e) {
    BAML_ASSERT(e.is<MyError>());
    const MyError value = e.get<MyError>();
    BAML_ASSERT((value == MyError{42, "boom"}));
    BAML_ASSERT(!e.is<ParseError>());
  }
}

BAML_TEST(union_throws_preserves_class_name) {
  // Single-member and multi-member throws must agree on class_name: the
  // engine wraps multi-member throws in union_variant_value, and the
  // decoder must still surface the thrown value's FQN.
  std::string single_name;
  try {
    baml_sdk::raises_test::Reparse("x");
    baml_test::Fail("Reparse did not throw");
  } catch (const baml::BamlError& e) {
    single_name = e.class_name();
  }
  try {
    baml_sdk::raises_test::LoadDoc("x");
    baml_test::Fail("LoadDoc did not throw");
  } catch (const baml::BamlError& e) {
    BAML_ASSERT_EQ(single_name, std::string("user.raises_test.ParseError"));
    BAML_ASSERT_EQ(e.class_name(), single_name);
    BAML_ASSERT(e.is<ParseError>());
  }
}

BAML_TEST(user_panic_surfaces_as_baml_panic) {
  // The panic payload is a typed baml.panics.UserPanic (routed by the
  // namespace check, distinct from a host-synthesized SdkPanic).
  try {
    baml_sdk::throws_test::DoPanic("user-initiated boom");
    baml_test::Fail("DoPanic did not throw");
  } catch (const baml::BamlPanic& e) {
    BAML_ASSERT_EQ(e.class_name(), std::string("baml.panics.UserPanic"));
    BAML_ASSERT(e.is<baml_sdk::baml::panics::UserPanic>());
    const auto value = e.get<baml_sdk::baml::panics::UserPanic>();
    BAML_ASSERT(value.message.find("user-initiated boom") != std::string::npos);
  }
}

BAML_TEST(cancellation_surfaces_as_baml_cancelled) {
  auto fut = baml_sdk::throws_test::SleepMs_async(2000);
  std::this_thread::sleep_for(std::chrono::milliseconds(100));
  BAML_ASSERT(fut.Cancel());
  try {
    fut.get();
    baml_test::Fail("cancelled SleepMs still returned");
  } catch (const baml::BamlCancelled&) {
    // expected
  }
}

BAML_TEST(str_is_non_empty) {
  // what() is non-empty -- guards the telemetry path, which records it.
  // (ParseJson is unavailable while unions -- and with them baml.json.json
  // -- are disabled; ThrowMyError stands in.)
  try {
    baml_sdk::throws_test::ThrowMyError();
    baml_test::Fail("ThrowMyError did not throw");
  } catch (const baml::BamlError& e) {
    BAML_ASSERT(std::string(e.what()).size() > 0);
  }
}

BAML_TEST(baml_error_carries_baml_trace) {
  // The trace is rendered File "<src>", line N, in <fn> lines,
  // most-recent-call-last; the throwing function is the last frame.
  try {
    baml_sdk::throws_test::ThrowMyError();
    baml_test::Fail("ThrowMyError did not throw");
  } catch (const baml::BamlError& e) {
    const std::string& trace = e.baml_trace();
    BAML_ASSERT(!trace.empty());
    const size_t last_nl = trace.find_last_of('\n');
    const std::string last =
        last_nl == std::string::npos ? trace : trace.substr(last_nl + 1);
    const std::regex frame(
        "File \"[^\"]*types\\.baml\", line [0-9]+, in "
        "user\\.throws_test\\.ThrowMyError");
    BAML_ASSERT(std::regex_match(last, frame));
  }
}

// -- clean exit: baml.sys.exit(code) must terminate the process with code,
// not raise a catchable panic. Observed via child processes.

BAML_TEST_CHILD(do_exit_0) {
  baml_sdk::throws_test::DoExit(0);
  std::printf("UNREACHABLE\n");
  return 99;
}

BAML_TEST_CHILD(do_exit_7) {
  baml_sdk::throws_test::DoExit(7);
  std::printf("UNREACHABLE\n");
  return 99;
}

#ifndef _WIN32
static int RunSelfChild(const char* name) {
  const std::string cmd = std::string(baml_test::Argv0()) + " --child " + name;
  const int status = std::system(cmd.c_str());
  BAML_ASSERT(status != -1 && WIFEXITED(status));
  return WEXITSTATUS(status);
}

BAML_TEST(clean_exit_terminates_process_with_code) {
  BAML_ASSERT_EQ(RunSelfChild("do_exit_0"), 0);
  BAML_ASSERT_EQ(RunSelfChild("do_exit_7"), 7);
}
#endif
