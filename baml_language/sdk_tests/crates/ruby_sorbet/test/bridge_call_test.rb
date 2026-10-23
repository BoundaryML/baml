# frozen_string_literal: true

require "minitest/autorun"
require "timeout"
$LOAD_PATH.unshift File.expand_path("../../../../sdks/ruby/bridge_ruby/lib", __dir__)
require "baml/bridge"

class BridgeCallTest < Minitest::Test
  Wire = BamlBridge::Cffi::V1
  Native = Baml::Bridge.const_get(:Native, false)

  class FakeApi
    attr_reader :callback, :initialize_count, :released_calls, :cancelled_calls
    attr_accessor :dispatch
    attr_reader :host_dispatch, :host_release, :host_cancel, :host_completions, :released_handles

    def initialize
      @initialize_count = 0
      @engine_id = 2**40
      @released_calls = []
      @cancelled_calls = []
      @host_completions = Queue.new
      @released_handles = Queue.new
    end

    def register_bridge!(_toolchain, _version); end
    def ensure_not_forked!; end
    def register_unhandled_spawn_error_callback(_callback); end
    def shutdown_runtime; end

    def handle_clone(key)
      key
    end

    def initialize_runtime(_bytes, _metadata = nil)
      @initialize_count += 1
    end

    def register_result_callback(callback)
      @callback = callback
    end

    def register_host_callbacks(dispatch, release, cancel)
      @host_dispatch, @host_release, @host_cancel = dispatch, release, cancel
    end

    def complete_host_call(id, is_error, bytes)
      @host_completions << [id, is_error, Wire::InboundValue.decode(bytes)]
    end

    def handle_release(key)
      @released_handles << key
    end

    def new_function_call
      @engine_id += 1
    end

    def release_function_call(call_id)
      @released_calls << call_id
    end

    def cancel_function_call(call_id)
      @cancelled_calls << call_id
      0
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
    @runtime.register_program("bytecode")
    Native::Api.stub(:new, @api) { @runtime.initialize! }
  end

  def test_result_handles_are_owned_before_wait_and_after_partial_decode
    handle = { handle_value: { key: 41, handle_type: :ADT_TAGGED_HEAP_HANDLE } }
    @api.dispatch = ->(_args, id) { @api.deliver(id, ok: { list_value: { items: [handle, handle] } }) }
    Thread.new { @runtime.call_async("user.test", {}); nil }.join
    Timeout.timeout(5) { GC.start until @api.released_handles.size == 2 }
    assert_equal [41, 41], 2.times.map { @api.released_handles.pop }

    @api.dispatch = lambda do |_args, id|
      @api.deliver(id, ok: { list_value: { items: [handle, { prompt_ast_value: {} }, handle] } })
    end
    assert_raises(Baml::Bridge::UnsupportedTypeError) { call }
    assert_equal 41, @api.released_handles.pop # The occurrence decoding never reached.
    Timeout.timeout(5) { GC.start until @api.released_handles.size == 1 }
    assert_equal 41, @api.released_handles.pop

    @api.dispatch = ->(_args, id) { @api.deliver(id, ok: { list_value: { items: [handle, handle] } }) }
    owners = call
    refute_same owners.first, owners.last
    GC.start
    assert @api.released_handles.empty?
    owners.each(&:close)
    assert_equal [41, 41], 2.times.map { @api.released_handles.pop }
  end

  def test_interrupted_callback_registration_is_retained_and_completed_once
    runtime = Baml::Bridge.const_get(:ProcessRuntime, false).new
    runtime.register_program("bytecode")
    runtime.instance_variable_set(:@library, Object.new)
    api = FakeApi.new
    attempts = Hash.new(0)
    { register_bridge!: nil, register_result_callback: :@result_callback,
      register_host_callbacks: :@host_callbacks, register_unhandled_spawn_error_callback: :@spawn_callback }.each do |method, owner|
      api.define_singleton_method(method) do |*args|
        raise "callback was not retained" if owner && !runtime.instance_variable_get(owner)
        attempts[method] += 1
        Thread.current.raise Interrupt
        super(*args)
      end
    end
    Native::Api.stub(:new, api) do
      4.times { assert_raises(Interrupt) { runtime.initialize! } }
      runtime.initialize!
    end
    assert_equal [1, 1, 1, 1], attempts.values
    assert_equal 1, api.initialize_count
    assert_same runtime.instance_variable_get(:@result_callback).function, api.callback
    refute_nil api.host_dispatch
    refute_nil api.host_release
    refute_nil api.host_cancel
  end

  def test_callback_returns_and_closure_arguments_preserve_bytes
    Baml::Bridge.define_function("user.bytes_callback", params: [["cb", [:function, [], [:bytes]]]])
    @api.dispatch = lambda do |args, id|
      key = args.kwargs.first.value.handle.key
      frame = Wire::HostInvocation.encode(Wire::HostInvocation.new(
        host_value_key: key, callback_id: 123,
        application_args: Wire::BamlToHostCall.encode(Wire::BamlToHostCall.new)
      ))
      pointer = FFI::MemoryPointer.from_string(frame)
      @api.host_dispatch.call(pointer, frame.bytesize)
      _, is_error, value = Timeout.timeout(5) { @api.host_completions.pop }
      assert_equal 0, is_error
      assert_equal :uint8array_value, value.value
      @api.host_release.call(key)
      @api.deliver(id, ok: { uint8array_value: value.uint8array_value })
    end
    assert_equal "\0bytes".b, @runtime.call_async("user.bytes_callback", { "cb" => -> { "\0bytes".b } }).wait

    @api.dispatch = lambda do |_args, id|
      @api.deliver(id, ok: { handle_value: { key: 77, handle_type: :FUNCTION_REF, ty: { function: {
        params: [{ name: "data", mode: :BAML_TY_FUNCTION_PARAM_MODE_REQUIRED,
                   ty: { primitive: { kind: :BAML_TY_PRIMITIVE_BYTES } } }]
      } } } })
    end
    closure = call
    @api.dispatch = lambda do |args, id|
      assert_equal :uint8array_value, args.kwargs.first.value.value
      @api.deliver(id, ok: { uint8array_value: args.kwargs.first.value.uint8array_value })
    end
    Baml::Bridge.stub(:call_async, ->(target, args, **options) { @runtime.call_async(target, args, **options) }) do
      assert_equal "\0arg".b, closure.call("\0arg".b)
    end
  ensure
    closure&.close
  end

  class GenericValue < T::Struct
    const :value, String

    def call
      raise "a registered class must not be encoded as a callback"
    end
  end

  def test_decoded_generic_preserves_string_literal_and_alias_hints
    protocol = Baml::Bridge::Protocol
    Baml::Bridge.define_class("user.GenericValue", GenericValue, fields: { "value" => [:value, [:param, 0, "T"]] }, type_params: ["T"])
    protocol.define_alias("user.Text", [:string])
    [{ primitive: { kind: :BAML_TY_PRIMITIVE_STRING } }, { literal: { string_value: "text" } },
     { type_alias: { name: "user.Text" } }].each do |text_type|
      type = { union: { options: [text_type, { primitive: { kind: :BAML_TY_PRIMITIVE_BYTES } }] } }
      [:string_value, :uint8array_value].each do |kind|
        @api.dispatch = lambda do |args, id|
          if args.kwargs.empty?
            @api.deliver(id, ok: { class_value: { name: "user.GenericValue", type_args: [type], fields: [
              { key: "value", value: { kind => "text" } }
            ] } })
          else
            assert_equal kind, args.kwargs.first.value.class_value.fields.first.value.value
            @api.deliver(id, ok: { bool_value: true })
          end
        end
        assert call("value" => call)
      end
    end
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

  def test_callback_capture_errors_reach_caller
    @api.dispatch = ->(_args, id) { @api.deliver(id, ok: { int_value: 1 }) }
    Baml::Bridge::Protocol.stub(:capture_result, ->(*) { raise "capture failed" }) do
      error = Timeout.timeout(5) { assert_raises(RuntimeError) { call } }
      assert_equal "capture failed", error.message
    end
    assert_empty pending
  end

  def test_dispatch_failure_removes_pending_call
    callback = ->(value) { value }
    registry = Baml::Bridge.const_get(:HostValues, false)
    key = nil
    @api.dispatch = lambda do |args, _id|
      key = args.kwargs.first.value.handle.key
      assert_same callback, registry.fetch(key)
      raise "dispatch failed"
    end
    error = assert_raises(RuntimeError) { call("callback" => callback) }
    assert_equal "dispatch failed", error.message
    assert_raises(Baml::Bridge::Error) { registry.fetch(key) }
    assert_equal [(2**40) + 1], @api.released_calls
    assert_empty pending
  end

  def test_interrupt_after_allocation_releases_call
    @api.define_singleton_method(:new_function_call) do
      call_id = super()
      Thread.current.raise Interrupt
      call_id
    end
    assert_raises(Interrupt) { call }
    assert_equal [(2**40) + 1], @api.released_calls
    assert_empty pending
  end

  def test_call_waiters_cache_outcomes_and_survive_interrupt_and_cancel
    dispatched = Queue.new
    @api.dispatch = ->(args, id) { dispatched << [args.call_id, id] }
    result = @runtime.call_async("user.test", {})
    engine_id, callback_id = dispatched.pop
    refute result.done?
    assert_raises(Baml::WaitTimeout) { result.wait(timeout: 0.001) }
    refute result.done?

    interrupted = Thread.new do
      result.wait
    rescue Interrupt => error
      error
    end
    Thread.pass until interrupted.status == "sleep"
    interrupted.raise(Interrupt)
    assert_instance_of Interrupt, interrupted.value
    refute result.done?

    waiters = 3.times.map { Thread.new { result.wait(timeout: 5) } }
    Thread.pass until waiters.all? { |thread| thread.status == "sleep" }
    assert result.cancel
    refute result.cancel
    assert_equal [engine_id], @api.cancelled_calls
    refute result.done?

    callback = @runtime.instance_variable_get(:@result_callback)
    handler = callback.instance_variable_get(:@handler)
    delivering_threads = Queue.new
    callback.instance_variable_set(:@handler, lambda do |id, bytes|
      delivering_threads << Thread.current
      handler.call(id, bytes)
    end)
    library = FFI::DynamicLibrary.open(ENV.fetch("BAML_RUBY_TEST_THREAD_FIXTURE"), FFI::DynamicLibrary::RTLD_NOW)
    invoke = FFI::Function.new(
      :void, %i[pointer uint32 pointer size_t],
      library.find_function("baml_test_invoke_on_native_thread"), blocking: true
    )
    bytes = Wire::BamlOutboundResult.encode(Wire::BamlOutboundResult.new(ok: { list_value: { items: [{ int_value: 42 }] } }))
    pointer = FFI::MemoryPointer.new(:uint8, bytes.bytesize)
    pointer.put_bytes(0, bytes)
    GC.start
    invoke.call(@api.callback, callback_id, pointer, bytes.bytesize)
    refute_same Thread.current, delivering_threads.pop
    values = waiters.map(&:value)
    assert_equal [42], values.first
    values.each { |value| assert_same values.first, value }
    assert_same values.first, result.wait(timeout: 0)
    assert result.done?
    refute result.cancel

    failed = @runtime.call_async("user.test", {})
    _, callback_id = dispatched.pop
    @api.deliver(callback_id, error: { value: { string_value: "cached error" } })
    assert failed.done?
    error = assert_raises(Baml::Error) { failed.wait }
    assert_equal "cached error", error.message
    assert_same error, assert_raises(Baml::Error) { failed.wait }
    assert_empty pending
  end

  def test_cancelled_host_callback_keeps_resources_until_proc_returns
    registry = Baml::Bridge.const_get(:HostValues, false)
    scope = Baml::Bridge.const_get(:EncodingScope, false).new
    entered = Queue.new
    unblock = Queue.new
    exited = Queue.new
    cb = ->(value) { entered << value; unblock.pop; exited << true; "finished" }
    key = registry.register(cb, scope)
    scope.submitted!
    frame = Wire::HostInvocation.new(
      host_value_key: key, callback_id: 123, effective_state: 77,
      application_args: Wire::BamlToHostCall.encode(Wire::BamlToHostCall.new(args: [{ value: { int_value: 5 } }])),
      cancel: { class_value: { name: "baml.spawn.CancelToken", fields: [
        { key: "inner", value: { handle_value: { key: 88, handle_type: :ADT_TAGGED_HEAP_HANDLE } } }
      ] } }
    )
    bytes = Wire::HostInvocation.encode(frame)
    pointer = FFI::MemoryPointer.new(:uint8, bytes.bytesize)
    pointer.put_bytes(0, bytes)
    Timeout.timeout(5) do
      # Dispatch must return while the proc is blocked. Overwrite the borrowed
      # buffer immediately to prove the invocation owns its request bytes.
      @api.host_dispatch.call(pointer, bytes.bytesize)
      pointer.put_bytes(0, "\0" * bytes.bytesize)
      assert_equal 5, entered.pop
      @api.host_cancel.call(123)
      assert_same cb, registry.fetch(key)
      assert @api.released_handles.empty?
      assert @api.host_completions.empty?
      assert exited.empty?
      unblock << true
      assert exited.pop
      id, is_error, value = @api.host_completions.pop
      assert_equal [123, 0, "finished"], [id, is_error, value.string_value]
      assert_equal [77, 88], 2.times.map { @api.released_handles.pop }.sort
      @api.host_cancel.call(123)
      assert @api.host_completions.empty?, "late cancellation must not complete twice"
      assert @api.released_handles.empty?, "late cancellation must not release twice"
      # Host keys in results are borrowed too: pin before engine release,
      # even when the first waiter does not run until after that release.
      @api.dispatch = lambda do |_args, id|
        @api.deliver(id, ok: { handle_value: { key: key, handle_type: :HOST_VALUE_CALLABLE } })
        @api.host_release.call(key)
      end
      result = @runtime.call_async("user.return_callback", {})
      assert_raises(Baml::Bridge::Error) { registry.fetch(key) }
      assert_same cb, result.wait
    end
  ensure
    unblock << true if unblock
    registry&.handle_release(key) if key
  end

  private

  def call(arguments = {})
    @runtime.call_async("user.namespace.OriginalName", arguments).wait
  end

  def pending
    @runtime.instance_variable_get(:@pending)
  end
end
