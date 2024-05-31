require_relative "../lib/baml"
require "sorbet-runtime"
require "pp"

class Color < T::Enum
  enums do
    RED = new('RED')
    BLUE = new
  end
end

class Foo < T::Struct
  const :foo, Integer
  #def to_hash
  #  {:foo => self.foo}
  #end
end
class Bar < T::Struct
  const :bar, String
  #def to_hash
  #  {:bar => self.bar}
  #end
end

class Top < T::Struct
  const :foo_or_bar, T.any(Foo, Bar)
  const :color, Color

  #def to_hash
  #  {
  #    :foo_or_bar => self.foo_or_bar,
  #    :foo_or_bar2 => self.foo_or_bar,
  #    "foo" => Foo.new(foo: 23),
  #    :bar => Bar.new(bar: "hello"),
  #    :color => Color::RED,
  #  }
  #end
end

foo_top = Top.new(
  foo_or_bar: Foo.new(foo: 12),
  color: Color::RED
  )

#puts  foo_top.to_hash

pp Baml::Ffi::roundtrip(foo_top)
#puts "instance methods"
#puts foo_top.class.instance_methods(false)