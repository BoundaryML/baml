# frozen_string_literal: true

require "minitest/autorun"
require "timeout"
$LOAD_PATH.unshift File.expand_path("../../../../sdks/ruby/bridge_ruby/lib", __dir__)
require "baml/bridge"

class BridgeCallTest < Minitest::Test
  Wire = BamlBridge::Cffi::V1
  Native = Baml::Bridge.const_get(:Native, false)

  class FakeApi
    attr_reader :callback, :initialize_count
    attr_accessor :dispatch

    def initialize
      @initialize_count = 0
      @engine_id = 2**40
    end

    def initialize_runtime(_bytes)
      @initialize_count += 1
    end

    def register_result_callback(callback)
      @callback = callback
    end

    def new_function_call
      @engine_id += 1
    end

    def call_function(bytes, callback_id)
      @dispatch.call(Wire::CallFunctionArgs.decode(bytes), callback_id)
    end

    def deliver(callback_id, **result)
      bytes = Wire::BamlOutboundResult.encode(Wire::BamlOutboundResult.new(**result))
      pointer = FFI::MemoryPointer.new(:uint8, [bytes.bytesize, 1].max)
      pointer.put_bytes(0, bytes)
      @callback.call(callback_id, pointer, bytes.bytesize)
    end
  end

  def setup
    @runtime = Baml::Bridge.const_get(:ProcessRuntime, false).new
    @api = FakeApi.new
    @runtime.define_singleton_method(:configured_runtime_path!) { "fake-runtime" }
    @runtime.define_singleton_method(:open_library) { |*_args| Object.new }
    Native::Api.stub(:new, @api) { @runtime.initialize!("bytecode") }
  end

  def test_separate_ids_wire_names_and_register_before_dispatch
    @api.dispatch = lambda do |args, id|
      assert_equal (2**40) + 1, args.call_id
      assert_equal 1, id
      assert_equal "user.namespace.OriginalName", args.function_name
      assert_equal "OriginalArg", args.kwargs.first.string_key
      assert_equal "hi", args.kwargs.first.value.string_value
      assert pending.key?(id)
      @api.deliver(id, ok: { string_value: "first" })
      @api.deliver(id, ok: { string_value: "duplicate" })
      @api.deliver(id + 100, ok: { string_value: "unknown" })
    end
    assert_equal "first", call("OriginalArg" => "hi")
    assert_empty pending
    assert_equal 1, @api.initialize_count
  end

  def test_concurrent_calls_receive_only_their_own_result
    dispatched = Queue.new
    @api.dispatch = ->(args, id) { dispatched << [args, id] }
    threads = 2.times.map { |i| Thread.new { call("value" => i) } }
    Timeout.timeout(5) do
      calls = 2.times.map { dispatched.pop }
      refute_equal calls[0][0].call_id, calls[1][0].call_id
      refute_equal calls[0][1], calls[1][1]
      calls.reverse_each do |args, id|
        @api.deliver(id, ok: { int_value: args.kwargs.first.value.int_value })
      end
      assert_equal [0, 1], threads.map(&:value)
    end
    assert_empty pending
    assert_equal 1, @api.initialize_count
  end

  def test_nil_encoding_and_decoding
    [{}, { null_value: {} }].each do |null_value|
      @api.dispatch = lambda do |args, id|
        assert_nil args.kwargs.first.value.value
        @api.deliver(id, ok: null_value)
      end
      assert_nil call("value" => nil)
    end
  end

  def test_callback_copy_errors_reach_caller_and_remove_pending_call
    @api.dispatch = ->(_args, id) { @api.callback.call(id, FFI::Pointer::NULL, 1) }
    error = Timeout.timeout(5) { assert_raises(ArgumentError) { call } }
    assert_includes error.message, "native callback pointer is null"
    assert_empty pending
    assert_nil @runtime.instance_variable_get(:@result_callback).pop_error
  end

  def test_callback_handler_errors_reach_caller
    callback = @runtime.instance_variable_get(:@result_callback)
    callback.instance_variable_set(:@handler, ->(*) { raise "handler failed" })
    @api.dispatch = ->(_args, id) { @api.deliver(id, ok: { int_value: 1 }) }
    error = Timeout.timeout(5) { assert_raises(RuntimeError) { call } }
    assert_equal "handler failed", error.message
    assert_empty pending
    assert_nil callback.pop_error
  end

  def test_dispatch_failure_removes_pending_call
    @api.dispatch = ->(*) { raise "dispatch failed" }
    error = assert_raises(RuntimeError) { call }
    assert_equal "dispatch failed", error.message
    assert_empty pending
  end

  def test_dispatch_failure_after_delivery_is_not_swallowed
    # The callback completed the call before dispatch unwound; the caller's
    # exception still wins and the registry stays clean.
    @api.dispatch = lambda do |_args, id|
      @api.deliver(id, ok: { string_value: "done" })
      raise Interrupt
    end
    assert_raises(Interrupt) { call }
    assert_empty pending
  end

  def test_unsupported_argument_and_result_kinds
    error = assert_raises(Baml::Bridge::UnsupportedTypeError) { call("value" => Object.new) }
    assert_includes error.message, "Object"
    assert_empty pending
    { handle_value: { key: 1 }, uint8array_value: "bytes" }.each do |kind, value|
      @api.dispatch = ->(_args, id) { @api.deliver(id, ok: { kind => value }) }
      error = assert_raises(Baml::Bridge::UnsupportedTypeError) { call }
      assert_includes error.message, kind.to_s
      assert_includes error.message, "not yet supported"
      assert_empty pending
    end
  end

  def test_exit_panic_is_rejected_before_payload_decoding
    @api.dispatch = ->(_args, id) { @api.deliver(id, panic: { is_exit_panic: true, exit_code: 7 }) }
    error = assert_raises(Baml::Bridge::UnsupportedTypeError) { call }
    assert_includes error.message, "is_exit_panic (exit code 7)"
    assert_empty pending
  end

  def test_empty_result_is_not_successful_nil
    @api.dispatch = ->(_args, id) { @api.deliver(id) }
    error = assert_raises(Baml::Bridge::Error) { call }
    assert_includes error.message, "empty result envelope"
    assert_empty pending
  end

  private

  def call(arguments = {})
    @runtime.call("bytecode", "user.namespace.OriginalName", arguments)
  end

  def pending
    @runtime.instance_variable_get(:@pending)
  end
end
