# frozen_string_literal: true

require "ffi"
require "thread"

module Baml
  module Bridge
    module Native
      ABI_VERSION = 2
      RUBY_BRIDGE_LANGUAGE = 10
      BRIDGE_RUNTIME_NAME = "Baml::Bridge"
      MAX_OWNED_BUFFER_BYTES = 16 * 1024 * 1024

      class Buffer < FFI::Struct
        layout :ptr, :pointer,
               :len, :size_t
      end

      class BridgeInfoV1 < FFI::Struct
        layout :struct_size, :size_t,
               :language, :uint32,
               :sdk_version, :pointer,
               :sdk_version_len, :size_t,
               :bridge_runtime_name, :pointer,
               :bridge_runtime_name_len, :size_t,
               :bridge_runtime_version, :pointer,
               :bridge_runtime_version_len, :size_t
      end

      class ApiV1 < FFI::Struct
        FUNCTION_FIELDS = %i[
          version
          initialize_runtime_from_bytecode
          free_buffer
          register_callback
          call_function
          new_function_call
          cancel_function_call
          register_host_dispatch_callback
          register_host_release_callback
          complete_host_call
          handle_clone
          handle_release
          media_from_url
          media_from_file
          media_from_base64
          media_url
          media_file
          media_base64
          media_mime_type
          register_bridge
          register_unhandled_spawn_error_callback
          shutdown_runtime
          initialize_runtime_from_bytecode_with_metadata
        ].freeze

        layout :abi_version, :uint32,
               :struct_size, :size_t,
               *FUNCTION_FIELDS.flat_map { |field| [field, :pointer] }
      end

      FUNCTION_SIGNATURES = {
        version: [Buffer.by_value, []],
        initialize_runtime_from_bytecode: [
          Buffer.by_value,
          %i[pointer size_t],
          { blocking: true }
        ],
        free_buffer: [:void, [Buffer.by_value]],
        register_callback: [:void, [:pointer]],
        register_bridge: [Buffer.by_value, [:pointer]]
      }.freeze

      class Api
        attr_reader :library, :runtime_version

        def initialize(library, path, toolchain_version, bridge_runtime_version)
          @library = library
          @path = path
          @functions = {}
          @table = read_table
          validate_table!
          @runtime_version = read_owned_utf8(function(:version).call, "version")
          unless @runtime_version == toolchain_version
            raise IncompatibleRuntimeError,
                  "BAML runtime version mismatch: Ruby bridge requires #{toolchain_version}, " \
                  "but #{path.inspect} reports #{@runtime_version.inspect}"
          end

          register_bridge!(toolchain_version, bridge_runtime_version)
        end

        def register_result_callback(callback)
          function(:register_callback).call(callback)
        end

        def initialize_runtime(bytecode)
          pointer = memory_for(bytecode)
          diagnostic = read_owned_utf8(
            function(:initialize_runtime_from_bytecode).call(pointer, bytecode.bytesize),
            "initialize_runtime_from_bytecode"
          )
          return if diagnostic.empty?

          raise ProgramInitializationError,
                "Native BAML program initialization failed: #{diagnostic}"
        end

        private

        def read_table
          symbol = @library.find_function("baml_get_api_v1")
          if symbol.nil?
            raise IncompatibleRuntimeError,
                  "Unable to resolve #{@path.inspect}!baml_get_api_v1: symbol not found"
          end

          getter = FFI::Function.new(:pointer, [], symbol)
          pointer = getter.call
          if pointer.nil? || pointer.null?
            raise IncompatibleRuntimeError,
                  "#{@path.inspect}!baml_get_api_v1 returned a null table"
          end

          ApiV1.new(pointer)
        rescue FFI::NotFoundError => error
          raise IncompatibleRuntimeError,
                "Unable to resolve #{@path.inspect}!baml_get_api_v1: #{error.message}"
        end

        def validate_table!
          actual_abi = @table[:abi_version]
          unless actual_abi == ABI_VERSION
            raise IncompatibleRuntimeError,
                  "Expected bridge_cffi ABI #{ABI_VERSION}, received #{actual_abi}"
          end

          actual_size = @table[:struct_size]
          if actual_size < ApiV1.size
            raise IncompatibleRuntimeError,
                  "BamlApiV1 is truncated: #{actual_size} < #{ApiV1.size}"
          end

          ApiV1::FUNCTION_FIELDS.each do |field|
            pointer = @table[field]
            next unless pointer.nil? || pointer.null?

            raise IncompatibleRuntimeError, "BamlApiV1.#{field} is null"
          end
        end

        def register_bridge!(toolchain_version, bridge_runtime_version)
          toolchain_pointer = memory_for(toolchain_version.b)
          name_pointer = memory_for(BRIDGE_RUNTIME_NAME.b)
          bridge_version_pointer = memory_for(bridge_runtime_version.b)
          info = BridgeInfoV1.new
          info[:struct_size] = BridgeInfoV1.size
          info[:language] = RUBY_BRIDGE_LANGUAGE
          info[:sdk_version] = toolchain_pointer
          info[:sdk_version_len] = toolchain_version.bytesize
          info[:bridge_runtime_name] = name_pointer
          info[:bridge_runtime_name_len] = BRIDGE_RUNTIME_NAME.bytesize
          info[:bridge_runtime_version] = bridge_version_pointer
          info[:bridge_runtime_version_len] = bridge_runtime_version.bytesize

          diagnostic = read_owned_utf8(
            function(:register_bridge).call(info.pointer),
            "register_bridge"
          )
          return if diagnostic.empty?

          raise IncompatibleRuntimeError,
                "Native bridge registration rejected #{BRIDGE_RUNTIME_NAME} " \
                "#{bridge_runtime_version}: #{diagnostic}"
        end

        def function(name)
          @functions[name] ||= begin
            return_type, parameter_types, options = FUNCTION_SIGNATURES.fetch(name)
            FFI::Function.new(return_type, parameter_types, @table[name], **(options || {}))
          end
        end

        def read_owned_utf8(buffer, operation)
          bytes = read_owned_bytes(buffer, operation)
          text = bytes.dup.force_encoding(Encoding::UTF_8)
          return text.freeze if text.valid_encoding?

          raise IncompatibleRuntimeError,
                "BAML runtime #{operation} returned invalid UTF-8"
        end

        def read_owned_bytes(buffer, operation)
          length = buffer[:len]
          pointer = buffer[:ptr]

          begin
            if length > MAX_OWNED_BUFFER_BYTES
              raise IncompatibleRuntimeError,
                    "BAML runtime #{operation} returned #{length} bytes; " \
                    "limit is #{MAX_OWNED_BUFFER_BYTES}"
            end
            if length.positive? && (pointer.nil? || pointer.null?)
              raise IncompatibleRuntimeError,
                    "BAML runtime #{operation} returned a null pointer with #{length} bytes"
            end

            value = length.zero? ? String.new(encoding: Encoding::BINARY) : pointer.read_bytes(length)
            value.force_encoding(Encoding::BINARY).freeze
          ensure
            function(:free_buffer).call(buffer)
          end
        end

        def memory_for(bytes)
          return FFI::Pointer::NULL if bytes.empty?

          pointer = FFI::MemoryPointer.new(:uint8, bytes.bytesize)
          pointer.put_bytes(0, bytes)
          pointer
        end
      end

      # Owns an FFI callback and copies borrowed bytes before invoking Ruby.
      # Every exception is retained for the Ruby caller instead of crossing C.
      class BorrowedBytesCallback
        attr_reader :function

        def initialize(&handler)
          raise ArgumentError, "callback handler is required" unless handler

          @handler = handler
          @errors = Queue.new
          @function = FFI::Function.new(:void, %i[uint32 pointer size_t]) do |call_id, pointer, length|
            begin
              bytes = if length.zero?
                        String.new(encoding: Encoding::BINARY)
                      elsif pointer.nil? || pointer.null?
                        raise ArgumentError, "native callback pointer is null with #{length} bytes"
                      else
                        pointer.read_bytes(length)
                      end
              @handler.call(call_id, bytes.force_encoding(Encoding::BINARY).freeze)
            rescue Exception => error # rubocop:disable Lint/RescueException -- exceptions cannot cross FFI
              @errors << error
            end
          end
        end

        # The generated call layer will drain this queue after callbacks and
        # surface errors on the caller's Ruby thread.
        def pop_error
          @errors.pop(true)
        rescue ThreadError
          nil
        end
      end
    end
  end
end
