# frozen_string_literal: true

require_relative "bridge/version"
require_relative "bridge/errors"
require_relative "options"
require_relative "bridge/native"
require_relative "bridge/call"
require_relative "bridge/handle"
require_relative "bridge/types"
require_relative "value_equality"
require_relative "bridge/protocol"
require_relative "bridge/process_runtime"

module Baml
  module Bridge
    @process_runtime = ProcessRuntime.new

    class << self
      def register_program(bytecode, metadata = nil)
        @process_runtime.register_program(bytecode, metadata)
      end

      def define_class(wire, klass, fields:, type_params: [])
        Protocol.define_class(wire, klass, fields: fields, type_params: type_params)
      end

      def define_enum(wire, klass)
        Protocol.define_enum(wire, klass)
      end

      def define_alias(wire, descriptor)
        Protocol.define_alias(wire, descriptor)
      end

      def define_function(wire, params:, class_type_params: [], type_params: [])
        Protocol.define_function(wire, params: params, class_type_params: class_type_params, type_params: type_params)
      end

      def call_async(wire, args, types: nil, baml: nil)
        @process_runtime.call_async(wire, args, types: types, baml: baml)
      end

      def initialize!
        @process_runtime.initialize!
      end
    end

    private_constant :ProcessRuntime
    private_constant :Native
  end
end
