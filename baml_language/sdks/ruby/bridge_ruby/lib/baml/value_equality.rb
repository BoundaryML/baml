# frozen_string_literal: true

module Baml
  # Generated structs are values. Exact runtime type arguments are part of a
  # generic value's identity, even though Sorbet erases its [] specialization.
  module ValueEquality
    def ==(other)
      other.instance_of?(self.class) &&
        Types.instance_args(self) == Types.instance_args(other) &&
        self.class.props.each_key.all? { |field| public_send(field) == other.public_send(field) }
    end

    def eql?(other)
      other.instance_of?(self.class) &&
        Types.instance_args(self).eql?(Types.instance_args(other)) &&
        self.class.props.each_key.all? { |field| public_send(field).eql?(other.public_send(field)) }
    end

    def hash
      [self.class, Types.instance_args(self), self.class.props.keys.map { |field| public_send(field) }].hash
    end
  end
end
