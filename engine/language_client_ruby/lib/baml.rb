begin
  ruby_version = /(\d+\.\d+)/.match(RUBY_VERSION)
  require_relative "baml/#{ruby_version}/ruby_ffi"
rescue LoadError
  require_relative "baml/ruby_ffi"
end
require_relative "stream"

module Baml
  # TODO: implement image support
  class Image; end

  # Dynamically + idempotently define Baml::TypeConverter
  # NB: this does not respect raise_coercion_error = false
  def self.convert_to(type)
    if !Baml.const_defined?(:TypeConverter)
      Baml.const_set(:TypeConverter, Class.new(TypeCoerce::Converter) do
        def initialize(type)
          super(type)
        end
        
        def _convert(value, type, raise_coercion_error, coerce_empty_to_nil)
          # make string handling more strict
          if type == String
            if value.is_a?(String)
              return value
            end

            raise TypeCoerce::CoercionError.new(value, type)
          end

          # add unions
          if type.is_a?(T::Types::Union)
            type.types.each do |t|
              # require raise_coercion_error on the recursive union call,
              # so that we can suppress the error if it fails
              converted = _convert(value, t, true, coerce_empty_to_nil)
              return converted
            rescue
              # do nothing - try every instance of the union
            end

            raise TypeCoerce::CoercionError.new(value, type)
          end

          super(value, type, raise_coercion_error, coerce_empty_to_nil)
        end
      end)
    end

    Baml.const_get(:TypeConverter).new(type)
  end
end