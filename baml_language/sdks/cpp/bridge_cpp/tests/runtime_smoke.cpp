// Bridge-core smoke test: exercises the header-only runtime layer against the
// real cdylib (version, runtime init, call registry fan-out, Arg tri-state).
// Function calls through the codec are covered separately once the codec lands.
#include <cassert>
#include <chrono>
#include <cstdio>
#include <map>
#include <string>
#include <type_traits>
#include <vector>

#include <baml/baml.hpp>

static void test_version() {
    const std::string v = baml::version();
    assert(!v.empty());
    std::printf("version: %s\n", v.c_str());
}

static void test_initialize_runtime() {
    const std::map<std::string, std::string> files = {
        {"main.baml", "function ReturnOne() -> int {\n  1\n}\n"},
    };
    baml::initialize_runtime(".", files);
    std::printf("runtime initialized\n");
}

static void test_call_registry_round_trip() {
    auto started = baml::detail::CallRegistry::instance().begin();
    const std::vector<int8_t> payload = {1, 2, 3, 4};
    baml_cpp_result_trampoline(started.correlation_id, payload.data(), payload.size());
    auto status = started.envelope.wait_for(std::chrono::seconds(1));
    assert(status == std::future_status::ready);
    const std::vector<uint8_t> got = started.envelope.get();
    assert(got.size() == 4 && got[0] == 1 && got[3] == 4);
    std::printf("call registry round trip ok\n");
}

static void test_arg_tri_state() {
    baml::Arg<int64_t> unset;
    assert(unset.is_unset() && !unset.is_null() && !unset.has_value());

    baml::Arg<int64_t> null_arg = std::nullopt;
    assert(null_arg.is_null());

    static_assert(std::is_same<baml::Null, std::monostate>::value,
                  "bare null unifies with the std vocabulary");

    baml::Arg<int64_t> value_arg = int64_t{42};
    assert(value_arg.has_value() && value_arg.value() == 42);

    bool threw = false;
    try {
        (void)unset.value();
    } catch (const std::logic_error&) {
        threw = true;
    }
    assert(threw);
    std::printf("arg tri-state ok\n");
}

static void test_owned_buffer_move() {
    baml::detail::OwnedBuffer a{::version()};
    assert(!a.empty());
    baml::detail::OwnedBuffer b = std::move(a);
    assert(a.empty() && !b.empty());
    std::printf("owned buffer move ok\n");
}

int main() {
    test_version();
    test_initialize_runtime();
    test_call_registry_round_trip();
    test_arg_tri_state();
    test_owned_buffer_move();
    std::printf("bridge core smoke: all ok\n");
    return 0;
}
