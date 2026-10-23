# frozen_string_literal: true

module Baml
  module Failure
    attr_reader :value, :baml_trace, :type_name

    def initialize(value, baml_trace: [], type_name: nil)
      @value = value
      @baml_trace = baml_trace.dup.freeze
      @type_name = type_name
      message = value.respond_to?(:message) ? value.message.to_s : value.to_s
      super(message.empty? ? (type_name || value.inspect) : message)
    end
  end

  class Error < StandardError
    include Failure
  end

  # Panics must escape ordinary `rescue => error`, just like Python's BaseException.
  class Panic < Exception # rubocop:disable Lint/InheritException
    include Failure
  end

  class CancelledError < Panic; end

  module Bridge
    class Error < StandardError; end
    class UnsupportedTypeError < Error; end

    # The runtime path is absent or unusable before a native library is opened.
    # Correcting BAML_BRIDGE_PATH and retrying is safe.
    class RuntimeConfigurationError < Error; end
    class RuntimeLoadError < Error; end

    # The configured library cannot satisfy the V1 contract.
    class IncompatibleRuntimeError < Error; end

    # The native table rejected bytecode without changing the current runtime.
    class ProgramInitializationError < Error; end

    # One process may register only one exact generated BAML program.
    class ProgramConflictError < Error; end

    # Native state inherited across fork is intentionally never touched.
    class ForkSafetyError < Error; end
  end
end
