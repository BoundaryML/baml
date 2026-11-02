# frozen_string_literal: true

module Baml
  module Bridge
    module Encode
      Tree = Struct.new(:kind, :value, :name, :children, :path, keyword_init: true)

      # Only reading runs host code. All arguments share one budget, charged
      # while pulling children, never trusting a host-reported collection size.
      class Read
        MAX_DEPTH = 50
        NODE_BUDGET = 1 << 20

        def initialize(registry, budget: NODE_BUDGET)
          @registry = registry
          @remaining = budget
        end

        def read(value, path, depth = 1)
          raise Fault, "#{path}: nested too deeply" if depth > MAX_DEPTH
          raise Fault, "#{path}: value exceeds the node budget" if @remaining.zero?

          @remaining -= 1
          name, fields = @registry.encoding_type(value.class)
          children = nil
          kind = if value.nil?
                   :null
                 elsif value.is_a?(::Baml::Handle)
                   :handle
                 elsif name && !fields
                   value = value.serialize.dup.freeze
                   :enum
                 elsif name
                   children = fields.map do |wire, (ruby, _descriptor)|
                     [wire, read(value.public_send(ruby), "#{path}.#{wire}", depth + 1)]
                   end
                   :class
                 elsif value.is_a?(Array)
                   children = []
                   value.each_with_index { |item, index| children << read(item, "#{path}[#{index}]", depth + 1) }
                   :list
                 elsif value.is_a?(Hash)
                   children = []
                   value.each do |key, item|
                     key_tree = read(key, "#{path}[key]", depth + 1)
                     entry_path = "#{path}[#{key.is_a?(String) ? key.inspect : 'entry'}]"
                     children << [key_tree, read(item, entry_path, depth + 1)]
                   end
                   :map
                 else
                   Rules.kind(value)
                 end
          if value.is_a?(String)
            unless value.encoding == Encoding::BINARY || value.valid_encoding?
              raise Refusal, "#{path}: cannot encode an invalid string"
            end
            value = value.dup.freeze
          end
          Tree.new(kind: kind, value: value, name: name, children: children, path: path)
        end
      end
    end
  end
end
