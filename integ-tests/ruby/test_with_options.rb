require 'json'
require 'minitest/autorun'
require 'minitest/reporters'
require 'base64'

require_relative "baml_client/client"

b = Baml.Client
describe "with_options" do
  before do
    # Ensure collector is empty before each test
    if Baml::Collector.respond_to?(:__function_span_count)
      assert_equal 0, Baml::Collector.__function_span_count, "Expected no active function spans at the start"
    end
  end

  after do
    # Force garbage collection and verify all spans are cleaned up
    GC.start
    if Baml::Collector.respond_to?(:__function_span_count)
      assert_equal 0, Baml::Collector.__function_span_count,
                   "Expected no active function spans after forcing GC"
    end
  end

  it "should_test_with_options_logger_sync_call" do
    if Baml::Collector.respond_to?(:__function_span_count)
      puts "### function_span_count #{Baml::Collector.__function_span_count}"
      assert_equal 0, Baml::Collector.__function_span_count, "Expected no function spans before test starts"
    end

    # Create a collector
    collector = Baml::Collector.new(name: "my-collector")
    function_logs = collector.logs
    assert_equal 0, function_logs.length, "Expected no logs initially"

    # Create a b client with .with_options
    my_b = b.with_options(collector: collector)

    # Make the call
    my_b.TestOpenAIGPT4oMini(input: "hi there")

    # Verify logs
    function_logs = collector.logs
    assert_equal 1, function_logs.length, "Expected exactly one log after the call"

    log = collector.last
    refute_nil log, "Log entry should not be nil"
    assert_equal "TestOpenAIGPT4oMini", log.function_name
    assert_equal "call", log.log_type

    # Verify usage fields
    refute_nil log.usage.input_tokens
    assert log.usage.input_tokens > 0
    refute_nil log.usage.output_tokens
    assert log.usage.output_tokens > 0

    # Verify calls
    calls = log.calls
    assert_equal 1, calls.length, "Expected exactly one call entry"

    # Make a second call on the default b object (no collector)
    b.TestOpenAIGPT4oMini(input: "hi there")

    # Should not be logged since collector was not passed in
    function_logs = collector.logs
    assert_equal 1, function_logs.length, "Expected no additional logs for calls without collector"

    # Force garbage collection to check whether function spans remain
    GC.start
    if Baml::Collector.respond_to?(:__function_span_count)
      # Still not collected because it's in use
      assert Baml::Collector.__function_span_count > 0, "Expected some function spans to remain in memory"
    end
  end

  it "should_test_with_options_logger_sync_stream" do
    # Create a collector
    collector = Baml::Collector.new(name: "my-collector")
    function_logs = collector.logs
    assert_equal 0, function_logs.length, "Expected no logs initially"

    # Create a new instance with the collector
    my_b = b.with_options(collector: collector)

    # Call the streaming function
    # We'll assume a streaming approach like .stream.TestOpenAIGPT4oMini
    # that returns an enumerable we can iterate over
    stream = my_b.stream.TestOpenAIGPT4oMini(input: "hi there")
    chunks = []
    stream.each do |chunk|
      # We don't need to do anything with the chunk in this test
      chunks << chunk
    end

    # After streaming completes, logs should have exactly one entry
    function_logs = collector.logs
    assert_equal 1, function_logs.length, "Expected exactly one log after the streaming call"
    GC.start # note if we dont add this here either then for some reason the _after_ will indicate there's still a collector around. Maybe it takes a bit to run.

  end
end