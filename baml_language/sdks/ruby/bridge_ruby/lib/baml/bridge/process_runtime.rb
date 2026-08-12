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
        @result_events = Queue.new
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

      private

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

      def receive_result(call_id, bytes)
        @result_events << [call_id, bytes, Thread.current.object_id].freeze
      end

      # The generated call layer will drain one event for each native result
      # before result delivery is enabled.
      def pop_result_event
        @result_events.pop(true)
      rescue ThreadError
        nil
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
          callback = Native::BorrowedBytesCallback.new(&method(:receive_result))
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
