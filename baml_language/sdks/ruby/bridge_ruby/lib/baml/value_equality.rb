# frozen_string_literal: true

module Baml
  # Generated structs are values.
  module ValueEquality
    def ==(other)
      other.instance_of?(self.class) &&
        self.class.props.each_key.all? { |field| public_send(field) == other.public_send(field) }
    end

    def eql?(other)
      other.instance_of?(self.class) &&
        self.class.props.each_key.all? { |field| public_send(field).eql?(other.public_send(field)) }
    end

    def hash
      [self.class, self.class.props.keys.map { |field| public_send(field) }].hash
    end
  end
end
