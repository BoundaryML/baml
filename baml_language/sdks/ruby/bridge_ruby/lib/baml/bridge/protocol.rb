# frozen_string_literal: true

require "baml_bridge/cffi/v1/baml_inbound_pb"
require "baml_bridge/cffi/v1/baml_outbound_pb"
require_relative "../media"

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
      @function_type_params = {}
      @aliases = {}
      module_function

      # The generator owns Ruby naming, including renamed fields. No constant
      # lookup or naming conversion happens while decoding untrusted wire data.
      def define_class(name, type, fields:, type_params: [])
        ::Baml::Types.register(type, name, type_params.length)
        @types[name] = type
        @names[type] = name
        @fields[type] = fields.transform_values { |ruby, descriptor| [ruby, descriptor].freeze }.freeze
      end

      def define_enum(name, type)
        ::Baml::Types.register(type, name)
        @types[name] = type
        @names[type] = name
        @fields[type] = nil
      end

      def define_alias(name, descriptor)
        @aliases[name] = descriptor
      end

      def define_function(name, params:, class_type_params: [], type_params: [])
        @functions[name] = params.to_h.freeze
        @function_type_params[name] = [class_type_params.freeze, type_params.freeze].freeze
      end

      def registered_type(name)
        @types.fetch(name) do
          raise UnsupportedTypeError, "BAML type #{name.inspect} is not registered"
        end
      end

      def encode_invocation(call_id, options, api, scope: nil)
        invocation = Wire::InvocationOptions.new
        return invocation unless options

        unless options.timeout_ms.nil?
          deadline = api.invocation_clock_ns(call_id) + options.timeout_ms * 1_000_000
          raise ArgumentError, "timeout deadline exceeds the runtime clock range" if deadline > 0xffff_ffff_ffff_ffff

          invocation.deadline_ns = deadline
        end
        unless options.cancel.nil?
          unless @names[options.cancel.class] == "baml.spawn.CancelToken"
            raise TypeError, "cancel must be a generated baml.spawn.CancelToken"
          end
          invocation.cancel = encode_value(options.cancel, scope: scope)
        end
        invocation
      end

      def prepare_type_args(function_name, arguments, types)
        unless types.nil? || types.is_a?(Hash)
          raise TypeError, "_types must be a Hash of Symbol names to BAML type tokens"
        end
        class_params, method_params = @function_type_params.fetch(function_name, [[], []])
        types ||= {}
        types.each_key do |name|
          unless name.is_a?(Symbol) && method_params.include?(name.to_s)
            raise TypeError, "unknown type binding #{name.inspect} for #{function_name}; expected #{method_params.inspect}"
          end
        end
        explicit = method_params.filter_map do |name|
          Wire::BamlTyArg.new(type_var: name, type_value: ::Baml::Types.token(types[name.to_sym])) if types.key?(name.to_sym)
        end
        receiver_args = arguments.key?("self") ? ::Baml::Types.instance_args(arguments["self"]) : []
        if !explicit.empty? && !class_params.empty? && receiver_args.empty?
          raise TypeError, "_types on a generic method requires a parameterized receiver; use Baml.bind_types(receiver, ...)"
        end
        unless receiver_args.empty?
          ::Baml::Types.check_arity("#{function_name} receiver", class_params.length, receiver_args.length)
        end
        inherited = receiver_args.each_with_index.map do |type, index|
          Wire::BamlTyArg.new(type_var: class_params.fetch(index), type_value: type)
        end
        inherited + explicit
      end

      def substitute_descriptor(descriptor, bindings, indexed: false)
        return descriptor unless descriptor
        substitute = ->(item) { substitute_descriptor(item, bindings, indexed: indexed) }
        case descriptor[0]
        when :param then bindings.fetch(descriptor[indexed ? 1 : 2], descriptor)
        when :list then [:list, substitute.call(descriptor[1])]
        when :map then [:map, substitute.call(descriptor[1]), substitute.call(descriptor[2])]
        when :union then [:union, descriptor[1].map(&substitute)]
        when :class then [:class, descriptor[1], descriptor[2].map(&substitute)]
        else descriptor
        end
      end

      def encode_call(call_id, function_name, arguments, scope: nil, invocation: Wire::InvocationOptions.new, type_args: [])
        hints = @functions.fetch(function_name, {})
        bindings = type_args.to_h { |arg| [arg.type_var, ::Baml::Types.descriptor(arg.type_value)] }
        kwargs = arguments.filter_map do |name, value|
          next if value.equal?(::Baml::UNSET)

          Wire::InboundMapEntry.new(string_key: name, value: encode_value(value, substitute_descriptor(hints[name], bindings), scope: scope))
        end
        Wire::CallFunctionArgs.encode(Wire::CallFunctionArgs.new(
          call_id: call_id, function_name: function_name, kwargs: kwargs, type_args: type_args,
          invocation: invocation
        ))
      end

      def encode_value(value, descriptor = nil, scope: nil)
        fields = case value
                 when String
                   bytes = encoding_hint(descriptor, :bytes) &&
                           (!encoding_hint(descriptor, :string) || value.encoding == Encoding::BINARY)
                   bytes ? { uint8array_value: value.b } : { string_value: value }
                 when Integer
                   value.between?(-(2**63), 2**63 - 1) ? { int_value: value } : { bigint_value: value.to_s(16) }
                 when Float then { float_value: value }
                 when true, false then { bool_value: value }
                 when nil then {}
                 when Array
                   hint = encoding_hint(descriptor, :list, 1)
                   { list_value: Wire::InboundListValue.new(values: value.map { |item| encode_value(item, hint, scope: scope) }) }
                 when Hash
                   hint = encoding_hint(descriptor, :map, 2)
                   { map_value: Wire::InboundMapValue.new(entries: value.map { |key, item| encode_entry(key, item, hint, scope: scope) }) }
                 when ::Baml::Media
                   { media_value: encode_media(value) }
                 when ::Baml::Handle
                   raise Error, "BAML handles can only be sent inside a call" unless scope

                   key, handle_type = value.clone_for_wire(scope)
                   { handle: Wire::BamlHandle.new(key: key, handle_type: handle_type) }
                 else
                   encode_registered_value(value, descriptor, scope: scope)
                 end
        Wire::InboundValue.new(**fields)
      end

      # Only bytes affect value-based encoding. Keep both bytes and string
      # hints through containers so a binary String can break a union tie.
      def encoding_hint(descriptor, kind, index = nil, visiting = [])
        return unless descriptor
        if descriptor[0] == :alias
          name = descriptor[1]
          return if visiting.include?(name)

          return encoding_hint(@aliases[name], kind, index, visiting + [name])
        end
        if descriptor[0] == :union
          hints = descriptor[1].filter_map { |option| encoding_hint(option, kind, index, visiting) }
          return if hints.empty?

          return index ? [:union, hints] : true
        end
        return true if kind == :string && descriptor[0] == :literal && descriptor[1].is_a?(String)
        return unless descriptor[0] == kind

        index ? descriptor[index] : true
      end

      def encode_media(value)
        kind = { ::Baml::Image => :IMAGE, ::Baml::Audio => :AUDIO, ::Baml::Video => :VIDEO, ::Baml::Pdf => :PDF }
          .fetch(value.class) { raise UnsupportedTypeError, "BAML media kind #{value.class} is not supported" }
        fields = { media: kind }
        fields[:mime_type] = value.mime_type if value.mime_type
        if value.url
          fields[:url] = value.url
        elsif value.name
          fields[:file_content] = Wire::BamlValueMediaFileContent.new(name: value.name, base64: value.base64)
        else
          fields[:base64] = value.base64
        end
        Wire::BamlValueMedia.new(**fields)
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

      def encode_registered_value(value, hint = nil, scope: nil)
        name = @names.fetch(value.class) do
          raise TypeError, "Cannot encode BAML argument kind #{value.class}"
        end
        fields = @fields.fetch(value.class)
        if fields
          type_args = ::Baml::Types.instance_args(value)
          field_args = type_args.map { |arg| ::Baml::Types.descriptor(arg) }
          field_args = hint[2] if field_args.empty? && hint&.first == :class && hint[1] == name
          bindings = field_args.each_with_index.to_h { |arg, index| [index, arg] }
          entries = fields.map do |wire, (ruby, descriptor)|
            encode_entry(wire, value.public_send(ruby), substitute_descriptor(descriptor, bindings, indexed: true), scope: scope)
          end
          {
            value_type: Wire::BamlTy.new(class_ty: Wire::BamlTyClass.new(name: name, type_args: type_args)),
            class_value: Wire::InboundClassValue.new(fields: entries)
          }
        else
          { enum_value: Wire::InboundEnumValue.new(name: name, value: value.serialize) }
        end
      end

      def encode_entry(key, value, descriptor = nil, scope: nil)
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
          raise_failure(::Baml::Error, result.error.value, api, trace: result.error.trace.to_a, **owners)
        when :panic
          return Kernel.exit!(result.panic.exit_code) if result.panic.is_exit_panic

          raise_failure(::Baml::Panic, result.panic.value, api, trace: result.panic.trace.to_a, **owners)
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
          instance = type.new(**decoded)
          unless object.type_args.empty?
            ::Baml::Types.bind(instance, *object.type_args.map(&:freeze))
          end
          instance
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
          # Current compiler map keys (including aliases and literal unions)
          # denote strings. Legacy integer, bool and enum keys retain their
          # explicit wire cases below.
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
          # The generator types literal keys as their primitive (String, Integer, T::Boolean).
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

      def raise_failure(error_class, value, api = nil, trace: [], handles: nil)
        while value&.value == :union_variant_value
          value = value.union_variant_value.value
        end
        name = if value&.value == :class_value
                 value.class_value.name
               else
                 kind = value&.value == :literal_value ? value.literal_value.literal : (value&.value || :null_value)
                 kind.to_s.delete_suffix("_value")
               end
        if error_class == ::Baml::Error && name == "baml.errors.TypeMismatch"
          message = value.class_value.fields.find { |field| field.key == "message" }&.value
          error = TypeError.new(message ? decode_value(message, api, handles: handles).to_s : name)
        else
          error_class = ::Baml::CancelledError if error_class == ::Baml::Panic && name == "baml.panics.Cancelled"
          error = error_class.new(decode_value(value, api, handles: handles), type_name: name, baml_trace: trace)
        end
        # Ruby backtraces use path:line:in `function`, while the wire uses
        # Python's frame format. Keep the original lines on #baml_trace.
        frames = trace.reverse.map do |frame|
          frame.sub(/\AFile "([^"]*)", line (\d+), in (.*)\z/, '\1:\2:in `\3\'')
        end
        error.set_backtrace(frames + caller)
        raise error
      end

    end
  end
end
