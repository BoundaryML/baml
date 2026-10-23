# frozen_string_literal: true

require "sorbet-runtime"
require "baml_bridge/cffi/v1/baml_type_pb"

module Baml
  # Sorbet erases Klass[T] at runtime. These tokens retain the complete BAML
  # type instead, including nested arguments. They are immutable wire values.
  module Types
    Wire = ::BamlBridge::Cffi::V1
    INSTANCE_ARGS = :@__baml_type_args
    @registered = {}
    module_function

    def register(klass, name, arity = nil)
      @registered[klass] = [name, arity].freeze
    end

    def list(item)
      Wire::BamlTy.new(list: Wire::BamlTyList.new(item: token(item))).freeze
    end

    def map(key, value)
      Wire::BamlTy.new(map: Wire::BamlTyMap.new(key: token(key), value: token(value))).freeze
    end

    def optional(inner)
      Wire::BamlTy.new(optional: Wire::BamlTyOptional.new(inner: token(inner))).freeze
    end

    def generic(klass, *args)
      name, arity = @registered.fetch(klass) { raise TypeError, "#{klass} is not a registered BAML class" }
      raise TypeError, "#{klass} is not a generic BAML class" unless arity && arity.positive?
      check_arity(klass, arity, args.length)
      Wire::BamlTy.new(class_ty: Wire::BamlTyClass.new(name: name, type_args: args.map { |arg| token(arg) })).freeze
    end

    def token(type)
      return type if type.is_a?(Wire::BamlTy) && type.frozen?

      primitive = { Integer => :INT, String => :STRING, Float => :FLOAT, T::Boolean => :BOOL, NilClass => :NULL }[type]
      if primitive
        return Wire::BamlTy.new(primitive: Wire::BamlTyPrimitive.new(kind: "BAML_TY_PRIMITIVE_#{primitive}".to_sym)).freeze
      end
      name, arity = @registered.fetch(type) { raise TypeError, "#{type.inspect} is not a BAML type token" }
      if arity
        raise TypeError, "Use Baml::Types.generic(#{type}, ...) to supply its #{arity} type argument(s)" if arity.positive?
        Wire::BamlTy.new(class_ty: Wire::BamlTyClass.new(name: name)).freeze
      else
        Wire::BamlTy.new(enum: Wire::BamlTyEnum.new(name: name)).freeze
      end
    end

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

    # Preserve bytes/string choices and callable hints through containers.
    # Keep exact protobuf types in instance metadata for re-encoding.
    def descriptor(type)
      case type.ty
      when :primitive
        [{ BAML_TY_PRIMITIVE_BYTES: :bytes, BAML_TY_PRIMITIVE_STRING: :string }.fetch(type.primitive.kind, :other)]
      when :literal
        type.literal.literal == :string_value ? [:literal, type.literal.string_value] : [:other]
      when :type_alias then [:alias, type.type_alias.name]
      when :function then [:function, type.function.params.map { |param| descriptor(param.ty) }, descriptor(type.function.ret)]
      when :list then [:list, descriptor(type.list.item)]
      when :map then [:map, descriptor(type.map.key), descriptor(type.map.value)]
      when :optional then [:union, [descriptor(type.optional.inner), [:null]]]
      when :union then [:union, type.union.options.map { |option| descriptor(option) }]
      when :class_ty then [:class, type.class_ty.name, type.class_ty.type_args.map { |arg| descriptor(arg) }]
      else [:other]
      end
    end
  end

  # Preserve the instance's Sorbet type and identity. T::Struct#with constructs
  # a new object and drops this binding; bind the copy again when needed.
  def self.bind_types(instance, *types)
    Types.bind(instance, *types)
  end
end
