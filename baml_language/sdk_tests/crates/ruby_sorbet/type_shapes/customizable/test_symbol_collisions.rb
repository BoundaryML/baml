# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class SymbolCollisionsTest < Minitest::Test
  def test_symbol_collisions_round_trip_foo_bar
    bar = BamlSdk::SymbolCollisions::Foo.make_foo_bar("hi", 2)
    assert_equal bar, BamlSdk::SymbolCollisions::Foo.round_trip_foo_bar(bar)
  end

  def test_symbol_collisions_round_trip_fizz_foo_bar
    bar = BamlSdk::SymbolCollisions::Fizz::Foo.make_fizz_foo_bar("t", 1.5)
    assert_equal bar, BamlSdk::SymbolCollisions::Fizz::Foo.round_trip_fizz_foo_bar(bar)
  end

  def test_symbol_collisions_round_trip_fizz_buzz_foo_bar
    bar = BamlSdk::SymbolCollisions::Fizz::Buzz::Foo.make_fizz_buzz_foo_bar("f", 2.5, true)
    assert_equal bar, BamlSdk::SymbolCollisions::Fizz::Buzz::Foo.round_trip_fizz_buzz_foo_bar(bar)
  end

  def test_symbol_collisions_round_trip_ipsum
    ipsum = BamlSdk::SymbolCollisions::Lorem.make_ipsum(BamlSdk::SymbolCollisions::Foo.make_foo_bar("a", 1), BamlSdk::SymbolCollisions::Fizz::Foo.make_fizz_foo_bar("b", 2.0), BamlSdk::SymbolCollisions::Fizz::Buzz::Foo.make_fizz_buzz_foo_bar("c", 3.0, false))
    assert_equal ipsum, BamlSdk::SymbolCollisions::Lorem.round_trip_ipsum(ipsum)
  end

  def test_symbol_collisions_round_trip_deep
    ipsum = BamlSdk::SymbolCollisions::Lorem.make_ipsum(BamlSdk::SymbolCollisions::Foo.make_foo_bar("a", 1), BamlSdk::SymbolCollisions::Fizz::Foo.make_fizz_foo_bar("b", 2.0), BamlSdk::SymbolCollisions::Fizz::Buzz::Foo.make_fizz_buzz_foo_bar("c", 3.0, false))
    deep = BamlSdk::SymbolCollisions::A::B::C::D.make_deep(BamlSdk::SymbolCollisions::Foo.make_foo_bar("h", 9), BamlSdk::SymbolCollisions::Fizz::Foo.make_fizz_foo_bar("th", 4.0), BamlSdk::SymbolCollisions::Fizz::Buzz::Foo.make_fizz_buzz_foo_bar("fu", 5.0, true), ipsum)
    assert_equal deep, BamlSdk::SymbolCollisions::A::B::C::D.round_trip_deep(deep)
  end

end
