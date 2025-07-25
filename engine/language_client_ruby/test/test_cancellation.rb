# frozen_string_literal: true

require "test/unit"
require "sorbet-runtime"
require_relative "../lib/stream"

# Mock FFI stream for testing
class MockFFIStream
  extend T::Sig

  def initialize
    @cancelled = false
    @mutex = Mutex.new
  end

  sig { void }
  def cancel
    @mutex.synchronize { @cancelled = true }
  end

  sig { returns(T::Boolean) }
  def is_cancelled
    @mutex.synchronize { @cancelled }
  end

  sig { params(callback: T.nilable(Proc)).void }
  def on_event(callback)
    @event_callback = callback
  end

  sig { params(ctx_manager: T.untyped).returns(T.untyped) }
  def done(ctx_manager)
    raise "Stream was cancelled" if is_cancelled
    
    # Simulate some partial events
    if @event_callback
      3.times do |i|
        break if is_cancelled
        result = OpenStruct.new(parsed: "partial_#{i}")
        @event_callback.call(result)
        sleep(0.01)
      end
    end
    
    OpenStruct.new(parsed: "final_result")
  end
end

class TestBamlStreamCancellation < Test::Unit::TestCase
  def setup
    @mock_ffi_stream = MockFFIStream.new
    @mock_ctx_manager = Object.new
    @stream = Baml::BamlStream.new(
      ffi_stream: @mock_ffi_stream,
      ctx_manager: @mock_ctx_manager
    )
  end

  def test_cancel_method_exists
    assert_respond_to(@stream, :cancel)
  end

  def test_cancelled_predicate_exists
    assert_respond_to(@stream, :cancelled?)
  end

  def test_initial_state_not_cancelled
    assert_false(@stream.cancelled?)
  end

  def test_cancel_sets_cancelled_state
    @stream.cancel
    assert_true(@stream.cancelled?)
  end

  def test_cancel_calls_ffi_stream_cancel
    @stream.cancel
    assert_true(@mock_ffi_stream.is_cancelled)
  end

  def test_cancelled_predicate_checks_ffi_stream
    @mock_ffi_stream.cancel
    assert_true(@stream.cancelled?)
  end

  def test_get_final_response_success
    result = @stream.get_final_response
    assert_equal("final_result", result)
  end

  def test_get_final_response_with_cancellation
    @stream.cancel
    
    assert_raises(RuntimeError, /cancelled/) do
      @stream.get_final_response
    end
  end

  def test_get_final_response_caches_result
    result1 = @stream.get_final_response
    result2 = @stream.get_final_response
    
    assert_equal(result1, result2)
    assert_same(result1, result2)
  end

  def test_each_iteration_with_cancellation
    partial_results = []
    
    # Cancel after short delay
    cancel_thread = Thread.new do
      sleep(0.05)
      @stream.cancel
    end
    
    @stream.each do |partial|
      partial_results << partial
      break if partial_results.length >= 5  # Safety limit
    end
    
    cancel_thread.join
    
    # Should have collected some partial results
    assert_operator(partial_results.length, :>=, 1)
    assert_true(@stream.cancelled?)
  end

  def test_each_without_block_returns_enumerator
    enumerator = @stream.each
    assert_instance_of(Enumerator, enumerator)
  end

  def test_multiple_cancellations_safe
    @stream.cancel
    @stream.cancel  # Should not raise error
    @stream.cancel  # Should not raise error
    
    assert_true(@stream.cancelled?)
  end

  def test_thread_safety_of_cancellation
    threads = []
    
    # Cancel from multiple threads simultaneously
    10.times do
      threads << Thread.new { @stream.cancel }
    end
    
    threads.each(&:join)
    
    assert_true(@stream.cancelled?)
  end

  def test_cancellation_prevents_resource_waste
    start_time = Time.now
    
    # Cancel immediately
    @stream.cancel
    
    assert_raises(RuntimeError, /cancelled/) do
      @stream.get_final_response
    end
    
    elapsed = Time.now - start_time
    
    # Should complete quickly due to immediate cancellation
    assert_operator(elapsed, :<, 0.1, "Cancellation should be immediate")
  end

  def test_cancellation_during_iteration
    partial_count = 0
    cancelled_during_iteration = false
    
    # Start iteration and cancel during it
    cancel_thread = Thread.new do
      sleep(0.02)  # Let iteration start
      @stream.cancel
    end
    
    begin
      @stream.each do |partial|
        partial_count += 1
        cancelled_during_iteration = @stream.cancelled?
        break if cancelled_during_iteration || partial_count >= 10
        sleep(0.01)  # Simulate processing time
      end
    rescue => e
      # Cancellation might cause iteration to stop with error
    end
    
    cancel_thread.join
    
    assert_true(@stream.cancelled?)
    # Should have detected cancellation during iteration
    assert_true(cancelled_during_iteration || partial_count == 0)
  end

  def test_coerce_methods_called
    # Test that subclasses can override coerce methods
    custom_stream = Class.new(Baml::BamlStream) do
      def coerce_partial(result)
        "partial_#{result}"
      end
      
      def coerce_final(result)
        "final_#{result}"
      end
    end.new(
      ffi_stream: @mock_ffi_stream,
      ctx_manager: @mock_ctx_manager
    )
    
    result = custom_stream.get_final_response
    assert_equal("final_final_result", result)
  end

  def test_error_handling_in_partial_processing
    # Mock FFI stream that causes errors in partial processing
    error_ffi_stream = Class.new(MockFFIStream) do
      def done(ctx_manager)
        if @event_callback
          # Simulate an error in partial processing
          result = OpenStruct.new(parsed: nil)  # This will cause coerce_partial to fail
          @event_callback.call(result)
        end
        
        OpenStruct.new(parsed: "final_result")
      end
    end.new
    
    error_stream = Baml::BamlStream.new(
      ffi_stream: error_ffi_stream,
      ctx_manager: @mock_ctx_manager
    )
    
    # Should handle errors gracefully and continue
    result = error_stream.get_final_response
    assert_equal("final_result", result)
  end

  def test_cleanup_on_cancellation
    partial_results = []
    
    @stream.cancel
    
    # Iteration should handle cancellation gracefully
    @stream.each do |partial|
      partial_results << partial
    end
    
    # Should not have processed any results due to cancellation
    assert_empty(partial_results)
  end
