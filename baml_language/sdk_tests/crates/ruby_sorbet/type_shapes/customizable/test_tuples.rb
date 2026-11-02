# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

# Ruby has no tuple type: a tuple is a frozen Array, and any Array of the
# right length and item types fills a tuple slot.
class TuplesTest < Minitest::Test
  Tuples = BamlSdk::Tuples

  def test_tuples_round_trip_pair
    result = Tuples.round_trip_pair([1, "a"])
    assert_equal [1, "a"], result
    assert_predicate result, :frozen?
  end

  def test_tuples_round_trip_single_and_repeated
    assert_equal ["x"], Tuples.round_trip_single(["x"])
    assert_equal [1, 2, 3], Tuples.round_trip_repeated([1, 2, 3])
  end

  def test_tuples_round_trip_nested
    assert_equal [[1, "a"], true], Tuples.round_trip_nested([[1, "a"], true])
  end

  def test_tuples_round_trip_optional_slot_and_optional_tuple
    assert_equal [1, nil], Tuples.round_trip_optional_slot([1, nil])
    assert_nil Tuples.round_trip_optional_tuple(nil)
    assert_equal [2, "b"], Tuples.round_trip_optional_tuple([2, "b"])
  end

  def test_tuples_round_trip_wide
    wide = Array.new(13) { |index| index.even? ? index : index.to_s }
    assert_equal wide, Tuples.round_trip_wide(wide)
  end

  def test_tuples_round_trip_pair_async
    assert_equal [1, "a"], Tuples.round_trip_pair_async([1, "a"]).wait
  end

  def test_tuples_plain_list_stands_in_at_a_tuple_slot
    assert_equal [1, "a"], Tuples.round_trip_pair([1, "a"].dup)
  end

  def test_tuples_wrong_arity_and_slots_rejected
    [[1], [1, "a", 2], ["a", 1]].each do |wrong|
      assert_raises(TypeError) { Tuples.round_trip_pair(T.unsafe(wrong)) }
    end
  end

  def test_tuples_round_trip_containers
    assert_equal [["a", 1], ["b", 2]], Tuples.round_trip_tuple_list([["a", 1], ["b", 2]])
    assert_equal({ "k" => [1, true] }, Tuples.round_trip_tuple_map({ "k" => [1, true] }))
  end

  def test_tuples_round_trip_union_arms
    assert_equal [1, "a"], Tuples.round_trip_tuple_or_string([1, "a"])
    assert_equal "s", Tuples.round_trip_tuple_or_string("s")
    assert_equal [1, "a"], Tuples.round_trip_tuple_arms([1, "a"])
    assert_equal ["a", 1], Tuples.round_trip_tuple_arms(["a", 1])
  end

  def test_tuples_array_fitting_tuple_and_list_is_tuple
    assert_equal true, Tuples.tuple_or_list_is_tuple([1, 2])
    assert_equal false, Tuples.tuple_or_list_is_tuple([1, 2, 3])
  end

  def test_tuples_round_trip_class_fields
    holder = Tuples::TupleHolder.new(pair: [1, "a"], nested: [[2, "b"], false], maybe: [3, nil], pairs: [["c", 4]])
    assert_equal holder, Tuples.round_trip_holder(holder)
  end

  def test_tuples_make_swap_and_index
    assert_equal [1, "a"], Tuples.make_pair(1, "a")
    assert_equal ["a", 1], Tuples.swap_pair([1, "a"])
    assert_equal 7, Tuples.pair_first([7, "a"])
    assert_equal [2, "b"], Tuples.round_trip_pair(Tuples.make_pair(2, "b"))
  end

  def test_tuples_method_round_trip
    box = Tuples::TuplePairBox.new(a: 1, b: "a")
    assert_equal [1, "a"], box.as_pair
    assert_predicate box.as_pair, :frozen?
    assert_equal Tuples::TuplePairBox.new(a: 2, b: "b"), box.with_pair([2, "b"])
    assert_equal Tuples::TuplePairBox.new(a: 3, b: "c"), Tuples::TuplePairBox.from_pair([3, "c"])
    assert_equal [1, "a"], box.as_pair_async.wait
    assert_equal Tuples::TuplePairBox.new(a: 5, b: "e"), Tuples::TuplePairBox.from_pair_async([5, "e"]).wait
  end
end
