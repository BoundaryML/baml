# typed: strict

# Only the dynamic bridge boundary, not generated SDK functions or models.
# call decodes values selected by bytecode at runtime; the generator owns each
# public method's precise return type. The gate does not check bridge internals.
module Baml
  class Error < StandardError
    extend T::Sig
    sig { returns(String) }
    def type_name; end
  end
  class PanicError < StandardError
    extend T::Sig
    sig { returns(String) }
    def type_name; end
  end
  module Bridge
    extend T::Sig
    class Error < StandardError; end
    class UnsupportedTypeError < Error; end

    sig { params(compiled_program_bytes: String).returns(NilClass) }
    def self.initialize!(compiled_program_bytes); end

    sig do
      params(
        name: String,
        type: T.any(T.class_of(T::Struct), T.class_of(T::Enum)),
        fields: T.nilable(T::Hash[String, Symbol])
      ).returns(T.nilable(T::Hash[String, Symbol]))
    end
    def self.register_type(name, type, fields: nil); end

    sig do
      params(
        compiled_program_bytes: String,
        function_name: String,
        arguments: T::Hash[String, T.untyped]
      ).returns(T.untyped)
    end
    def self.call(compiled_program_bytes, function_name, arguments); end
  end
end

# External runtime helper used to eagerly validate generated signatures.
# Its private Signature result is not consumed by generated code.
module T::Utils
  sig { params(method: Method).returns(T.untyped) }
  def self.signature_for_method(method); end
end