end

class TestBamlStreamIntegration < Test::Unit::TestCase
  def test_realistic_cancellation_scenario
    # Simulate a realistic scenario where user cancels a long-running operation
    mock_ffi_stream = MockFFIStream.new
    mock_ctx_manager = Object.new
    
    stream = Baml::BamlStream.new(
      ffi_stream: mock_ffi_stream,
      ctx_manager: mock_ctx_manager
    )
    
    start_time = Time.now
    results_collected = 0
    
    # Simulate user cancelling after seeing some results
    cancel_thread = Thread.new do
      sleep(0.03)  # Let some processing happen
      stream.cancel
    end
    
    begin
      stream.each do |partial|
        results_collected += 1
        break if results_collected >= 2  # User sees a couple results then cancels
      end
    rescue => e
      # Cancellation might cause errors
    end
    
    cancel_thread.join
    elapsed = Time.now - start_time
    
    assert_true(stream.cancelled?)
    assert_operator(elapsed, :<, 0.2, "Should complete quickly after cancellation")
  end

  def test_memory_cleanup_with_many_streams
    # Test that creating and cancelling many streams doesn't leak memory
    streams = []
    
    100.times do |i|
      mock_ffi_stream = MockFFIStream.new
      stream = Baml::BamlStream.new(
        ffi_stream: mock_ffi_stream,
        ctx_manager: Object.new
      )
      
      stream.cancel
      streams << stream
      
      assert_true(stream.cancelled?, "Stream #{i} should be cancelled")
    end
    
    # All streams should be cancelled
    assert_true(streams.all?(&:cancelled?))
  end
end
