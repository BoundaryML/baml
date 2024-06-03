require 'minitest/autorun'
require 'minitest/reporters'
require "sorbet-runtime"
require "pp"


class Color < T::Enum
  enums do
    RED = new('rojo')
    BLUE = new('azul')
  end
end

module Identifiable
  def identify
    "i am a #{self.name}"
  end
end

class Vehicle
  include Identifiable

  def name
    "vehicle"
  end
end

module Baml
  module Serializable
    def serialize
      Baml::Ffi::serialize(Baml::Types, self)
    end
  end

  module Types
    class Foo < T::Struct
      prepend Baml::Serializable

      const :foo, Integer
    end

    class Bar < T::Struct
      const :bar, String
    end

    class Top < T::Struct
      const :foo_or_bar, T.any(Foo, Bar)
      const :color, Color
    end
  end
end


describe "learning ruby and sorbet" do
  it "defines an enum without warnings" do
    assert_equal(Color::RED.serialize, "rojo")
    assert_equal(Color.deserialize("rojo"), Color::RED)
  end

  it "uses include correctly" do
    v = Vehicle.new
    assert_equal(v.identify, "i am a vehicle")
  end

  it "serializes a basic struct without warnings" do
    foo = Baml::Types::Foo.new(foo: 1)
    assert_equal(foo.serialize.foo, 1)
  end

  it "forwards each correctly" do
    class FakeStream
      include Enumerable

      def initialize
        @data = [1, 2, 3]
        @final = nil
      end

      def each(&block)
        @data.each(&block)
        @final = 'final'
      end

      def final
        @final
      end
    end

    f = FakeStream.new

    assert_nil(f.final)
    f.each_with_index do |e, i|
      assert_equal(e, i + 1)
    end
    assert_equal(f.final, 'final')
  end
end

Minitest::Reporters.use! Minitest::Reporters::SpecReporter.new
