# frozen_string_literal: true

require "pathname"
require "thread"

module Baml
  module Bridge
    class ProcessRuntime
      RUNTIME_PATH_ENV = "BAML_BRIDGE_PATH"

      def initialize
        @mutex = Mutex.new
        @library = nil
        @api = nil
        @bridge_registered = false
        @result_callback = nil
        @result_registered = false
        @spawn_callback = nil
        @spawn_registered = false
        @initialized = false
        @program_bytes = nil
        @metadata = nil
        @pending_mutex = Mutex.new
        @pending = {}
        @next_callback_id = 1
      end

      def register_program(bytecode, metadata = nil)
        @api&.ensure_not_forked!
        raise TypeError, "compiled BAML program must be a String of bytes" unless bytecode.is_a?(String)
        raise TypeError, "BAML metadata must be a String or nil" unless metadata.nil? || metadata.is_a?(String)

        program = bytecode.dup.force_encoding(Encoding::BINARY).freeze
        metadata = metadata&.dup&.freeze
        @mutex.synchronize do
          if @program_bytes && (@program_bytes != program || @metadata != metadata)
            raise ProgramConflictError, "This Ruby process already registered a different generated BAML program"
          end

          @program_bytes = program
          @metadata = metadata
        end
        nil
      end

      def initialize!
        @api&.ensure_not_forked!
        @mutex.synchronize do
          return if @initialized
          raise RuntimeConfigurationError, "No BAML program is registered" unless @program_bytes

          unless @library
            @runtime_path = configured_runtime_path!
            flags = FFI::DynamicLibrary::RTLD_NOW | FFI::DynamicLibrary::RTLD_LOCAL
            begin
              @library = open_library(@runtime_path, flags)
            rescue LoadError => error
              raise RuntimeLoadError, "Unable to open BAML runtime #{@runtime_path.inspect}: #{error.message}"
            end
          end
          @api ||= Native::Api.new(@library, @runtime_path, TOOLCHAIN_VERSION)
          unless @bridge_registered
            Thread.handle_interrupt(Exception => :never) do
              @api.register_bridge!(TOOLCHAIN_VERSION, BRIDGE_RUNTIME_VERSION)
              @bridge_registered = true
            end
          end
          unless @result_registered
            @result_callback ||= Native::BorrowedBytesCallback.new(&method(:receive_result))
            Thread.handle_interrupt(Exception => :never) do
              @api.register_result_callback(@result_callback.function)
              @result_registered = true
            end
          end
          unless @spawn_registered
            @spawn_callback ||= FFI::Function.new(:void, %i[pointer size_t int32]) do |pointer, length, cancelled|
              begin
                Protocol.decode_result(pointer.read_bytes(length), @api)
              rescue Exception => error # rubocop:disable Lint/RescueException -- match Python's spawn-error policy
                warn error.full_message
                $stderr.flush
                Kernel.exit!(1) if cancelled.zero?
              end
            end
            Thread.handle_interrupt(Exception => :never) do
              @api.register_unhandled_spawn_error_callback(@spawn_callback)
              @spawn_registered = true
            end
          end
          Thread.handle_interrupt(Exception => :never) do
            @api.initialize_runtime(@program_bytes, @metadata)
            @initialized = true
            at_exit do
              begin
                @api.shutdown_runtime
              rescue ForkSafetyError
                # A forked child must not shut down the parent's runtime.
              end
            end
          end
          nil
        end
      end

      def call_async(function_name, arguments)
        call_id = callback_id = nil
        submitted = false
        scope = EncodingScope.new
        initialize!
        # Retain native ownership before a deferred interrupt can reach Ruby.
        Thread.handle_interrupt(Exception => :never) { call_id = @api.new_function_call }
        encoded = Protocol.encode_call(call_id, function_name, arguments, scope: scope)
        Thread.handle_interrupt(Exception => :never) do
          call = ::Baml::Call.new(@api, call_id)
          callback_id = register_call(call)
          @api.call_function(encoded, callback_id)
          submitted = true
          scope.submitted!
          call
        end
      ensure
        Thread.handle_interrupt(Exception => :never) do
          unless submitted
            @pending_mutex.synchronize { @pending.delete(callback_id) } if callback_id
            begin
              scope&.close
            ensure
              @api.release_function_call(call_id) if call_id
            end
          end
        end
      end

      private

      def register_call(call)
        @pending_mutex.synchronize do
          # Never reuse IDs: a late duplicate must not complete a newer call.
          raise Error, "BAML callback correlation IDs are exhausted" if @next_callback_id > 0xffff_ffff

          callback_id = @next_callback_id
          @next_callback_id += 1
          @pending[callback_id] = call
          callback_id
        end
      end

      def configured_runtime_path!
        value = ENV.fetch(RUNTIME_PATH_ENV, "")
        if value.empty?
          raise RuntimeConfigurationError,
                "#{RUNTIME_PATH_ENV} must name the bridge_cffi library"
        end

        path = Pathname.new(value)
        unless path.absolute?
          raise RuntimeConfigurationError, "#{RUNTIME_PATH_ENV} must be an absolute path: #{value.inspect}"
        end
        unless path.file?
          raise RuntimeConfigurationError, "#{RUNTIME_PATH_ENV} does not name a file: #{value.inspect}"
        end

        path.to_s.freeze
      end

      def receive_result(callback_id, outcome)
        call = @pending_mutex.synchronize { @pending.delete(callback_id) }
        return unless call

        begin
          outcome = Protocol.capture_result(outcome, @api) unless outcome.is_a?(Exception)
        rescue Exception => error # rubocop:disable Lint/RescueException -- settle parse failures inside Ruby
          outcome = error
        end
        call.complete(outcome)
      end

      def open_library(path, flags)
        FFI::DynamicLibrary.open(path, flags)
      end
    end
  end
end
