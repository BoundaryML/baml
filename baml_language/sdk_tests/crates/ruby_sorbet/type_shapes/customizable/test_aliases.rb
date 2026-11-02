# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class AliasesTest < Minitest::Test
  def test_aliases_round_trip_string_list
    assert_equal ["a", "b"], BamlSdk::Aliases.round_trip_string_list(["a", "b"])
  end

  def test_aliases_round_trip_rec_list
    assert_equal 1, BamlSdk::Aliases.round_trip_rec_list(1)
    assert_equal [1, [2, 3]], BamlSdk::Aliases.round_trip_rec_list([1, [2, 3]])
  end

  def test_aliases_round_trip_alias_container
    c = BamlSdk::Aliases::AliasContainer.new(list_field: ["x"], rec_field: [1, [2]])
    assert_equal c, BamlSdk::Aliases.round_trip_alias_container(c)
  end

end
