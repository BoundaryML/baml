# frozen_string_literal: true

require "baml_bridge/cffi/v1/baml_inbound_pb"
require "baml_bridge/cffi/v1/baml_outbound_pb"

module Baml
  module Bridge
    module Protocol
      Wire = ::BamlBridge::Cffi::V1
      # Each wire occurrence owns a reference, even when several share a key.
      # Wait transfers those references to wrappers; an abandoned or failed
      # decode releases the rest without retaining the captured result itself.
      class Result
        attr_reader :message

        def initialize(message, value, api)
          @message = message
          @keys = []
          Protocol.visit_handles(value) do |handle|
            @keys << handle.key unless %i[HOST_VALUE_CALLABLE HOST_VALUE_OPAQUE].include?(handle.handle_type)
          end
          @cleanup = self.class.finalizer(api, @keys, Process.pid)
          ObjectSpace.define_finalizer(self, @cleanup)
        end

        def self.finalizer(api, keys, owner_pid)
          proc do
            if Process.pid == owner_pid
              until keys.empty?
                key = keys.pop
                begin
                  api.handle_release(key)
                rescue StandardError
                  # Cleanup cannot replace the result's decoding error.
                end
              end
            end
          end
        end

        def adopt(key)
          Thread.handle_interrupt(Exception => :never) do
            index = @keys.index(key) or raise Error, "BAML result handle #{key} was already adopted"
            owner = yield
            @keys.delete_at(index)
            owner
          end
        end

        def close
          @cleanup&.call
        end
      end
      @types = {}
      @names = {}
      @fields = {}
      @functions = {}
      module_function

      # The generator owns Ruby naming, including renamed fields. No constant
      # lookup or naming conversion happens while decoding untrusted wire data.
      def define_class(name, type, fields:)
        @types[name] = type
        @names[type] = name
        @fields[type] = fields.transform_values { |ruby, descriptor| [ruby, descriptor].freeze }.freeze
      end

      def define_enum(name, type)
        @types[name] = type
        @names[type] = name
        @fields[type] = nil
      end

      def define_function(name, params:)
        @functions[name] = params.to_h.freeze
      end

      def registered_type(name)
        @types.fetch(name) do
          raise UnsupportedTypeError, "BAML type #{name.inspect} is not registered"
        end
      end

      def encode_call(call_id, function_name, arguments, scope: nil)
        hints = @functions.fetch(function_name, {})
        kwargs = arguments.map do |name, value|
          Wire::InboundMapEntry.new(string_key: name, value: encode_value(value, hints[name], scope: scope))
        end
        Wire::CallFunctionArgs.encode(Wire::CallFunctionArgs.new(
          call_id: call_id, function_name: function_name, kwargs: kwargs,
          invocation: Wire::InvocationOptions.new
        ))
      end

      def encode_value(value, descriptor = nil, scope: nil)
        fields = case value
                 when String then { string_value: value }
                 when Integer then { int_value: value }
                 when Float then { float_value: value }
                 when true, false then { bool_value: value }
                 when nil then {}
                 when Array
                   { list_value: Wire::InboundListValue.new(values: value.map { |item| encode_value(item, scope: scope) }) }
                 when Hash
                   { map_value: Wire::InboundMapValue.new(entries: value.map { |key, item| encode_entry(key, item, scope: scope) }) }
                 when ::Baml::Handle
                   raise Error, "BAML handles can only be sent inside a call" unless scope

                   key, handle_type = value.clone_for_wire(scope)
                   { handle: Wire::BamlHandle.new(key: key, handle_type: handle_type) }
                 else
                   encode_registered_value(value, scope: scope)
                 end
        Wire::InboundValue.new(**fields)
      end

      def encode_registered_value(value, scope: nil)
        name = @names.fetch(value.class) do
          raise TypeError, "Cannot encode BAML argument kind #{value.class}"
        end
        fields = @fields.fetch(value.class)
        if fields
          entries = fields.map do |wire, (ruby, descriptor)|
            encode_entry(wire, value.public_send(ruby), descriptor, scope: scope)
          end
          {
            value_type: Wire::BamlTy.new(class_ty: Wire::BamlTyClass.new(name: name)),
            class_value: Wire::InboundClassValue.new(fields: entries)
          }
        else
          { enum_value: Wire::InboundEnumValue.new(name: name, value: value.serialize) }
        end
      end

      def encode_entry(key, value, descriptor = nil, scope: nil)
        fields = case key
                 when String then { string_key: key }
                 else
                   raise UnsupportedTypeError, "BAML map key kind #{key.class} is not yet supported"
                 end
        Wire::InboundMapEntry.new(**fields, value: encode_value(value, descriptor, scope: scope))
      end

      def capture_result(bytes, api = nil)
        message = Wire::BamlOutboundResult.decode(bytes)
        value = case message.result
                when :ok then message.ok
                when :error then message.error.value
                when :panic then message.panic.value
                end
        Result.new(message, value, api)
      end

      def visit_handles(value, &block)
        return unless value

        case value.value
        when :handle_value
          block.call(value.handle_value)
        when :class_value
          value.class_value.fields.each { |field| visit_handles(field.value, &block) }
        when :list_value
          value.list_value.items.each { |item| visit_handles(item, &block) }
        when :map_value
          value.map_value.entries.each { |entry| visit_handles(entry.value, &block) }
        when :union_variant_value
          visit_handles(value.union_variant_value.value, &block)
        end
      end

      def decode_result(bytes, api = nil)
        captured = bytes.is_a?(Result) ? bytes : capture_result(bytes, api)
        result = captured.message
        owners = { handles: captured }
        case result.result
        when :ok
          decode_value(result.ok, api, **owners)
        when :error
          raise_failure(::Baml::Error, result.error.value, api, **owners)
        when :panic
          raise_failure(::Baml::Panic, result.panic.value, api, **owners)
        else
          raise Error, "BAML returned an empty result envelope"
        end
      ensure
        captured&.close
      end

      def decode_value(value, api = nil, handles: nil)
        raise Error, "BAML returned an empty value" unless value

        case value.value
        when :string_value, :int_value, :float_value, :bool_value
          value.public_send(value.value)
        when :handle_value
          handle = value.handle_value
          if %i[HOST_VALUE_CALLABLE HOST_VALUE_OPAQUE].include?(handle.handle_type)
            raise UnsupportedTypeError, "BAML host-value handles are not supported"
          end
          build = -> { ::Baml::Handle.new(api, handle.key, handle.handle_type) }
          handles ? handles.adopt(handle.key, &build) : build.call
        when nil, :null_value
          # V1 currently emits an absent oneof for null.
          nil
        when :class_value
          object = value.class_value
          type = registered_type(object.name)
          fields = @fields.fetch(type)
          decoded = object.fields.to_h { |field| [fields.fetch(field.key)[0], decode_value(field.value, api, handles: handles)] }
          type.new(**decoded)
        when :enum_value
          enum = value.enum_value
          if enum.is_dynamic
            raise UnsupportedTypeError, "BAML dynamic enum #{enum.name} is not yet supported"
          end
          registered_type(enum.name).deserialize(enum.value)
        when :list_value
          value.list_value.items.map { |item| decode_value(item, api, handles: handles) }
        when :map_value
          map = value.map_value
          map.entries.to_h { |entry| [decode_map_key(entry.key, map.key_type), decode_value(entry.value, api, handles: handles)] }
        when :union_variant_value
          decode_value(value.union_variant_value.value, api, handles: handles)
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
        when :primitive
          case type.primitive.kind
          when :BAML_TY_PRIMITIVE_STRING then key
          else
            raise UnsupportedTypeError, "BAML map key type #{type.primitive.kind} is not yet supported"
          end
        else
          raise UnsupportedTypeError, "BAML map key type #{type&.ty.inspect} is not yet supported"
        end
      end

      def raise_failure(error_class, value, api = nil, handles: nil)
        while value&.value == :union_variant_value
          value = value.union_variant_value.value
        end
        if value&.value == :class_value
          object = value.class_value
          message = object.fields.find { |field| field.key == "message" }&.value
          raise error_class.new(message ? decode_value(message, api, handles: handles).to_s : object.name, type_name: object.name)
        end

        decoded = decode_value(value, api, handles: handles)
        kind = value.value == :literal_value ? value.literal_value.literal : (value.value || :null_value)
        raise error_class.new(decoded.to_s, type_name: kind.to_s.delete_suffix("_value"))
      end

    end
  end
end
