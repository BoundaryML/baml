# frozen_string_literal: true

module Baml
  module Bridge
    module Encode
      module Rules
        INT_RANGE = (-(2**62)..(2**62 - 1)).freeze
        module_function

        def kind(value)
          case value
          when nil then :null
          when true, false then :bool
          when Integer then INT_RANGE.cover?(value) ? :int : :bigint
          when Float then :float
          when String then value.encoding == Encoding::BINARY ? :bytes : :string
          else :other
          end
        end

        # The rank and converted payload come from the same rule.
        def fit(value, slot)
          natural = kind(value)
          return [:as_is, value] if natural == slot && natural != :other
          return [:converted, value] if natural == :int && slot == :bigint
          return [:converted, value.b] if natural == :string && slot == :bytes
          if natural == :bytes && slot == :string
            text = value.dup.force_encoding(Encoding::UTF_8)
            return text.valid_encoding? ? [:converted, text] : nil
          end
          return unless value.is_a?(Integer) && slot == :float

          float = value.to_f
          [:converted, float] if float.finite? && float.to_i == value
        end
      end
    end
  end
end
