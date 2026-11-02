// Tuples as stream items, offline: `tuple_streams` scripts each function's
// client to replay fixed text deltas, so the generated stream companions run
// the ordinary partial/final parse with no replay server.
//
//   * `stream_tuple_items` -- `(string, int)[]`: partials are vectors of
//     std::tuple, `[("a", 1)]` first; the final value is
//     `[("a", 1), ("b", 2)]`.
//   * `stream_tuple_final` -- `(int, string)`: every partial is itself a
//     std::tuple, and so is the final value `(1, "a")`.
#include <baml_sdk.h>
#include <baml_test.h>

#include <cstddef>
#include <string>
#include <tuple>
#include <type_traits>
#include <vector>

namespace streams = baml_sdk::tuple_streams;

namespace {

using Item = std::tuple<std::string, int64_t>;
using Items = std::vector<Item>;
using Final = std::tuple<int64_t, std::string>;

static_assert(std::is_same<decltype(streams::stream_tuple_items_stream(
                               std::declval<const std::string&>())),
                           baml::stream<Items>>::value,
              "a tuple-list stream streams vectors of std::tuple");
static_assert(std::is_same<decltype(streams::stream_tuple_final_stream(
                               std::declval<const std::string&>())),
                           baml::stream<Final>>::value,
              "a tuple stream streams std::tuple partials");

template <typename T>
std::vector<T> drain(const baml::stream<T>& stream) {
  std::vector<T> partials;
  for (;;) {
    baml::stream_item<T> item = stream.next();
    if (item.done()) return partials;
    partials.push_back(std::move(item.value()));
    BAML_ASSERT(partials.size() < 10000);
  }
}

template <typename T>
std::vector<T> drain_async(const baml::stream<T>& stream) {
  std::vector<T> partials;
  for (;;) {
    baml::stream_item<T> item = stream.next_async().get();
    if (item.done()) return partials;
    partials.push_back(std::move(item.value()));
    BAML_ASSERT(partials.size() < 10000);
  }
}

const Items kItems{Item("a", 1), Item("b", 2)};

void assert_items_partials(const std::vector<Items>& partials) {
  BAML_ASSERT(!partials.empty());
  BAML_ASSERT(partials.front() == Items{Item("a", 1)});
  for (const Items& partial : partials) {
    // Each partial is a prefix of the final value.
    BAML_ASSERT(partial.size() <= kItems.size());
    BAML_ASSERT(Items(kItems.begin(), kItems.begin() +
                                          static_cast<std::ptrdiff_t>(
                                              partial.size())) == partial);
  }
}

void assert_final_partials(const std::vector<Final>& partials) {
  BAML_ASSERT(!partials.empty());
  for (const Final& partial : partials) {
    BAML_ASSERT(std::get<0>(partial) == 1);
    BAML_ASSERT(std::string("a").rfind(std::get<1>(partial), 0) == 0);
  }
  BAML_ASSERT(partials.back() == Final(1, "a"));
}

}  // namespace

BAML_TEST(tuples_stream_items) {
  const baml::stream<Items> stream = streams::stream_tuple_items_stream("x");
  assert_items_partials(drain(stream));
  BAML_ASSERT(stream.final_() == kItems);
  // The non-streaming call parses the same reply to the same value.
  BAML_ASSERT(streams::stream_tuple_items("x") == kItems);

  const baml::stream<Items> async_stream =
      streams::stream_tuple_items_stream_async("x").get();
  assert_items_partials(drain_async(async_stream));
  BAML_ASSERT(async_stream.final_async().get() == kItems);
}

BAML_TEST(tuples_stream_final) {
  const baml::stream<Final> stream = streams::stream_tuple_final_stream("x");
  assert_final_partials(drain(stream));
  BAML_ASSERT(stream.final_() == Final(1, "a"));
  BAML_ASSERT(streams::stream_tuple_final("x") == Final(1, "a"));

  const baml::stream<Final> async_stream =
      streams::stream_tuple_final_stream_async("x").get();
  assert_final_partials(drain_async(async_stream));
  BAML_ASSERT(async_stream.final_async().get() == Final(1, "a"));
  BAML_ASSERT(streams::stream_tuple_final_async("x").get() == Final(1, "a"));
}
