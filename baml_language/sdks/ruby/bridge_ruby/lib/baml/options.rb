# frozen_string_literal: true

module Baml
  class Unset
    private_class_method :new
  end
  UNSET = Unset.send(:new).freeze

  # Call controls are immutable. The engine clock fixes the deadline before
  # argument encoding, so conversion time counts against the timeout too.
  class Options
    attr_reader :timeout_ms, :cancel, :trace

    def initialize(timeout_ms: nil, cancel: nil, trace: nil)
      unless timeout_ms.nil? || (timeout_ms.is_a?(Integer) && (0..2_147_483_647).cover?(timeout_ms))
        raise ArgumentError, "timeout_ms must be an integer in 0…2147483647 or nil"
      end
      raise ArgumentError, "trace only supports nil for now" unless trace.nil?

      @timeout_ms = timeout_ms
      @cancel = cancel
      @trace = trace
      freeze
    end
  end
end
