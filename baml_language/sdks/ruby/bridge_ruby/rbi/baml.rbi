# typed: strict
# frozen_string_literal: true

# Contract between generated `baml_sdk/` code and the Ruby runtime.
#
# Generated code may call only what is declared here. The runtime implements it.
# The Sorbet static gate type-checks generated code against this file, so a
# mismatch between generator and runtime fails the gate rather than a test run.
#
# Later changes extend this file; they do not rename or remove anything in it.

module Baml
  extend T::Sig

  module ValueEquality
    extend T::Sig
    sig { params(other: T.untyped).returns(T::Boolean) }
    def ==(other); end
    sig { params(other: T.untyped).returns(T::Boolean) }
    def eql?(other); end
    sig { returns(Integer) }
    def hash; end
  end

  # Raised by Call#wait(timeout:) when the timeout passes. The call keeps running.
  class WaitTimeout < StandardError; end

  class Error < StandardError
    extend T::Sig
    sig { params(value: T.untyped, type_name: T.nilable(String)).void }
    def initialize(value, type_name: nil); end
    sig { returns(T.nilable(String)) }
    def type_name; end
  end
  class Panic < Exception
    extend T::Sig
    sig { params(value: T.untyped, type_name: T.nilable(String)).void }
    def initialize(value, type_name: nil); end
    sig { returns(T.nilable(String)) }
    def type_name; end
  end

  # The promise every BAML call returns. Sync generated methods are `call_async(...).wait`.
  class Call
    extend T::Sig
    extend T::Generic
    Value = type_member

    # Block until the outcome is known, then return the decoded value or raise
    # the decoded error. Repeated and concurrent waits see the same outcome.
    # With a timeout (seconds), raise Baml::WaitTimeout and leave the call running.
    sig { params(timeout: T.nilable(Numeric)).returns(Value) }
    def wait(timeout: nil); end

    # Ask the engine to cancel. True when the request was accepted, not when
    # cancellation won; the eventual outcome still comes from the engine.
    sig { returns(T::Boolean) }
    def cancel; end

    sig { returns(T::Boolean) }
    def done?; end
  end

  # An owned engine handle. Released by #close or when garbage-collected.
  # Sending a handle to BAML clones it first, so the Ruby object stays valid.
  class Handle
    extend T::Sig
    sig { void }
    def close; end
  end

  module Bridge
    extend T::Sig

    class Error < StandardError; end
    class UnsupportedTypeError < Error; end

    # Type descriptors are frozen arrays mirroring the generator's Ty. Grammar:
    #   [:int] [:float] [:string] [:bool] [:null] [:never] [:handle]
    #   [:literal, value]                  value: String, Integer, Float, true or false
    #   [:class, "pkg.ns.Name"]
    #   [:enum, "pkg.ns.Name"]
    #   [:list, d]  [:map, [:string], value]  [:union, [d, [:null]]]
    #   [:other]                           anything the encoder encodes by value
    # Values encode by their Ruby kind.
    Descriptor = T.type_alias { T::Array[T.untyped] }

    # Called once when the generated package loads. Registering a different
    # program in the same process raises.
    sig { params(bytecode: String, metadata: T.nilable(String)).void }
    def self.register_program(bytecode, metadata = nil); end

    # Load the library and initialize the registered program now, instead of
    # on the first call. Calling it again is a no-op.
    sig { void }
    def self.initialize!; end

    # fields: wire field name => [Ruby field name, descriptor]
    sig do
      params(
        wire: String,
        klass: T.class_of(T::Struct),
        fields: T::Hash[String, [Symbol, Descriptor]]
      ).void
    end
    def self.define_class(wire, klass, fields:); end

    sig { params(wire: String, klass: T.class_of(T::Enum)).void }
    def self.define_enum(wire, klass); end

    # params: [wire name, descriptor] in declaration order.
    # Instance methods include their receiver as the first param, named "self".
    sig do
      params(
        wire: String,
        params: T::Array[[String, Descriptor]]
      ).void
    end
    def self.define_function(wire, params:); end

    # Start a call. args: wire parameter name => Ruby value; omitted keys are not sent.
    sig do
      params(
        wire: String,
        args: T::Hash[String, T.untyped]
      ).returns(::Baml::Call[T.untyped])
    end
    def self.call_async(wire, args); end
  end
end
