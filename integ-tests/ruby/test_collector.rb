require 'json'
require 'minitest/autorun'
require 'minitest/reporters'
require 'base64'

require_relative "baml_client/client"

b = Baml.Client

# Run all these tests with:
# infisical run --env=test -- mise exec -- rake test test_collector.rb TEST_OPTS="--name=/collector/"
describe "Ruby Collector Tests" do
  before do
    # Ensure collector is empty before each test
    # This depends on if Ruby exposes the same API as Python
    # You might need to modify this based on actual Ruby API
    assert_equal 0, Baml::Collector.__function_span_count if Baml::Collector.respond_to?(:__function_span_count)
  end

  after do
    # Force garbage collection and check collector is empty
    GC.start
    # GC.start(full_mark: true, immediate_sweep: true);
    assert_equal 0, Baml::Collector.__function_span_count if Baml::Collector.respond_to?(:__function_span_count)
  end

  it "test_collector_no_stream_success" do
    collector = Baml::Collector.new()
    function_logs = collector.logs
    assert_equal 0, function_logs.length


    # Call a test function with the collector'
    puts "calling func"
    b.TestOpenAIGPT4oMini(input: "hi there", baml_options: {collector: collector})

    puts "func called"


    puts "#{Baml::Collector.__function_span_count}"
    puts "#{Baml::Collector.__print_storage}"

    function_logs = collector.logs
    assert_equal 1, function_logs.length


    log = collector.last
    refute_nil log
    assert_equal "TestOpenAIGPT4oMini", log.function_name
    assert_equal "call", log.log_type

    # Verify timing fields
    assert log.timing.start_time_utc_ms > 0
    refute_nil log.timing.duration_ms
    assert log.timing.duration_ms > 0

    # Verify usage fields
    refute_nil log.usage.input_tokens
    assert log.usage.input_tokens > 0
    refute_nil log.usage.output_tokens
    assert log.usage.output_tokens > 0

    # Verify calls
    calls = log.calls
    assert_equal 1, calls.length

    call = calls[0]
    assert_equal "openai", call.provider
    assert_equal "GPT4oMini", call.client_name
    assert call.selected

    # Verify request/response
    request = call.http_request
    body = request.body.json()
    refute_nil body
    assert_kind_of Hash, body
    assert_includes body, "messages"
    assert_includes body["messages"][0], "content"
    refute_nil body["messages"][0]["content"]
    assert_equal "gpt-4o-mini", body["model"]

    # Verify http response
    response = call.http_response
    refute_nil response
    body = response.body.json()
    assert_equal 200, response.status
    refute_nil body
    assert_kind_of Hash, body
    assert_includes body, "choices"
    assert body["choices"].length > 0
    assert_includes body["choices"][0], "message"
    assert_includes body["choices"][0]["message"], "content"
    refute_nil body["choices"][0]["message"]["content"]

    puts "call.body.headers: #{call.http_response.headers}"
    # Verify response headers contain openai-version
    refute_nil response.headers
    assert_kind_of Hash, response.headers
    assert_includes response.headers, "openai-version"
    refute_nil response.headers["openai-version"]

    # Verify call timing
    call_timing = call.timing
    assert call_timing.start_time_utc_ms > 0
    refute_nil call_timing.duration_ms
    assert call_timing.duration_ms > 0

    # Verify call usage
    call_usage = call.usage
    refute_nil call_usage.input_tokens
    assert call_usage.input_tokens > 0
    refute_nil call_usage.output_tokens
    assert call_usage.output_tokens > 0

    # Matches log usage
    assert_equal call_usage.input_tokens, log.usage.input_tokens
    assert_equal call_usage.output_tokens, log.usage.output_tokens

    # Verify raw response exists
    refute_nil log.raw_llm_response

    assert_equal log.usage.input_tokens, collector.usage.input_tokens
    assert_equal log.usage.output_tokens, collector.usage.output_tokens

    # Verify metadata
    # assert_kind_of Hash, log.metadata

    collector = nil
    # Force GC to run
    GC.start
    # Still not collected because it's in use
    assert Baml::Collector.__function_span_count > 0 if Baml::Collector.respond_to?(:__function_span_count)
  end

  it "tests_collector_no_stream_no_getting_logs" do
    collector = Baml::Collector.new(name: "my-collector")
    function_logs = collector.logs
    assert_equal 0, function_logs.length

    b.TestOpenAIGPT4oMini(input: "hi there", baml_options: {collector: collector})

    # Force GC to run
    GC.start
    # Still not collected because it's in use
    assert Baml::Collector.__function_span_count > 0 if Baml::Collector.respond_to?(:__function_span_count)
  end

  it "tests_collector_stream_success" do
    collector = Baml::Collector.new(name: "my-collector")
    function_logs = collector.logs
    assert_equal 0, function_logs.length

    stream = b.stream.TestOpenAIGPT4oMini(input: "hi there", baml_options: {collector: collector})

    chunks = []
    stream.each do |chunk|
      puts "### chunk: #{chunk}"
      chunks << chunk
    end

    res = stream.get_final_response
    puts "### res: #{res}"

    function_logs = collector.logs
    assert_equal 1, function_logs.length

    log = collector.last
    refute_nil log
    assert_equal "TestOpenAIGPT4oMini", log.function_name
    assert_equal "call", log.log_type

    # Verify timing fields
    assert log.timing.start_time_utc_ms > 0
    refute_nil log.timing.duration_ms
    assert log.timing.duration_ms > 0

    # Verify usage fields
    refute_nil log.usage.input_tokens
    assert log.usage.input_tokens > 0
    refute_nil log.usage.output_tokens
    assert log.usage.output_tokens > 0

    # Verify calls
    calls = log.calls
    assert_equal 1, calls.length

    call = calls[0]
    assert_equal "openai", call.provider
    assert_equal "GPT4oMini", call.client_name
    assert call.selected

    # Verify request/response
    request = call.http_request
    refute_nil request
    assert_kind_of Hash, request.body
    assert_includes request.body, "messages"

    # For streaming, http_response is likely nil
    response = call.http_response
    assert_nil response

    # Verify call timing
    call_timing = call.timing
    assert call_timing.start_time_utc_ms > 0
    refute_nil call_timing.duration_ms
    assert call_timing.duration_ms > 0

    # Verify call usage
    call_usage = call.usage
    refute_nil call_usage.input_tokens
    assert call_usage.input_tokens > 0
    refute_nil call_usage.output_tokens
    assert call_usage.output_tokens > 0

    # Verify raw response exists
    refute_nil log.raw_llm_response

    # Force GC to run
    GC.start
    # Still not collected because it's in use
    assert Baml::Collector.__function_span_count > 0 if Baml::Collector.respond_to?(:__function_span_count)
  end

  it "tests_collector_multiple_calls_usage" do
    collector = Baml::Collector.new(name: "my-collector")

    # First call
    b.TestOpenAIGPT4oMini(input: "First call", baml_options: {collector: collector})
    function_logs = collector.logs
    assert_equal 1, function_logs.length

    # Capture usage after first call
    first_call_usage = function_logs[0].usage
    assert_equal first_call_usage.input_tokens, collector.usage.input_tokens
    assert_equal first_call_usage.output_tokens, collector.usage.output_tokens

    # Second call
    b.TestOpenAIGPT4oMini(input: "Second call", baml_options: {collector: collector})
    function_logs = collector.logs
    assert_equal 2, function_logs.length

    # Capture usage after second call and verify it's the sum of both calls
    second_call_usage = function_logs[1].usage
    total_input = first_call_usage.input_tokens + second_call_usage.input_tokens
    total_output = first_call_usage.output_tokens + second_call_usage.output_tokens
    assert_equal total_input, collector.usage.input_tokens
    assert_equal total_output, collector.usage.output_tokens
  end

  it "tests_collector_multiple_collectors" do
    coll1 = Baml::Collector.new(name: "collector-1")
    coll2 = Baml::Collector.new(name: "collector-2")

    # Pass in both collectors for the first call
    b.TestOpenAIGPT4oMini(input: "First call", baml_options: {collector: [coll1, coll2]})

    # Check usage/logs after the first call
    logs1 = coll1.logs
    logs2 = coll2.logs
    assert_equal 1, logs1.length
    assert_equal 1, logs2.length

    usage_first_call_coll1 = logs1[0].usage
    usage_first_call_coll2 = logs2[0].usage

    # Verify both collectors have the exact same usage for the first call
    assert_equal usage_first_call_coll1.input_tokens, usage_first_call_coll2.input_tokens
    assert_equal usage_first_call_coll1.output_tokens, usage_first_call_coll2.output_tokens

    # Also check that the collector-level usage matches the single call usage for each collector
    assert_equal usage_first_call_coll1.input_tokens, coll1.usage.input_tokens
    assert_equal usage_first_call_coll1.output_tokens, coll1.usage.output_tokens
    assert_equal usage_first_call_coll2.input_tokens, coll2.usage.input_tokens
    assert_equal usage_first_call_coll2.output_tokens, coll2.usage.output_tokens

    # Second call uses only coll1
    b.TestOpenAIGPT4oMini(input: "Second call", baml_options: {collector: coll1})

    # Re-check logs/usage
    logs1 = coll1.logs
    logs2 = coll2.logs
    assert_equal 2, logs1.length
    assert_equal 1, logs2.length

    # Verify coll1 usage is now the sum of both calls
    usage_second_call_coll1 = logs1[1].usage
    total_input = usage_first_call_coll1.input_tokens + usage_second_call_coll1.input_tokens
    total_output = usage_first_call_coll1.output_tokens + usage_second_call_coll1.output_tokens
    assert_equal total_input, coll1.usage.input_tokens
    assert_equal total_output, coll1.usage.output_tokens

    # Verify coll2 usage remains unchanged (it did not participate in the second call)
    assert_equal usage_first_call_coll2.input_tokens, coll2.usage.input_tokens
    assert_equal usage_first_call_coll2.output_tokens, coll2.usage.output_tokens
  end

  it "tests_collector_sync_calls" do
    collector = Baml::Collector.new(name: "sync-collector")

    # First call
    b.TestOpenAIGPT4oMini(input: "call #1", baml_options: {collector: collector})
    logs = collector.logs
    assert_equal 1, logs.length
    usage_first_call = logs[0].usage

    # Verify collector usage matches the first call's usage
    assert_equal usage_first_call.input_tokens, collector.usage.input_tokens
    assert_equal usage_first_call.output_tokens, collector.usage.output_tokens

    # Second call
    b.TestOpenAIGPT4oMini(input: "call #2", baml_options: {collector: collector})
    logs = collector.logs
    assert_equal 2, logs.length

    # Verify the second call's usage
    usage_second_call = logs[1].usage
    assert logs[1].timing.start_time_utc_ms > logs[0].timing.start_time_utc_ms
    total_input = usage_first_call.input_tokens + usage_second_call.input_tokens
    total_output = usage_first_call.output_tokens + usage_second_call.output_tokens
    assert_equal total_input, collector.usage.input_tokens
    assert_equal total_output, collector.usage.output_tokens
  end

  # Since Ruby doesn't have async/await patterns like Python,
  # the parallel calls test might need to use threads
  it "tests_collector_parallel_calls" do
    collector = Baml::Collector.new(name: "parallel-collector")

    # Execute two calls in parallel using threads
    threads = []
    threads << Thread.new { b.TestOpenAIGPT4oMini(input: "call #1", baml_options: {collector: collector}) }
    threads << Thread.new { b.TestOpenAIGPT4oMini(input: "call #2", baml_options: {collector: collector}) }
    threads.each(&:join)

    puts "------------------------- ended parallel calls"

    # Verify the collector has two function logs
    logs = collector.logs
    assert_equal 2, logs.length

    # Ensure each call is recorded properly
    logs.each do |log|
      assert_equal "TestOpenAIGPT4oMini", log.function_name
      assert_equal "call", log.log_type
    end

    # Check usage for each call
    usage_call1 = logs[0].usage
    usage_call2 = logs[1].usage
    refute_nil usage_call1
    refute_nil usage_call2

    # Verify that total collector usage equals the sum of the two logs
    total_input = usage_call1.input_tokens + usage_call2.input_tokens
    total_output = usage_call1.output_tokens + usage_call2.output_tokens
    assert_equal total_input, collector.usage.input_tokens
    assert_equal total_output, collector.usage.output_tokens
  end
end
