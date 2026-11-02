// Roundtrip coverage for baml_sdk::tuples.
//
// A BAML tuple is a native std::tuple in every position: slots in
// declaration order with repeats kept (unlike baml::variant, nothing is
// sorted or merged), identity by spelling, every arity representable.
#include <baml_sdk.h>
#include <baml_test.h>

#include <string>
#include <tuple>
#include <type_traits>
#include <unordered_map>
#include <vector>

namespace tuples = baml_sdk::tuples;
using Pair = std::tuple<int64_t, std::string>;

// Slot order is identity: the swap returns the other C++ type.
static_assert(
    std::is_same<decltype(tuples::swap_pair(std::declval<const Pair&>())),
                 std::tuple<std::string, int64_t>>::value,
    "(string, int) is std::tuple<std::string, int64_t>");
static_assert(
    std::is_same<
        decltype(tuples::round_trip_repeated(
            std::declval<const std::tuple<int64_t, int64_t, int64_t>&>())),
        std::tuple<int64_t, int64_t, int64_t>>::value,
    "repeated slot types keep every slot");

// SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
BAML_TEST(tuples_round_trip_pair) {
  const Pair pair{1, "a"};
  BAML_ASSERT(tuples::round_trip_pair(pair) == pair);
}

// SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
BAML_TEST(tuples_round_trip_pair_async) {
  const Pair pair{2, "b"};
  BAML_ASSERT(tuples::round_trip_pair_async(pair).get() == pair);
}

// SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
BAML_TEST(tuples_make_swap_and_index) {
  BAML_ASSERT(tuples::make_pair(3, "c") == Pair(3, "c"));
  BAML_ASSERT(tuples::swap_pair(Pair(4, "d")) ==
              std::make_tuple(std::string("d"), int64_t{4}));
  BAML_ASSERT_EQ(tuples::pair_first(Pair(5, "e")), int64_t{5});
}

// SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
BAML_TEST(tuples_round_trip_single_and_repeated) {
  const std::tuple<std::string> single{"x"};
  BAML_ASSERT(tuples::round_trip_single(single) == single);
  const std::tuple<int64_t, int64_t, int64_t> repeated{1, 2, 3};
  BAML_ASSERT(tuples::round_trip_repeated(repeated) == repeated);
}

// SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
BAML_TEST(tuples_round_trip_nested) {
  const std::tuple<Pair, bool> nested{Pair(1, "inner"), true};
  BAML_ASSERT(tuples::round_trip_nested(nested) == nested);
}

// SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
BAML_TEST(tuples_round_trip_optional_slot_and_optional_tuple) {
  // An optional slot still occupies its position.
  using OptSlot = std::tuple<int64_t, std::optional<std::string>>;
  BAML_ASSERT(tuples::round_trip_optional_slot(OptSlot(1, "s")) ==
              OptSlot(1, "s"));
  BAML_ASSERT(tuples::round_trip_optional_slot(OptSlot(1, std::nullopt)) ==
              OptSlot(1, std::nullopt));
  BAML_ASSERT(tuples::round_trip_optional_tuple(Pair(1, "a")) ==
              std::optional<Pair>(Pair(1, "a")));
  BAML_ASSERT(!tuples::round_trip_optional_tuple(std::nullopt).has_value());
}

// SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
BAML_TEST(tuples_round_trip_containers) {
  const std::vector<std::tuple<std::string, int64_t>> xs{{"a", 1}, {"b", 2}};
  BAML_ASSERT(tuples::round_trip_tuple_list(xs) == xs);
  BAML_ASSERT(tuples::round_trip_tuple_list({}).empty());
  const std::unordered_map<std::string, std::tuple<int64_t, bool>> m{
      {"k", {1, true}}};
  BAML_ASSERT(tuples::round_trip_tuple_map(m) == m);
}

// SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
BAML_TEST(tuples_round_trip_union_arms) {
  using TupleOrString = baml::variant<Pair, std::string>;
  const TupleOrString tuple{Pair(1, "a")};
  const TupleOrString string{std::string("s")};
  BAML_ASSERT(tuples::round_trip_tuple_or_string(tuple) == tuple);
  BAML_ASSERT(tuples::round_trip_tuple_or_string(string) == string);
  // Two same-arity tuple arms: the engine's selection picks the arm.
  using Arms = baml::variant<Pair, std::tuple<std::string, int64_t>>;
  const Arms first{Pair(1, "a")};
  const Arms second{std::make_tuple(std::string("a"), int64_t{1})};
  BAML_ASSERT(tuples::round_trip_tuple_arms(first) == first);
  BAML_ASSERT(tuples::round_trip_tuple_arms(second) == second);
}

// SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
BAML_TEST(tuples_round_trip_class_fields) {
  tuples::TupleHolder holder;
  holder.pair = Pair(1, "a");
  holder.nested = std::make_tuple(Pair(2, "b"), false);
  holder.maybe = std::make_tuple(int64_t{3}, std::optional<std::string>());
  holder.pairs = {std::make_tuple(std::string("c"), int64_t{4})};
  BAML_ASSERT(tuples::round_trip_holder(holder) == holder);
}

// SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
BAML_TEST(tuples_host_callback_args_and_results) {
  const auto result = tuples::call_tuple_callback(
      [](const Pair& t) {
        return std::make_tuple(std::get<1>(t), std::get<0>(t) * 10);
      },
      Pair(4, "four"));
  BAML_ASSERT(result == std::make_tuple(std::string("four"), int64_t{40}));
}

// SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
BAML_TEST(tuples_thrown_value) {
  bool caught = false;
  try {
    (void)tuples::throw_pair(Pair(7, "boom"));
  } catch (const baml::thrown<baml::variant<Pair>>& error) {
    caught = true;
    BAML_ASSERT(std::get<Pair>(error.value) == Pair(7, "boom"));
  }
  BAML_ASSERT(caught);
}

// SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
BAML_TEST(tuples_round_trip_wide) {
  // Thirteen slots: still a native std::tuple.
  const auto wide = std::make_tuple(
      int64_t{0}, std::string("s1"), int64_t{2}, std::string("s3"), int64_t{4},
      std::string("s5"), int64_t{6}, std::string("s7"), int64_t{8},
      std::string("s9"), int64_t{10}, std::string("s11"), int64_t{12});
  BAML_ASSERT(tuples::round_trip_wide(wide) == wide);
}

// SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
BAML_TEST(tuples_dynamic_value_decodes_as_a_tuple) {
  const baml::value result = baml::invoke(
      baml::target("user.tuples.make_pair"),
      {{"n", baml::input(int64_t{8})}, {"s", baml::input(std::string("h"))}},
      {});
  BAML_ASSERT(result.as<Pair>() == Pair(8, "h"));
  // A tuple never decodes as a list, whatever its length.
  bool rejected = false;
  try {
    (void)result.as<std::vector<std::string>>();
  } catch (const baml::error&) {
    rejected = true;
  }
  BAML_ASSERT(rejected);
}

// SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
BAML_TEST(tuples_dynamic_value_keeps_carried_type) {
  // A received tuple sent back as an argument states the tuple type it
  // carries, so the engine takes it as that tuple wherever it fits.
  const baml::value pair = baml::invoke(
      baml::target("user.tuples.make_pair"),
      {{"n", baml::input(int64_t{9})}, {"s", baml::input(std::string("i"))}},
      {});
  const baml::value echoed =
      baml::invoke(baml::target("user.tuples.round_trip_pair"),
                   {{"t", baml::input(pair)}}, {});
  BAML_ASSERT(echoed.as<Pair>() == Pair(9, "i"));
  // Under two same-arity tuple arms it is the arm of its own type.
  using Arms = baml::variant<Pair, std::tuple<std::string, int64_t>>;
  const baml::value selected =
      baml::invoke(baml::target("user.tuples.round_trip_tuple_arms"),
                   {{"u", baml::input(pair)}}, {});
  BAML_ASSERT(selected.as<Arms>() == Arms(Pair(9, "i")));
  // It stays a tuple: where a list is declared, it is refused.
  bool rejected = false;
  try {
    (void)baml::invoke(baml::target("user.tuples.round_trip_tuple_list"),
                       {{"xs", baml::input(pair)}}, {});
  } catch (const baml::error&) {
    rejected = true;
  }
  BAML_ASSERT(rejected);
}

// SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
BAML_TEST(tuples_callback_forwards_a_thrown_tuple_under_its_carried_type) {
  // The callback lets a nested call's thrown tuple through: the bridge
  // restates it under the tuple type it carries, and it comes back out of
  // the outer call as the same tuple.
  bool caught = false;
  try {
    (void)tuples::call_tuple_callback(
        [](const Pair& t) {
          (void)tuples::throw_pair(t);
          return std::make_tuple(std::get<1>(t), std::get<0>(t));
        },
        Pair(6, "forwarded"));
  } catch (const baml::error& error) {
    caught = true;
    BAML_ASSERT(error.get<Pair>() == Pair(6, "forwarded"));
  }
  BAML_ASSERT(caught);
}

// SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
BAML_TEST(tuples_wrong_arity_and_slots_rejected) {
  // `(string, int)` where `(int, string)` is declared: the engine checks
  // each slot, so the swapped tuple is refused.
  bool rejected = false;
  try {
    (void)baml::invoke(
        baml::target("user.tuples.round_trip_pair"),
        {{"t", baml::input(std::make_tuple(std::string("a"), int64_t{1}))}},
        {});
  } catch (const baml::error&) {
    rejected = true;
  }
  BAML_ASSERT(rejected);
  // A one-slot tuple where a pair is declared is refused too.
  rejected = false;
  try {
    (void)baml::invoke(baml::target("user.tuples.round_trip_pair"),
                       {{"t", baml::input(std::make_tuple(int64_t{1}))}}, {});
  } catch (const baml::error&) {
    rejected = true;
  }
  BAML_ASSERT(rejected);
}
