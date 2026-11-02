# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class LiteralsTest < Minitest::Test
  def test_literals_return_literals
    assert_equal 42, BamlSdk::Literals.return_literal42()
    assert_equal -1, BamlSdk::Literals.return_literal_neg_one()
    assert_equal "draft", BamlSdk::Literals.return_literal_draft()
    assert_equal "has \"quotes\"", BamlSdk::Literals.return_literal_escaped()
    assert_equal true, BamlSdk::Literals.return_literal_true()
    assert_equal false, BamlSdk::Literals.return_literal_false()
  end

  def test_literals_round_trip_literal42
    assert_equal 42, BamlSdk::Literals.round_trip_literal42(42)
  end

  def test_literals_round_trip_literal_draft
    assert_equal "draft", BamlSdk::Literals.round_trip_literal_draft("draft")
  end

  def test_literals_round_trip_literal_escaped
    assert_equal "has \"quotes\"", BamlSdk::Literals.round_trip_literal_escaped("has \"quotes\"")
  end

  def test_literals_round_trip_literal_true
    assert_equal true, BamlSdk::Literals.round_trip_literal_true(true)
  end

  def test_literals_round_trip_literal_false
    assert_equal false, BamlSdk::Literals.round_trip_literal_false(false)
  end

  def test_literals_round_trip_literals
    lit = BamlSdk::Literals::Literals.new(literal_42: 42, literal_draft: "draft", literal_escaped: "has \"quotes\"", literal_true: true, literal_false: false)
    assert_equal lit, BamlSdk::Literals.round_trip_literals(lit)
  end

end
