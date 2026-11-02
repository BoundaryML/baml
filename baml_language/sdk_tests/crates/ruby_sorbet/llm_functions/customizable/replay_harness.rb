# frozen_string_literal: true

require "net/http"
require "pathname"
require "tempfile"
require "timeout"

module ReplayHarness
  module_function

  def recording_path(name)
    Pathname.new(__dir__).ascend do |parent|
      next unless parent.basename.to_s == "sdk_tests"

      recording = parent.join("fixtures/llm_functions/recordings/#{name}.snap.sse")
      raise "missing recording #{recording}" unless recording.file?

      return recording.to_s
    end
    raise "could not locate the sdk_tests/ ancestor directory"
  end

  ENV_KEYS = %w[BAML_REPLAY_BASE_URL BAML_REPLAY_API_KEY].freeze

  def with_server(recording)
    recording = recording_path(recording)
    saved_env = ENV.to_h.slice(*ENV_KEYS)
    Tempfile.create("baml-ruby-replay") do |file|
      addr_file = file.path
      file.close
      addr = nil
      thread = Thread.new do
        Thread.current.report_on_exception = false
        BamlSdk::Replay.replay_serve_until_shutdown(recording, addr_file)
      end
      begin
        deadline = Process.clock_gettime(Process::CLOCK_MONOTONIC) + 10
        loop do
          unless thread.alive?
            thread.value # Re-raise the original bridge/server failure.
            raise "replay server exited before binding"
          end
          text = File.read(addr_file).strip
          unless text.empty?
            addr = text
            break
          end
          if Process.clock_gettime(Process::CLOCK_MONOTONIC) >= deadline
            raise Timeout::Error, "replay server did not bind within 10s"
          end
          sleep 0.02
        end
        ENV["BAML_REPLAY_BASE_URL"] = "http://#{addr}"
        ENV["BAML_REPLAY_API_KEY"] = "replay-test-key"
        yield addr
      ensure
        begin
          if addr && thread.alive?
            uri = URI("http://#{addr}/__replay__/shutdown")
            Net::HTTP.start(uri.host, uri.port, open_timeout: 5, read_timeout: 5) do |http|
              response = http.post(uri.request_uri, "")
              raise "replay shutdown failed: #{response.code}" unless response.is_a?(Net::HTTPSuccess)
            end
          end
        ensure
          begin
            # A server that will not stop fails this test; the process exit reaps it.
            raise Timeout::Error, "replay server did not stop within 10s" unless thread.join(10)
          ensure
            ENV_KEYS.each { |key| saved_env.key?(key) ? ENV[key] = saved_env[key] : ENV.delete(key) }
          end
        end
      end
    end
  end
end
