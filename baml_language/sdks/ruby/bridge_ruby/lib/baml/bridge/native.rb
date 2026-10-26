# frozen_string_literal: true

require "ffi"
require "thread"

module Baml
  module Bridge
    module Native
      ABI_VERSION = 4
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
          initialize_runtime_from_blob
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
          initialize_runtime_from_blob_with_metadata
          invocation_protocol_version
          invocation_clock_ns
          release_function_call
          register_host_dispatch_v2
          register_host_cancel_callback
          trace_selection
          invocation_context
          media_name
          media_from_file_content
        ].freeze

        layout :abi_version, :uint32,
               :struct_size, :size_t,
               *FUNCTION_FIELDS.flat_map { |field| [field, :pointer] }
      end

      FUNCTION_SIGNATURES = {
        version: [Buffer.by_value, []],
        initialize_runtime_from_blob: [
          Buffer.by_value,
          %i[pointer size_t],
          { blocking: true }
        ],
        initialize_runtime_from_blob_with_metadata: [
          Buffer.by_value,
          %i[pointer size_t string],
          { blocking: true }
        ],
        free_buffer: [:void, [Buffer.by_value]],
        register_callback: [:void, [:pointer]],
        new_function_call: [:uint64, []],
        release_function_call: [:int32, [:uint64]],
        cancel_function_call: [:int32, [:uint64]],
        handle_clone: [:int32, %i[uint64 pointer]],
        handle_release: [:int32, [:uint64]],
        call_function: [:void, %i[pointer size_t uint32], { blocking: true }],
        register_bridge: [Buffer.by_value, [:pointer]],
        register_unhandled_spawn_error_callback: [:void, [:pointer]],
        shutdown_runtime: [Buffer.by_value, [], { blocking: true }]
      }.freeze

      class Api
        attr_reader :library, :runtime_version

        def initialize(library, path, toolchain_version)
          @owner_pid = Process.pid
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
        end

        def register_result_callback(callback)
          function(:register_callback).call(callback)
        end

        def register_unhandled_spawn_error_callback(callback)
          function(:register_unhandled_spawn_error_callback).call(callback)
        end

        def shutdown_runtime
          diagnostic = read_owned_utf8(function(:shutdown_runtime).call, "shutdown_runtime")
          raise Error, "BAML runtime shutdown failed: #{diagnostic}" unless diagnostic.empty?
        end

        def initialize_runtime(bytecode, metadata = nil)
          pointer = memory_for(bytecode)
          operation = metadata.nil? ? :initialize_runtime_from_blob : :initialize_runtime_from_blob_with_metadata
          arguments = [pointer, bytecode.bytesize]
          arguments << metadata unless metadata.nil?
          diagnostic = read_owned_utf8(function(operation).call(*arguments), operation)
          return if diagnostic.empty?

          raise ProgramInitializationError,
                "Native BAML program initialization failed: #{diagnostic}"
        end

        def new_function_call
          call_id = function(:new_function_call).call
          raise Error, "BAML engine call IDs are exhausted" if call_id.zero?

          call_id
        end

        def release_function_call(call_id)
          function(:release_function_call).call(call_id)
        end

        def cancel_function_call(call_id)
          function(:cancel_function_call).call(call_id)
        end

        def handle_clone(key)
          out_key = FFI::MemoryPointer.new(:uint64)
          status = function(:handle_clone).call(key, out_key)
          raise Error, "Unable to clone BAML handle #{key}: status #{status}" unless status.zero?

          out_key.read_uint64
        end

        def handle_release(key)
          status = function(:handle_release).call(key)
          raise Error, "Unable to release BAML handle #{key}: status #{status}" unless status.zero?
        end

        def call_function(encoded_args, callback_id)
          pointer = memory_for(encoded_args)
          function(:call_function).call(pointer, encoded_args.bytesize, callback_id)
        end

        def ensure_not_forked!
          if @owner_pid != Process.pid
            raise ForkSafetyError, "Forked child #{Process.pid} must exec before using BAML"
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

        def function(name)
          ensure_not_forked!

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

      # Copy borrowed bytes, or pass the copy error to receive_result. The
      # handler settles failures inside Ruby; no exception may cross FFI.
      class BorrowedBytesCallback
        attr_reader :function

        def initialize(&handler)
          @handler = handler
          @function = FFI::Function.new(:void, %i[uint32 pointer size_t]) do |call_id, pointer, length|
            outcome = begin
              if length.zero?
                "".b.freeze
              elsif pointer.nil? || pointer.null?
                raise ArgumentError, "native callback pointer is null with #{length} bytes"
              else
                pointer.read_bytes(length).b.freeze
              end
            rescue Exception => error # rubocop:disable Lint/RescueException -- settle copy errors inside Ruby
              error
            end
            @handler.call(call_id, outcome)
          end
        end
      end
    end
  end
end
