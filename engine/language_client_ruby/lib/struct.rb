# This file should NOT be imported from baml.rb; we don't want
# to introduce a hard dependency on Sorbet for the baml gem.
require "ostruct"
#require "pp"

module Baml
  module Sorbet
    module Struct
      # Needed to allow accessing dynamic types
      def method_missing(symbol)
        @props[symbol]
      end

      def eql?(other)
        self.class == other.class &&
          @props.eql?(other.instance_variable_get(:@args))
      end

      def hash
        [self.class, @props].hash
      end

      def inspect
        PP.pp(self, +"", 79)
      end

      # https://docs.ruby-lang.org/en/master/PP.html
      def pretty_print(pp)
        pp.object_group(self) do
          pp.breakable
          @props.each do |key, value|
            pp.text "#{key}="
            pp.pp value
            pp.comma_breakable unless key == @props.keys.last
          end
        end
      end

      # From the ostruct implementation
      def to_h(&block)
        if block
          @props.map(&block).to_h
        else
          @props.dup
        end
      end

      def to_json(*args)
        @props.to_json(*args)
      end
    end
  end

  class DynamicStruct < OpenStruct
    def to_json(*args)
      @table.to_json(*args)
    end
  end
end