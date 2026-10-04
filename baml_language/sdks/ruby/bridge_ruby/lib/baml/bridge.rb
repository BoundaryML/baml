# frozen_string_literal: true

require_relative "bridge/version"
require_relative "bridge/errors"
require_relative "bridge/native"
require_relative "bridge/process_runtime"

module Baml
  module Bridge
    @process_runtime = ProcessRuntime.new

    class << self
      def initialize!(compiled_program_bytes)
        @process_runtime.initialize!(compiled_program_bytes)
      end
    end

    private_constant :ProcessRuntime
    private_constant :Native
  end
end
