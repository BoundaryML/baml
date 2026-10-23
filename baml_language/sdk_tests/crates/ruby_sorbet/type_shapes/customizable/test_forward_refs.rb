# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class ForwardRefsTest < Minitest::Test
  def test_forward_refs_round_trip_other
    # Node's required self-reference has no finite host value; check reachability.
    assert_operator BamlSdk::ForwardRefs::Node, :<, T::Struct
    assert_respond_to BamlSdk::ForwardRefs, :round_trip_node
    o = BamlSdk::ForwardRefs::Other.new(v: 7)
    assert_equal o, BamlSdk::ForwardRefs.round_trip_other(o)
  end

  def test_forward_refs_round_trip_rec_list
    assert_equal [1, [2, 3]], BamlSdk::ForwardRefs.round_trip_rec_list([1, [2, 3]])
  end

  def test_forward_refs_round_trip_rec_list_with_other
    assert_equal 1, BamlSdk::ForwardRefs.round_trip_rec_list_with_other(1)
    assert_equal [1, 2], BamlSdk::ForwardRefs.round_trip_rec_list_with_other([1, 2])
  end

end
