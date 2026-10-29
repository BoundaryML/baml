// Instances the runtime keeps to itself (`baml.hash.DefaultHasher`) cross as
// references: one passed back is the same instance, wherever a class can sit.
// Its state shows which instance a call reached: one hasher fed "a" and then
// "b" hashes as a fresh one fed "ab".
#include <baml_sdk.h>
#include <baml_test.h>

#include <string>
#include <variant>
#include <vector>

namespace references = baml_sdk::references;
using baml_sdk::baml::hash::DefaultHasher;
using references::HasherMarker;

BAML_TEST(references_a_returned_instance_passed_back_is_the_same_instance) {
  const DefaultHasher hasher = references::make_hasher();
  references::hash_after(hasher, "a");
  const int64_t fed_twice = references::hash_after(hasher, "b");
  BAML_ASSERT_EQ(fed_twice,
                 references::hash_after(references::make_hasher(), "ab"));
  // Negative control: a fresh hasher fed only "b" differs.
  BAML_ASSERT(fed_twice !=
              references::hash_after(references::make_hasher(), "b"));
}

BAML_TEST(references_one_engine_instance_is_one_reference) {
  const DefaultHasher hasher = references::make_hasher();
  BAML_ASSERT(references::same_hasher(hasher) == hasher);
  BAML_ASSERT(references::make_hasher() != hasher);
}

BAML_TEST(references_containers_hold_the_instance) {
  const DefaultHasher hasher = references::make_hasher();
  BAML_ASSERT(references::maybe_hasher(true).has_value());
  BAML_ASSERT(!references::maybe_hasher(false).has_value());
  const std::vector<DefaultHasher> twice = references::hasher_twice(hasher);
  BAML_ASSERT_EQ(twice.size(), std::size_t{2});
  BAML_ASSERT(twice[0] == hasher && twice[1] == hasher);
  const auto under = references::hasher_under("key", hasher);
  BAML_ASSERT_EQ(under.size(), std::size_t{1});
  BAML_ASSERT(under.at("key") == hasher);
}

BAML_TEST(references_a_field_holds_the_instance) {
  const DefaultHasher hasher = references::make_hasher();
  const references::HasherHolder holder =
      references::hold_hasher("label", hasher);
  BAML_ASSERT_EQ(holder.label, std::string("label"));
  BAML_ASSERT(holder.hasher == hasher);
}

BAML_TEST(references_a_union_selects_the_instance_arm) {
  BAML_ASSERT(std::holds_alternative<DefaultHasher>(
      references::hasher_or_marker(true)));
  const auto marker = references::hasher_or_marker(false);
  BAML_ASSERT(std::holds_alternative<HasherMarker>(marker));
  BAML_ASSERT_EQ(std::get<HasherMarker>(marker).tag, std::string("marker"));
  BAML_ASSERT_EQ(
      references::describe_hasher_or_marker(references::make_hasher()),
      std::string("hasher"));
  BAML_ASSERT_EQ(references::describe_hasher_or_marker(HasherMarker{"marker"}),
                 std::string("marker"));
}

BAML_TEST(references_a_callback_receives_and_returns_the_instance) {
  const DefaultHasher hasher = references::make_hasher();
  std::vector<DefaultHasher> received;
  const DefaultHasher returned = references::hasher_through_callback(
      hasher, [&received](DefaultHasher value) {
        received.push_back(value);
        return value;
      });
  BAML_ASSERT(returned == hasher);
  BAML_ASSERT_EQ(received.size(), std::size_t{1});
  BAML_ASSERT(received[0] == hasher);
}
