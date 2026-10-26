# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class RecursionTest < Minitest::Test
  def test_recursion_round_trip_int_binary_tree
    t = BamlSdk::Recursion::IntBinaryTree.new(value: 1, left: BamlSdk::Recursion::IntBinaryTree.new(value: 2, left: nil, right: nil), right: nil)
    assert_equal t, BamlSdk::Recursion.round_trip_int_binary_tree(t)
  end

  def test_recursion_round_trip_mutual_recursion
    a = BamlSdk::Recursion::A.new(b: BamlSdk::Recursion::B.new(a: nil))
    b = BamlSdk::Recursion::B.new(a: BamlSdk::Recursion::A.new(b: nil))
    assert_equal a, BamlSdk::Recursion.round_trip_a(a)
    assert_equal b, BamlSdk::Recursion.round_trip_b(b)
  end

  def test_recursion_round_trip_scc_t1_t2_t3
    t1 = BamlSdk::Recursion::T1.new(via2: BamlSdk::Recursion::T2.new(via1: nil, via3: nil), via3: nil)
    t2 = BamlSdk::Recursion::T2.new(via1: nil, via3: BamlSdk::Recursion::T3.new(via1: nil, via2: nil))
    t3 = BamlSdk::Recursion::T3.new(via1: nil, via2: nil)
    assert_equal t1, BamlSdk::Recursion.round_trip_t1(t1)
    assert_equal t2, BamlSdk::Recursion.round_trip_t2(t2)
    assert_equal t3, BamlSdk::Recursion.round_trip_t3(t3)
  end

  def test_recursion_round_trip_scc_t4_t5_t6
    t4 = BamlSdk::Recursion::T4.new(via5: BamlSdk::Recursion::T5.new(via4: nil, via6: nil), via6: nil)
    t5 = BamlSdk::Recursion::T5.new(via4: nil, via6: BamlSdk::Recursion::T6.new(via4: nil, via5: nil))
    t6 = BamlSdk::Recursion::T6.new(via4: nil, via5: nil)
    assert_equal t4, BamlSdk::Recursion.round_trip_t4(t4)
    assert_equal t5, BamlSdk::Recursion.round_trip_t5(t5)
    assert_equal t6, BamlSdk::Recursion.round_trip_t6(t6)
  end

end
