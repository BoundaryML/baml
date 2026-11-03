# frozen_string_literal: true
require "json"
require "minitest/autorun"
require "tempfile"
require "baml_sdk"
require_relative "replay_harness"

ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class FunctionSpecCallTest < Minitest::Test
  # A non-streaming OpenAI Responses reply; the checked-in recordings are SSE.
  REPLY = {
    id: "resp_ruby", model: "gpt-5.4-nano", status: "completed",
    output: [{
      type: "message", id: "msg_ruby", status: "completed", role: "assistant",
      content: [{ type: "output_text", text: "A replayed summary." }]
    }],
    usage: { input_tokens: 1, output_tokens: 3, total_tokens: 4 }
  }.freeze

  # SDK_PARITY_LINT(skip): no shared test runs a spec non-streaming; Ruby replays an inline Responses body
  def test_function_spec_call_uses_the_default_or_a_host_built_client
    Tempfile.create(["baml-ruby-reply", ".json"]) do |reply|
      reply.write(JSON.generate(REPLY))
      reply.close
      ReplayHarness.with_server(path: reply.path) do
        spec = BamlSdk::Lorem.stream_e2e_extract_spec("ignored-by-replay-server")
        assert_equal "A replayed summary.", spec.call
        assert_equal "A replayed summary.", spec.call_async.wait
        assert_equal "A replayed summary.", spec.call(client: client(ENV.fetch("BAML_REPLAY_BASE_URL")))

        # A client that can't be reached proves the override replaced the default.
        assert_raises(Baml::Error) { spec.call(client: client("http://127.0.0.1:1")) }
        assert_equal "A replayed summary.", spec.call
        spec.close
      end
    end
  end

  private

  def client(base_url)
    BamlSdk::Vendor::Openai::ResponsesClient.baml_new(
      model: "gpt-5.4-nano", api_key: ENV.fetch("BAML_REPLAY_API_KEY"), base_url: base_url
    )
  end
end
