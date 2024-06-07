require "sorbet-runtime"

module Baml
  # class BamlStream
  #   include Enumerable

  #   def initialize(raw_stream:)
  #     @raw_stream = raw_stream
  #   end

  #   def each(&block)
  #     @raw_stream.each do |raw_msg|
  #       yield Message.from(raw_msg)
  #     end
  #   end
  # end

  class BamlStream
    extend T::Sig
    extend T::Generic

    include Enumerable

    PartialType = type_member
    FinalType = type_member

    def initialize(
      ffi_stream:,
      ctx_manager:
    )
      @ffi_stream = ffi_stream
      @ctx_manager = ctx_manager

      @final_response = nil
    end

    # Calls the given block once for each event in the stream, where event is a parsed
    # partial response. Returns `self` to enable chaining `.get_final_response`.
    #
    # Must be called with a block.
    #
    # @yieldparam [PartialType] event the parsed partial response
    # @return [BamlStream] self
    sig { params(block: T.proc.params(event: PartialType).void).returns(BamlStream)}
    def each(&block)
      # Implementing this and include-ing Enumerable allows users to treat this as a Ruby
      # collection: https://ruby-doc.org/3.1.6/Enumerable.html#module-Enumerable-label-Usage
      if @final_response == nil
        @final_response = @ffi_stream.done(@ctx_manager) do |event|
          block.call event.parsed_using_types(Baml::PartialTypes)
        end
      end

      self
    end


    # Gets the final response from the stream.
    #
    # @return [FinalType] the parsed final response
    sig {returns(FinalType)}
    def get_final_response
      if @final_response == nil
        @final_response = @ffi_stream.done(@ctx_manager)
      end

      @final_response.parsed_using_types(Baml::Types)
    end
  end
end