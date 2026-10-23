# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"

ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class CancellationTest < Minitest::Test
  def test_cancellation_sync_call_returns_none
    assert_nil BamlSdk::ThrowsTest.sleep_ms(1)
  end

  def test_cancellation_async_call_returns_none
    assert_nil BamlSdk::ThrowsTest.sleep_ms_async(1).wait
  end

  def test_cancellation_sync_cancel_via_call_context
    token = BamlSdk::Baml::Spawn::CancelToken.baml_new
    start = monotonic
    timer = Thread.new { sleep 0.05; token.cancel }
    error = assert_raises(Baml::CancelledError) do
      BamlSdk::ThrowsTest.sleep_ms(60_000, _baml: Baml::Options.new(cancel: token))
    end
    assert_cancelled(error, start)
  ensure
    timer&.kill&.join
  end

  def test_cancellation_async_cancel_via_call_context
    token = BamlSdk::Baml::Spawn::CancelToken.baml_new
    start = monotonic
    call = BamlSdk::ThrowsTest.sleep_ms_async(60_000, _baml: Baml::Options.new(cancel: token))
    sleep 0.05
    token.cancel
    assert_cancelled(assert_raises(Baml::CancelledError) { call.wait(timeout: 5) }, start)
  end

  def test_cancellation_async_cancel_via_task_cancel
    start = monotonic
    call = BamlSdk::ThrowsTest.sleep_ms_async(60_000)
    sleep 0.05
    assert call.cancel
    assert_cancelled(assert_raises(Baml::CancelledError) { call.wait(timeout: 5) }, start)
  end

  def test_cancellation_async_cancel_via_task_group_sibling
    # Ruby has no TaskGroup. Like Java's port, a failing sibling explicitly
    # cancels the other call, and the original failure remains observable.
    start = monotonic
    call = BamlSdk::ThrowsTest.sleep_ms_async(60_000)
    sibling = Thread.new do
      sleep 0.05
      raise "cancel siblings"
    rescue RuntimeError => error
      call.cancel
      error
    end
    assert_instance_of RuntimeError, sibling.value
    assert_equal "cancel siblings", sibling.value.message
    assert_cancelled(assert_raises(Baml::CancelledError) { call.wait(timeout: 5) }, start)
  end

  def test_cancellation_async_cancel_via_asyncio_timeout
    # A bounded wait leaves the call running; explicit cancel cleans it up.
    start = monotonic
    call = BamlSdk::ThrowsTest.sleep_ms_async(60_000)
    assert_raises(Baml::WaitTimeout) { call.wait(timeout: 0.05) }
    refute call.done?
    assert call.cancel
    assert_cancelled(assert_raises(Baml::CancelledError) { call.wait(timeout: 5) }, start)

    start = monotonic
    timed = BamlSdk::ThrowsTest.sleep_ms_async(60_000, _baml: Baml::Options.new(timeout_ms: 50))
    assert_cancelled(assert_raises(Baml::CancelledError) { timed.wait(timeout: 5) }, start)
  end

  private

  def monotonic
    Process.clock_gettime(Process::CLOCK_MONOTONIC)
  end

  def assert_cancelled(error, start)
    assert_kind_of Baml::Panic, error
    refute_kind_of StandardError, error
    assert_instance_of BamlSdk::Baml::Panics::Cancelled, error.value
    # Java's 5s bound avoids loaded-runner flakes; the call would take 60s.
    assert_operator monotonic - start, :<, 5
  end
end
