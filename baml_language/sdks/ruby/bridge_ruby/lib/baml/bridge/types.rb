# frozen_string_literal: true

require "sorbet-runtime"

module Baml
  # Sorbet erases Klass[T] at runtime. Immutable descriptors retain the complete
  # BAML type, including nested arguments, until the walker commits to wire.
  module Types
    INSTANCE_ARGS = :@__baml_type_args
    @registered = {}
    module_function

    def register(klass, name, arity = nil)
      @registered[klass] = [name, arity].freeze
    end

    def list(item)
      [:list, token(item)].freeze
    end

    def map(key, value)
      [:map, token(key), token(value)].freeze
    end

    def optional(inner)
      [:union, [token(inner), [:null].freeze].freeze].freeze
    end

    def generic(klass, *args)
      name, arity = @registered.fetch(klass) { raise TypeError, "#{klass} is not a registered BAML class" }
      raise TypeError, "#{klass} is not a generic BAML class" unless arity && arity.positive?
      check_arity(klass, arity, args.length)
      [:class, name, args.map { |arg| token(arg) }.freeze].freeze
    end

    def token(type)
      return immutable(type) if type.is_a?(Array)

      primitive = { Integer => :int, String => :string, Float => :float, T::Boolean => :bool, NilClass => :null }[type]
      return [primitive].freeze if primitive

      name, arity = @registered.fetch(type) { raise TypeError, "#{type.inspect} is not a BAML type token" }
      if arity
        raise TypeError, "Use Baml::Types.generic(#{type}, ...) to supply its #{arity} type argument(s)" if arity.positive?
        [:class, name, [].freeze].freeze
      else
        [:enum, name].freeze
      end
    end

    def immutable(value)
      case value
      when Array then value.map { |item| immutable(item) }.freeze
      when String then value.dup.freeze
      else value
      end
    end
    private_class_method :immutable

    def bind(instance, *types)
      _name, arity = @registered.fetch(instance.class) { raise TypeError, "#{instance.class} is not a registered BAML class" }
      raise TypeError, "#{instance.class} is not a generic BAML class" unless arity && arity.positive?
      check_arity(instance.class, arity, types.length)
      instance.instance_variable_set(INSTANCE_ARGS, types.map { |type| token(type) }.freeze)
      instance
    end

    def instance_args(instance)
      instance.instance_variable_get(INSTANCE_ARGS) || []
    end

    def check_arity(klass, expected, actual)
      return if expected == actual
      raise TypeError, "expected #{expected} type argument(s) for #{klass}, got #{actual}"
    end
  end

  # Preserve the instance's Sorbet type and identity. T::Struct#with constructs
  # a new object and drops this binding; bind the copy again when needed.
  def self.bind_types(instance, *types)
    Types.bind(instance, *types)
  end
end
