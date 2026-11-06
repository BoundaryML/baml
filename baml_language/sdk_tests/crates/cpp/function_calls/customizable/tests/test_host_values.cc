// End-to-end tests for a host's own value (`baml.host.Value`): the bridge
// registers it, BAML holds it by key, and it comes back as the same value.
#include <baml_sdk.h>
#include <baml_test.h>

#include <any>
#include <array>
#include <chrono>
#include <cstdint>
#include <functional>
#include <string>
#include <vector>

namespace {

baml::detail::host_value_registry& registry() {
  return baml::detail::host_value_registry::instance();
}

// The key `value` is registered under, taking a reference the caller gives
// back with `release_input_value` on the returned message.
baml::detail::pb::InboundValue probe(const baml::host_value& value) {
  baml::detail::pb::InboundValue sent;
  baml::codec<baml::host_value>::encode(sent, value);
  return sent;
}

// Collects until the engine has given back every reference to `key`; false
// if it still holds one after a few seconds.
bool released_by_the_engine(uint64_t key) {
  const auto deadline =
      std::chrono::steady_clock::now() + std::chrono::seconds(5);
  do {
    baml_sdk::baml::sys::collect_garbage();
    if (registry().count(key) == 0) return true;
  } while (std::chrono::steady_clock::now() < deadline);
  return false;
}

}  // namespace

BAML_TEST(host_values_round_trip_as_the_same_value) {
  const baml::host_value sent(std::string("mine"));
  const baml::host_value back =
      baml_sdk::host_value_tests::echo_host_value(sent);
  BAML_ASSERT_EQ(std::any_cast<std::string>(back.value()), std::string("mine"));
  BAML_ASSERT(back == sent);
  // Negative control: another host value with equal contents is another
  // value.
  BAML_ASSERT(!(back == baml::host_value(std::string("mine"))));
}

BAML_TEST(host_values_come_back_inside_a_list) {
  const baml::host_value sent(7);
  const std::vector<baml::host_value> back =
      baml_sdk::host_value_tests::twice_host_value(sent);
  BAML_ASSERT_EQ(back.size(), std::size_t{2});
  for (const auto& value : back) BAML_ASSERT(value == sent);
}

BAML_TEST(host_values_sent_twice_are_one_value_to_baml) {
  const baml::host_value sent(3);
  BAML_ASSERT(baml_sdk::host_value_tests::same_host_value(sent, sent));
  // Negative control: two host values are two values.
  BAML_ASSERT(
      !baml_sdk::host_value_tests::same_host_value(sent, baml::host_value(3)));
}

// A host value the engine sends back decodes even when a collection ran
// between the result and the decode: its pin keeps the engine's reference.
// Once decoded, the pin is released and the engine gives the key back.
BAML_TEST(host_value_back_decodes_after_baml_collects_garbage) {
  const baml::host_value sent(std::string("pinned"));
  baml::detail::pb::InboundValue held = probe(sent);
  const uint64_t key = held.handle().key();
  auto pending = baml_sdk::host_value_tests::echo_host_value_async(sent);
  pending.wait();
  // Only the engine's references are left; the collection drops all but
  // the result's pin.
  baml::detail::release_input_value(held);
  baml_sdk::baml::sys::collect_garbage();
  const baml::host_value back = pending.get();
  BAML_ASSERT(back == sent);
  // Sent again under the key it came back with, and balanced after.
  BAML_ASSERT(baml_sdk::host_value_tests::echo_host_value(back) == sent);
  BAML_ASSERT(released_by_the_engine(key));
  BAML_ASSERT(!registry().find_value(key));
  // Still the host's value: sent again, it registers anew.
  BAML_ASSERT(baml_sdk::host_value_tests::echo_host_value(sent) == back);
}

// A call that never starts gives back exactly the references its arguments
// took: a callable registered for it is gone, a value held elsewhere keeps
// the rest.
BAML_TEST(host_values_of_a_rolled_back_call_give_back_only_its_references) {
  const baml::host_value value(std::string("held"));
  baml::detail::pb::InboundValue held = probe(value);
  const uint64_t key = held.handle().key();
  uint64_t callable_key = 0;
  {
    baml::detail::args_encoder args;
    args.add_arg("value", [&](baml::detail::pb::InboundValue& out) {
      baml::codec<baml::host_value>::encode(out, value);
    });
    baml::detail::pb::BamlOutboundValue received;
    received.mutable_handle_value()->set_key(key);
    received.mutable_handle_value()->set_handle_type(
        baml::detail::pb::HOST_VALUE_OPAQUE);
    args.add_arg("again", [&](baml::detail::pb::InboundValue& out) {
      baml::detail::transcode_outbound_to_inbound(out, received);
    });
    args.add_arg("callable", [&](baml::detail::pb::InboundValue& out) {
      baml::detail::encode_callable(
          out, std::function<int64_t(int64_t)>([](int64_t x) { return x; }),
          std::array<std::string, 1>{{""}}, baml::detail::unknown_baml_ty());
      callable_key = out.handle().key();
    });
    BAML_ASSERT_EQ(registry().count(key), uint64_t{3});
    BAML_ASSERT_EQ(registry().count(callable_key), uint64_t{1});
  }
  BAML_ASSERT_EQ(registry().count(key), uint64_t{1});
  BAML_ASSERT(!registry().find_callable(callable_key));
  baml::detail::release_input_value(held);
  BAML_ASSERT(!registry().find_value(key));
}
