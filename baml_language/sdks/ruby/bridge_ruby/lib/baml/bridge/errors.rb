# frozen_string_literal: true

module Baml
  class Error < StandardError
    attr_reader :type_name

    def initialize(type_name, message)
      @type_name = type_name
      super(message)
    end
  end

  class PanicError < StandardError
    attr_reader :type_name

    def initialize(type_name, message)
      @type_name = type_name
      super(message)
    end
  end

  module Bridge
    class Error < StandardError; end
    class UnsupportedTypeError < Error; end

    # The runtime path is absent or unusable before a native library is opened.
    # Correcting BAML_RUNTIME_PATH and retrying is safe.
    class RuntimeConfigurationError < Error; end
    class RuntimeLoadError < Error; end

    # The configured library opened but cannot safely satisfy the V1 contract.
    # The process caches this failure and never unloads or replaces the library.
    class IncompatibleRuntimeError < Error; end

    # The native table rejected bytecode without changing the current runtime.
    class ProgramInitializationError < Error; end

    # One process may initialize only one exact generated BAML program.
    class ProgramConflictError < Error; end

    # Native state inherited across fork is intentionally never touched.
    class ForkSafetyError < Error; end
  end
end
