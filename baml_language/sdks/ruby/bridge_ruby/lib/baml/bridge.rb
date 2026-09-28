# frozen_string_literal: true

require_relative "bridge/version"
require_relative "bridge/errors"
require_relative "bridge/native"
require_relative "bridge/protocol"
require_relative "bridge/process_runtime"

module Baml
  module Bridge
    @process_runtime = ProcessRuntime.new

    class << self
      def register_type(name, type, fields: nil)
        Protocol.register_type(name, type, fields: fields)
      end

      def initialize!(compiled_program_bytes)
        @process_runtime.initialize!(compiled_program_bytes)
      end

      def call(compiled_program_bytes, function_name, arguments)
        @process_runtime.call(compiled_program_bytes, function_name, arguments)
      end
    end

    private_constant :ProcessRuntime
    private_constant :Native
    private_constant :Protocol
  end
end
