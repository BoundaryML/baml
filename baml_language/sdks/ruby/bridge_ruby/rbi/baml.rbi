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

  # Attach runtime arguments without changing the instance's Sorbet type.
  # T::Struct#with builds a new instance and drops the binding.
  sig { type_parameters(:Instance).params(instance: T.type_parameter(:Instance), types: T.untyped).returns(T.type_parameter(:Instance)) }
  def self.bind_types(instance, *types); end

  module ValueEquality
    extend T::Sig
    sig { params(other: T.untyped).returns(T::Boolean) }
    def ==(other); end
    sig { params(other: T.untyped).returns(T::Boolean) }
    def eql?(other); end
    sig { returns(Integer) }
    def hash; end
  end

  module Types
    extend T::Sig

    sig { params(item: T.untyped).returns(T.untyped) }
    def self.list(item); end

    sig { params(key: T.untyped, value: T.untyped).returns(T.untyped) }
    def self.map(key, value); end

    sig { params(inner: T.untyped).returns(T.untyped) }
    def self.optional(inner); end

    sig { params(klass: T.untyped, args: T.untyped).returns(T.untyped) }
    def self.generic(klass, *args); end
  end

  # Raised by Call#wait(timeout:) when the timeout passes. The call keeps running.
  class WaitTimeout < StandardError; end

  class Unset; end
  UNSET = T.let(T.unsafe(nil), Unset)

  class Options
    extend T::Sig
    # cancel is the generated baml.spawn.CancelToken, checked at the boundary.
    sig { params(timeout_ms: T.nilable(Integer), cancel: T.untyped, trace: NilClass).void }
    def initialize(timeout_ms: nil, cancel: nil, trace: nil); end
    sig { returns(T.nilable(Integer)) }
    def timeout_ms; end
    sig { returns(T.untyped) }
    def cancel; end
    sig { returns(NilClass) }
    def trace; end
  end

  class Error < StandardError
    extend T::Sig
    sig { params(value: T.untyped, baml_trace: T::Array[String], type_name: T.nilable(String)).void }
    def initialize(value, baml_trace: [], type_name: nil); end
    sig { returns(T.untyped) }
    def value; end
    sig { returns(T::Array[String]) }
    def baml_trace; end
    sig { returns(T.nilable(String)) }
    def type_name; end
  end
  class Panic < Exception
    extend T::Sig
    sig { params(value: T.untyped, baml_trace: T::Array[String], type_name: T.nilable(String)).void }
    def initialize(value, baml_trace: [], type_name: nil); end
    sig { returns(T.untyped) }
    def value; end
    sig { returns(T::Array[String]) }
    def baml_trace; end
    sig { returns(T.nilable(String)) }
    def type_name; end
  end
  class CancelledError < Panic; end

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

  # Frozen portable media values; file constructors read content immediately.
  class Media
    extend T::Sig
    sig { params(url: String, mime_type: T.nilable(String)).returns(T.attached_class) }
    def self.from_url(url, mime_type: nil); end
    sig { params(base64: String, mime_type: T.nilable(String)).returns(T.attached_class) }
    def self.from_base64(base64, mime_type: nil); end
    sig { params(file: String, mime_type: T.nilable(String)).returns(T.attached_class) }
    def self.from_file(file, mime_type: nil); end
    sig { params(file: String, base64: String, mime_type: T.nilable(String)).returns(T.attached_class) }
    def self.from_file_content(file, base64, mime_type: nil); end
    sig { returns(T.nilable(String)) }
    def url; end
    sig { returns(String) }
    def base64; end
    sig { returns(T.nilable(String)) }
    def mime_type; end
    sig { returns(T.nilable(String)) }
    def name; end
  end
  class Image < Media; end
  class Audio < Media; end
  class Video < Media; end
  class Pdf < Media; end

  module Bridge
    extend T::Sig

    class Error < StandardError; end
    class UnsupportedTypeError < Error; end

    # Type descriptors are frozen arrays mirroring the generator's Ty. Grammar:
    #   [:int] [:bigint] [:float] [:string] [:bool] [:null] [:bytes] [:never] [:handle]
    #   [:literal, value]                  value: String, Integer, Float, true or false
    #   [:media, kind]                     kind: :image, :audio, :video, :pdf, :generic
    #   [:class, "pkg.ns.Name", [args]]    args: descriptors
    #   [:enum, "pkg.ns.Name"]
    #   [:list, d]  [:map, key, value]  [:union, [d, ...]]
    #   [:alias, "pkg.ns.Name"]
    #   [:param, index, "T"]               compiler type-parameter index and name
    #   [:other]                           anything the encoder encodes by value
    # The encoder substitutes bound parameters to preserve bytes hints through
    # generic fields and containers; other values encode by their Ruby kind.
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
        fields: T::Hash[String, [Symbol, Descriptor]],
        type_params: T::Array[String]
      ).void
    end
    def self.define_class(wire, klass, fields:, type_params: []); end

    sig { params(wire: String, klass: T.class_of(T::Enum)).void }
    def self.define_enum(wire, klass); end

    # Exact alias targets retain byte hints, including through recursive aliases.
    sig { params(wire: String, descriptor: Descriptor).void }
    def self.define_alias(wire, descriptor); end

    # params: [wire name, descriptor] in declaration order.
    # Instance methods include their receiver as the first param, named "self".
    sig do
      params(
        wire: String,
        params: T::Array[[String, Descriptor]],
        class_type_params: T::Array[String],
        type_params: T::Array[String]
      ).void
    end
    def self.define_function(wire, params:, class_type_params: [], type_params: []); end

    # Start a call. args: wire parameter name => Ruby value; omitted keys are not sent.
    # types: explicit type bindings (Change D). baml: per-call options (Change C).
    # UNSET arguments are omitted. Binding names are method/function parameters;
    # class bindings come from the receiver's hidden runtime arguments and
    # precede method bindings.
    sig do
      params(
        wire: String,
        args: T::Hash[String, T.untyped],
        types: T.nilable(T::Hash[Symbol, T.untyped]),
        baml: T.nilable(::Baml::Options)
      ).returns(::Baml::Call[T.untyped])
    end
    def self.call_async(wire, args, types: nil, baml: nil); end
  end
end
