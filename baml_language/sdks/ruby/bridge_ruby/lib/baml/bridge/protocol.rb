# frozen_string_literal: true

require "baml_bridge/cffi/v1/baml_inbound_pb"
require "baml_bridge/cffi/v1/baml_outbound_pb"

module Baml
  module Bridge
    module Protocol
      Wire = ::BamlBridge::Cffi::V1
      @types = {}
      @names = {}
      @fields = {}
      module_function

      # The generator owns Ruby naming, including renamed fields. No constant
      # lookup or naming conversion happens while decoding untrusted wire data.
      def register_type(name, type, fields: nil)
        @types[name] = type
        @names[type] = name
        @fields[type] = fields&.freeze
      end

      def registered_type(name)
        @types.fetch(name) do
          raise UnsupportedTypeError, "BAML type #{name.inspect} is not registered"
        end
      end

      def encode_call(call_id, function_name, arguments)
        kwargs = arguments.map do |name, value|
          Wire::InboundMapEntry.new(string_key: name, value: encode_value(value))
        end
        Wire::CallFunctionArgs.encode(Wire::CallFunctionArgs.new(
          call_id: call_id, function_name: function_name, kwargs: kwargs
        ))
      end

      def encode_value(value)
        fields = case value
                 when String then { string_value: value }
                 when Integer then { int_value: value }
                 when Float then { float_value: value }
                 when true, false then { bool_value: value }
                 when nil then {}
                 when Array
                   { list_value: Wire::InboundListValue.new(values: value.map { |item| encode_value(item) }) }
                 when Hash
                   { map_value: Wire::InboundMapValue.new(entries: value.map { |key, item| encode_entry(key, item) }) }
                 else
                   encode_registered_value(value)
                 end
        Wire::InboundValue.new(**fields)
      end

      def encode_registered_value(value)
        name = @names.fetch(value.class) do
          raise UnsupportedTypeError, "BAML argument kind #{value.class} is not yet supported"
        end
        fields = @fields.fetch(value.class)
        if fields
          entries = fields.map { |wire, ruby| encode_entry(wire, value.public_send(ruby)) }
          {
            value_type: Wire::BamlTy.new(class_ty: Wire::BamlTyClass.new(name: name)),
            class_value: Wire::InboundClassValue.new(fields: entries)
          }
        else
          { enum_value: Wire::InboundEnumValue.new(name: name, value: value.serialize) }
        end
      end

      def encode_entry(key, value)
        fields = case key
                 when String then { string_key: key }
                 when Integer then { int_key: key }
                 when true, false then { bool_key: key }
                 else
                   encoded = encode_value(key)
                   unless encoded.value == :enum_value
                     raise UnsupportedTypeError, "BAML map key kind #{key.class} is not yet supported"
                   end
                   { enum_key: encoded.enum_value }
                 end
        Wire::InboundMapEntry.new(**fields, value: encode_value(value))
      end

      def decode_result(bytes)
        result = Wire::BamlOutboundResult.decode(bytes)
        case result.result
        when :ok
          decode_value(result.ok)
        when :error
          raise_failure(::Baml::Error, result.error.value)
        when :panic
          if result.panic.is_exit_panic
            raise UnsupportedTypeError,
                  "BAML is_exit_panic (exit code #{result.panic.exit_code}) is not yet supported"
          end
          raise_failure(::Baml::PanicError, result.panic.value)
        else
          raise Error, "BAML returned an empty result envelope"
        end
      end

      def decode_value(value)
        raise Error, "BAML returned an empty value" unless value

        case value.value
        when :string_value, :int_value, :float_value, :bool_value
          value.public_send(value.value)
        when nil, :null_value
          # V1 currently emits an absent oneof for null.
          nil
        when :class_value
          object = value.class_value
          unless object.type_args.empty?
            raise UnsupportedTypeError, "BAML generic class #{object.name} is not yet supported"
          end
          type = registered_type(object.name)
          fields = @fields.fetch(type)
          decoded = object.fields.to_h { |field| [fields.fetch(field.key), decode_value(field.value)] }
          type.new(**decoded)
        when :enum_value
          enum = value.enum_value
          if enum.is_dynamic
            raise UnsupportedTypeError, "BAML dynamic enum #{enum.name} is not yet supported"
          end
          registered_type(enum.name).deserialize(enum.value)
        when :list_value
          value.list_value.items.map { |item| decode_value(item) }
        when :map_value
          map = value.map_value
          map.entries.to_h { |entry| [decode_map_key(entry.key, map.key_type), decode_value(entry.value)] }
        when :union_variant_value
          decode_value(value.union_variant_value.value)
        when :literal_value
          literal = value.literal_value
          case literal.literal
          when :string_value, :int_value, :bool_value
            literal.public_send(literal.literal)
          when :float_value
            Float(literal.float_value)
          else
            raise UnsupportedTypeError, "BAML literal kind #{literal.literal.inspect} is not yet supported"
          end
        else
          raise UnsupportedTypeError, "BAML result kind #{value.value} is not yet supported"
        end
      end

      def decode_map_key(key, type)
        case type&.ty
        when :enum
          registered_type(type.enum.name).deserialize(key)
        when :primitive
          case type.primitive.kind
          when :BAML_TY_PRIMITIVE_STRING then key
          when :BAML_TY_PRIMITIVE_INT then Integer(key, 10)
          when :BAML_TY_PRIMITIVE_BOOL then decode_bool_key(key)
          else
            raise UnsupportedTypeError, "BAML map key type #{type.primitive.kind} is not yet supported"
          end
        when :literal
          # The generator types literal keys as their primitive (String, Integer, T::Boolean).
          case type.literal.literal
          when :string_value then key
          when :int_value then Integer(key, 10)
          when :bool_value then decode_bool_key(key)
          else
            raise UnsupportedTypeError, "BAML map key literal #{type.literal.literal} is not yet supported"
          end
        else
          raise UnsupportedTypeError, "BAML map key type #{type&.ty.inspect} is not yet supported"
        end
      end

      def decode_bool_key(key)
        return true if key == "true"
        return false if key == "false"

        raise Error, "BAML returned an invalid bool map key #{key.inspect}"
      end

      def raise_failure(error_class, value)
        # Errors retain the concrete thrown type and message without requiring
        # generated declarations for built-in error or panic classes.
        while value&.value == :union_variant_value
          value = value.union_variant_value.value
        end
        if value&.value == :class_value
          object = value.class_value
          message = object.fields.find { |field| field.key == "message" }&.value
          raise error_class.new(object.name, message ? decode_value(message).to_s : object.name)
        end

        decoded = decode_value(value)
        kind = value.value == :literal_value ? value.literal_value.literal : (value.value || :null_value)
        raise error_class.new(kind.to_s.delete_suffix("_value"), decoded.to_s)
      end
    end
  end
end
