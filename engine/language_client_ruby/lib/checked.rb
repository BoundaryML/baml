require "sorbet-runtime"

module Baml
  module Checks

    class Check < T::Struct
      extend T::Sig

      const :name, String
      const :expr, String
      const :status, String


      def initialize(props)
        super(name: props[:name], expr: props[:expr], status: props[:status])
      end
    end

    class Checked < T::Struct
      extend T::Sig

      extend T::Generic

      Value = type_member

      const :value, Value
      const :checks, T::Hash[Symbol, Check]

      def initialize(props)
        super(value: props[:value], checks: props[:checks])
      end

    end

  end
end
