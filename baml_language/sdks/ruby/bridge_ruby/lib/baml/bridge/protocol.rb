# frozen_string_literal: true

require "baml_bridge/cffi/v1/baml_inbound_pb"
require "baml_bridge/cffi/v1/baml_outbound_pb"
require_relative "../media"
require_relative "encode/slot"
require_relative "encode/rules"
require_relative "encode/read"
require_relative "encode/plan"
require_relative "encode/commit"

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
      @enums = {}
      @aliases = {}
      @crossings = {}
      module_function

      # The generator owns Ruby naming, including renamed fields. No constant
      # lookup or naming conversion happens while decoding untrusted wire data.
      def define_class(name, type, fields:, crossing: :fields)
        @types[name] = type
        @names[type] = name
        @crossings[name] = crossing
        @fields[type] = fields.transform_values { |ruby, descriptor| [ruby, descriptor].freeze }.freeze
      end

      def define_enum(name, type)
        @types[name] = type
        @names[type] = name
        @fields[type] = nil
        @enums[name] = type.values.map(&:serialize).freeze
      end

      def define_alias(name, descriptor, recursive: false)
        @aliases[name] = [descriptor, recursive].freeze
      end

      def alias_definition(name)
        @aliases[name]
      end

      def class_crossing(name)
        @crossings.fetch(name, :fields)
      end

      def define_function(name, params:)
        @functions[name] = params.to_h.freeze
      end

      def registered_type(name)
        @types.fetch(name) do
          raise UnsupportedTypeError, "BAML type #{name.inspect} is not registered"
        end
      end

      def encoding_type(type)
        [@names[type], @fields[type]]
      end

      def encoding_fields(name)
        @fields.fetch(registered_type(name))
      end

      def enum_values(name)
        @enums.fetch(name)
      end

      def encode_call(call_id, function_name, arguments, scope: nil)
        hints = @functions.fetch(function_name, {})
        reader = Encode::Read.new(self)
        planner = Encode::Plan.new(self)
        trees = arguments.map { |name, value| [name, reader.read(value, name)] }
        plans = trees.map { |name, tree| [name, planner.plan(tree, hints.fetch(name, [:unknown]))] }
        plans.each { |name, plan| Encode::Commit.check_depth!(Encode::Commit.depth(plan), name) }
        kwargs = plans.map do |name, plan|
          value = Encode::Commit.commit(plan, scope: scope)
          Encode::Commit.check_depth!(Encode::Slot.wire_depth(value), name)
          Wire::InboundMapEntry.new(string_key: name, value: value)
        end
        Wire::CallFunctionArgs.encode(Wire::CallFunctionArgs.new(
          call_id: call_id, function_name: function_name, kwargs: kwargs,
          invocation: Wire::InvocationOptions.new
        ))
      rescue Encode::Refusal => error
        raise error.class, "#{function_name}: #{error.message}"
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
        when :ty_value
          visit_type_handles(value.ty_value, &block)
        end
      end

      def visit_type_handles(type, &block)
        if type.respond_to?(:head) && type.head == :live
          block.call(Wire::BamlHandle.new(key: type.live, handle_type: :ADT_TAGGED_HEAP_HANDLE))
        end
        Encode::Slot.each_message(type) { |child| visit_type_handles(child, &block) }
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

      def decode_media(media)
        type = { IMAGE: ::Baml::Image, AUDIO: ::Baml::Audio, VIDEO: ::Baml::Video, PDF: ::Baml::Pdf }[media.media]
        raise UnsupportedTypeError, "BAML media kind #{media.media} is not supported" unless type

        mime = media.has_mime_type? ? media.mime_type : nil
        case media.value
        when :url then type.from_url(media.url, mime_type: mime)
        when :base64 then type.from_base64(media.base64, mime_type: mime)
        when :file_content
          type.from_file_content(media.file_content.name, media.file_content.base64, mime_type: mime)
        else raise Error, "BAML returned media with no content"
        end
      end

      def decode_value(value, api = nil, handles: nil)
        raise Error, "BAML returned an empty value" unless value

        case value.value
        when :string_value, :int_value, :float_value, :bool_value
          value.public_send(value.value)
        when :uint8array_value
          value.uint8array_value.b
        when :bigint_value
          Integer(value.bigint_value, 16)
        when :media_value
          decode_media(value.media_value)
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
          when :bigint_value
            Integer(literal.bigint_value, 16)
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
        when :enum_variant
          registered_type(type.enum_variant.name).deserialize(key)
        when :union, :type_alias, :never
          # Compiler map keys (including aliases and literal unions) denote strings.
          key
        when :primitive
          case type.primitive.kind
          when :BAML_TY_PRIMITIVE_STRING then key
          when :BAML_TY_PRIMITIVE_INT, :BAML_TY_PRIMITIVE_BIGINT then Integer(key, 10)
          when :BAML_TY_PRIMITIVE_BOOL then decode_bool_key(key)
          else
            raise UnsupportedTypeError, "BAML map key type #{type.primitive.kind} is not yet supported"
          end
        when :literal
          case type.literal.literal
          when :string_value then key
          when :int_value, :bigint_value then Integer(key, 10)
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
