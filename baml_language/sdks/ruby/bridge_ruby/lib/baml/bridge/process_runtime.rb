# frozen_string_literal: true

require "pathname"
require "thread"

module Baml
  module Bridge
    class ProcessRuntime
      RUNTIME_PATH_ENV = "BAML_RUNTIME_PATH"

      def initialize
        @mutex = Mutex.new
        @owner_pid = nil
        @native_load_in_progress = false
        @library = nil
        @api = nil
        @result_callback = nil
        @pending_mutex = Mutex.new
        @pending = {}
        @next_callback_id = 1
        @program_bytes = nil
        @terminal_error = nil
      end

      def initialize!(compiled_program_bytes)
        program = owned_program_bytes(compiled_program_bytes)
        ensure_not_forked!

        candidate_path = nil
        if @api.nil? && @terminal_error.nil?
          candidate_path = configured_runtime_path!
          claim_process!
        end

        @mutex.synchronize do
          ensure_not_forked!
          raise @terminal_error if @terminal_error

          if @program_bytes
            return nil if @program_bytes == program

            raise ProgramConflictError,
                  "This Ruby process already initialized a different generated BAML program"
          end

          load_api!(candidate_path) unless @api
          begin
            @api.initialize_runtime(program)
          rescue IncompatibleRuntimeError => error
            @terminal_error = error
            raise
          end
          @program_bytes = program
          nil
        end
      end

      def call(compiled_program_bytes, function_name, arguments)
        initialize!(compiled_program_bytes)
        encoded = Protocol.encode_call(@api.new_function_call, function_name, arguments)
        # Defer Timeout/Thread#raise so an interrupt cannot land between
        # registering and dispatching, or be swallowed by a dispatch failure.
        # Once dispatched, the callback owns completion; an interrupted caller
        # just stops waiting.
        result = Thread.handle_interrupt(Exception => :never) do
          callback_id, queue = register_call
          begin
            @api.call_function(encoded, callback_id)
          rescue Exception # rubocop:disable Lint/RescueException -- a failed dispatch never reaches the callback
            @pending_mutex.synchronize { @pending.delete(callback_id) }
            raise
          end
          queue
        end
        outcome = result.pop
        raise outcome if outcome.is_a?(Exception)

        Protocol.decode_result(outcome)
      end

      private

      def register_call
        @pending_mutex.synchronize do
          # Never reuse IDs: a late duplicate must not complete a newer call.
          raise Error, "BAML callback correlation IDs are exhausted" if @next_callback_id > 0xffff_ffff

          callback_id = @next_callback_id
          @next_callback_id += 1
          queue = Queue.new
          @pending[callback_id] = queue
          [callback_id, queue]
        end
      end

      def owned_program_bytes(value)
        unless value.is_a?(String)
          raise TypeError, "compiled BAML program must be a String of bytes"
        end

        value.dup.force_encoding(Encoding::BINARY).freeze
      end

      def configured_runtime_path!
        value = ENV.fetch(RUNTIME_PATH_ENV, "")
        if value.empty?
          raise RuntimeConfigurationError,
                "#{RUNTIME_PATH_ENV} must name the bridge_cffi library"
        end

        path = Pathname.new(value)
        unless path.absolute?
          raise RuntimeConfigurationError,
                "#{RUNTIME_PATH_ENV} must be an absolute path: #{value.inspect}"
        end
        unless path.file?
          raise RuntimeConfigurationError,
                "#{RUNTIME_PATH_ENV} does not name a file: #{value.inspect}"
        end

        path.to_s.freeze
      end

      def claim_process!
        current_pid = Process.pid
        owner_pid = @owner_pid
        if owner_pid && owner_pid != current_pid
          raise_fork_error(owner_pid, current_pid)
        end

        # Assignment occurs before entering @mutex. A child created while a
        # load is in progress therefore rejects inherited state without ever
        # touching a mutex that may have been locked by a vanished thread.
        @owner_pid ||= current_pid
      end

      def ensure_not_forked!
        owner_pid = @owner_pid
        current_pid = Process.pid
        return unless owner_pid && owner_pid != current_pid

        # Once a failed native open returns, a child has no native state to
        # inherit. Replace the mutex that may still be locked by a vanished
        # parent thread, then let the child retry with its own process claim.
        if @library.nil? && @api.nil? && @terminal_error.nil? && !@native_load_in_progress
          @mutex = Mutex.new
          @owner_pid = nil
          return
        end

        raise_fork_error(owner_pid, current_pid)
      end

      def raise_fork_error(owner_pid, current_pid)
        raise ForkSafetyError,
              "BAML native state belongs to process #{owner_pid}; forked child " \
              "#{current_pid} must exec before using BAML"
      end

      def receive_result(callback_id, outcome)
        @pending_mutex.synchronize do
          queue = @pending.delete(callback_id)
          queue << outcome if queue
        end
      end

      def open_library(path, flags)
        FFI::DynamicLibrary.open(path, flags)
      end

      def load_api!(path)
        flags = FFI::DynamicLibrary::RTLD_NOW | FFI::DynamicLibrary::RTLD_LOCAL
        begin
          @native_load_in_progress = true
          @library = open_library(path, flags)
        rescue LoadError => error
          raise RuntimeLoadError,
                "Unable to open BAML runtime #{path.inspect}: #{error.message}"
        ensure
          @native_load_in_progress = false
        end

        begin
          api = Native::Api.new(
            @library,
            path,
            TOOLCHAIN_VERSION,
            BRIDGE_RUNTIME_VERSION
          )
          callback = Native::BorrowedBytesCallback.new(
            on_error: ->(id) { receive_result(id, @result_callback.pop_error(id)) },
            &method(:receive_result)
          )
          api.register_result_callback(callback.function)
          @result_callback = callback
          @api = api
        rescue Error => error
          @terminal_error = error
          raise
        rescue LoadError, StandardError => error
          wrapped = IncompatibleRuntimeError.new(
            "Unable to use BAML runtime #{path.inspect}: #{error.class}: #{error.message}"
          )
          @terminal_error = wrapped
          raise wrapped
        end
      end
    end
  end
end
