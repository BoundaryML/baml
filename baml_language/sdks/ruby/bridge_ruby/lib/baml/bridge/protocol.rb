# frozen_string_literal: true

require "baml_bridge/cffi/v1/baml_inbound_pb"
require "baml_bridge/cffi/v1/baml_outbound_pb"

module Baml
  module Bridge
    module Protocol
      Wire = ::BamlBridge::Cffi::V1
      module_function

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
                 else
                   raise UnsupportedTypeError, "BAML argument kind #{value.class} is not yet supported"
                 end
        Wire::InboundValue.new(**fields)
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

      def raise_failure(error_class, value)
        # Error unions retain the concrete thrown type. Class payloads remain
        # unsupported, but their wire name and message can already be surfaced.
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
