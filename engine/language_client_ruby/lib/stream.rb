require "sorbet-runtime"

module Baml
  class StreamState < T::Struct
    extend T::Sig
    extend T::Generic

    Value = type_member

    const :value, Value
    const :state, Symbol

    def initialize(props)
      super(value: props[:value], state: props[:state])
    end
  end

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
      @cancelled = false
      @mutex = Mutex.new
    end

    # Cancel the stream processing.
    # This will:
    # 1. Cancel the Rust-level stream
    # 2. Cancel ongoing HTTP requests to LLM providers
    # 3. Stop consuming network bandwidth and API quota
    # 4. Clean up resources
    sig { void }
    def cancel
      @mutex.synchronize do
        return if @cancelled
        @cancelled = true
        
        # Call the FFI stream's cancel method if it exists
        @ffi_stream.cancel if @ffi_stream.respond_to?(:cancel)
      end
    end

    # Check if the stream has been cancelled
    sig { returns(T::Boolean) }
    def cancelled?
      @mutex.synchronize { @cancelled } || 
        (@ffi_stream.respond_to?(:is_cancelled) && @ffi_stream.is_cancelled)
    end

    # Get the final response from the stream.
    # This will block until the stream completes and return the final result.
    # If the stream is cancelled, this will raise an exception.
    sig { returns(FinalType) }
    def get_final_response
      @mutex.synchronize do
        return @final_response if @final_response
      end

      raise "Stream was cancelled" if cancelled?

      begin
        result = @ffi_stream.done(@ctx_manager)
        final_result = coerce_final(result.parsed)
        
        @mutex.synchronize do
          @final_response = final_result
        end
        
        final_result
      rescue => e
        raise "Stream was cancelled: #{e.message}" if cancelled?
        raise
      end
    end

    # Iterate over partial results as they become available
    sig { params(block: T.proc.params(arg0: PartialType).void).void }
    def each(&block)
      return enum_for(:each) unless block_given?

      partial_results = []
      stream_done = false
      
      # Set up event handler for partial results
      @ffi_stream.on_event(proc do |result|
        next if cancelled?
        
        begin
          partial = coerce_partial(result.parsed)
          partial_results << partial
        rescue => e
          # Log error but continue streaming
          puts "Error processing partial result: #{e.message}"
        end
      end)

      # Start stream processing in background thread
      stream_thread = Thread.new do
        begin
          get_final_response
        rescue => e
          # Error will be handled when get_final_response is called again
        ensure
          stream_done = true
        end
      end

      begin
        # Yield partial results as they come in
        last_index = 0
        while !stream_done && !cancelled?
          # Yield any new partial results
          while last_index < partial_results.length
            yield partial_results[last_index]
            last_index += 1
          end
          
          # Small delay to avoid busy waiting
          sleep(0.01)
        end
        
        # Yield any remaining partial results
        while last_index < partial_results.length
          yield partial_results[last_index]
          last_index += 1
        end
        
      ensure
        # Clean up
        @ffi_stream.on_event(nil) if @ffi_stream.respond_to?(:on_event)
        stream_thread.join(1.0) # Give it a second to clean up
      end
    end

    private

    # Override these methods in subclasses to provide proper type coercion
    sig { params(result: T.untyped).returns(PartialType) }
    def coerce_partial(result)
      result
    end

    sig { params(result: T.untyped).returns(FinalType) }
    def coerce_final(result)
      result
    end
  end
end
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
          block.call event.parsed_using_types(Baml::Types, Baml::PartialTypes, true)
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

      @final_response.parsed_using_types(Baml::Types, Baml::PartialTypes, false)
    end
  end

end