# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class RoutingTest < Minitest::Test
  def test_routing_make_foo
    assert_equal 3, BamlSdk.make_foo(3).v
  end

  def test_routing_round_trip_foo
    f = BamlSdk::Foo.new(v: 10)
    assert_equal f, BamlSdk.round_trip_foo(f)
  end

  def test_routing_round_trip_thing_from_ab
    t = BamlSdk::A::B::Thing.new(v: 1)
    assert_equal t, BamlSdk::A::B.round_trip_thing_from_ab(t)
  end

  def test_routing_round_trip_root_foo_from_ab
    f = BamlSdk::Foo.new(v: 2)
    assert_equal f, BamlSdk::A::B.round_trip_root_foo_from_ab(f)
  end

  def test_routing_round_trip_deep_thing_from_a
    t = BamlSdk::A::B::Thing.new(v: 4)
    assert_equal t, BamlSdk::A.round_trip_deep_thing_from_a(t)
  end

  def test_routing_round_trip_deep_thing_from_lorem
    t = BamlSdk::A::B::Thing.new(v: 5)
    assert_equal t, BamlSdk::Lorem.round_trip_deep_thing_from_lorem(t)
  end

  def test_routing_round_trip_resume
    r = BamlSdk::Lorem::Resume.new(name: "ada", email: nil)
    assert_equal r, BamlSdk::Lorem.round_trip_resume(r)
  end

  def test_routing_round_trip_root_foo
    f = BamlSdk::Foo.new(v: 6)
    assert_equal f, BamlSdk::Lorem.round_trip_root_foo(f)
  end

  def test_routing_round_trip_lorem_resume_from_ipsum
    r = BamlSdk::Lorem::Resume.new(name: "grace", email: "g@x.com")
    assert_equal r, BamlSdk::Ipsum.round_trip_lorem_resume_from_ipsum(r)
  end

  def test_routing_round_trip_resume_or_http_response
    r = BamlSdk::Lorem::Resume.new(name: "lovelace", email: "a@x.com")
    assert_equal r, BamlSdk::Lorem.round_trip_resume_or_http_response(r)
  end

end
