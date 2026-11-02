# frozen_string_literal: true

require "minitest/autorun"
require "timeout"
require "open3"
BRIDGE_LIB = File.expand_path("../../../../sdks/ruby/bridge_ruby/lib", __dir__)
$LOAD_PATH.unshift BRIDGE_LIB
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

    def complete_host_call(id, bytes)
      @host_completions << [id, Wire::HostCallCompletion.decode(bytes)]
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

  def released_handle
    Timeout.timeout(5) { @api.released_handles.pop }
  end

  def test_shutdown_allows_an_active_callback_to_reject_a_nested_call
    entered, finish = Queue.new, Queue.new
    callback = Thread.new do
      entered << true
      finish.pop
      begin
        @runtime.call_async("user.probe", {})
      rescue Baml::Bridge::Error => error
        error.message
      end
    end
    Timeout.timeout(5) { entered.pop }
    @api.define_singleton_method(:shutdown_runtime) do
      finish << true
      raise "callback deadlocked during shutdown" unless callback.join(2)
    end
    @runtime.send(:shutdown!)
    assert_equal "BAML runtime has shut down", callback.value
  ensure
    finish << true if finish
    callback&.kill
    callback&.join(5)
  end

  def test_native_shutdown_is_interruptible_and_not_repeated
    entered, finish = Queue.new, Queue.new
    shutdowns = 0
    @api.define_singleton_method(:shutdown_runtime) do
      shutdowns += 1
      entered << true
      finish.pop
    end
    worker = Thread.new { @runtime.send(:shutdown!) }
    Timeout.timeout(5) { entered.pop }
    worker.kill
    assert worker.join(1), "native shutdown could not be killed"
    @runtime.send(:shutdown!)
    assert_equal 1, shutdowns
  ensure
    finish << true if finish
    worker&.kill
    worker&.join(5)
  end

  def test_runtime_initialized_by_minitest_exit_hook_shuts_down_once
    source = <<~'RUBY'
      require "minitest/autorun"
      require "baml/bridge"
      class ExitOrderTest < Minitest::Test
        def test_first_call
          runtime = Baml::Bridge.instance_variable_get(:@process_runtime)
          api = Object.new
          def api.initialize_runtime(*); puts "initialize"; end
          def api.shutdown_runtime; puts "shutdown"; end
          runtime.instance_variable_set(:@api, api)
          runtime.instance_variable_set(:@library, Object.new)
          runtime.instance_variable_set(:@bridge_registered, true)
          runtime.instance_variable_set(:@result_registered, true)
          runtime.instance_variable_set(:@spawn_registered, true)
          runtime.instance_variable_set(:@host_registered, true)
          runtime.register_program("bytecode")
          2.times { runtime.initialize! }
        end
      end
    RUBY
    stdout, stderr, status = Open3.capture3(RbConfig.ruby, "-I", BRIDGE_LIB, "-e", source)
    assert status.success?, "#{stdout}\n#{stderr}"
    assert_equal 1, stdout.lines.count { |line| line.strip == "initialize" }
    assert_equal 1, stdout.lines.count { |line| line.strip == "shutdown" }
    assert_match(/shutdown\n\z/, stdout)
  end

  def test_native_callback_children_are_killable_after_masked_initialization
    source = <<~'RUBY'
      require "baml/bridge"
      native = Baml::Bridge.const_get(:Native, false)
      runtime = Baml::Bridge.const_get(:ProcessRuntime, false).new
      runtime.register_program("bytecode")
      ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_FIXTURE")
      ENV["BAML_FAKE_RUNTIME_VERSION"] = Baml::Bridge::TOOLCHAIN_VERSION
      Thread.handle_interrupt(Object => :never) { runtime.initialize! }
      children = Queue.new
      callback = native::BorrowedBytesCallback.new { |*| children << Thread.new { sleep } }
      library = FFI::DynamicLibrary.open(ENV.fetch("BAML_RUBY_TEST_THREAD_FIXTURE"), FFI::DynamicLibrary::RTLD_NOW)
      invoke = FFI::Function.new(:void, %i[pointer uint32 pointer size_t],
        library.find_function("baml_test_invoke_on_native_thread"), blocking: true)
      # Like Tokio, this fixture enters Ruby from a foreign native thread.
      invoke.call(callback.function, 1, FFI::Pointer::NULL, 0)
      child = children.pop(timeout: 5) or abort "native callback did not run"
      child.kill
      abort "FFI callback runner passed its interrupt mask to a child" unless child.join(1)
      puts "killed"
    RUBY
    Open3.popen3(RbConfig.ruby, "-I", BRIDGE_LIB, "-e", source) do |stdin, stdout, stderr, child|
      stdin.close
      exited = child.join(15)
      Process.kill("KILL", child.pid) unless exited
      status = child.value
      output, diagnostic = stdout.read, stderr.read
      assert exited, "native callback subprocess hung:\n#{output}\n#{diagnostic}"
      assert status.success?, "#{output}\n#{diagnostic}"
      assert_equal "killed\n", output
    end
  end

  def test_spawn_callback_threads_and_interrupted_registration_are_safe_at_exit
    source = <<~'RUBY'
      require "baml/bridge"
      runtime = Baml::Bridge.const_get(:ProcessRuntime, false).new
      runtime.register_program("bytecode")
      api = Object.new
      wire = BamlBridge::Cffi::V1
      bytes = wire::BamlOutboundResult.encode(wire::BamlOutboundResult.new(error: { value: { string_value: "spawn failed" } }))
      pointer = FFI::MemoryPointer.from_string(bytes)
      registrations = initializations = 0
      api.define_singleton_method(:register_unhandled_spawn_error_callback) do |callback|
        registrations += 1
        callback.call(pointer, bytes.bytesize, 1)
      end
      api.define_singleton_method(:initialize_runtime) { |*| initializations += 1 }
      api.define_singleton_method(:shutdown_runtime) {}
      runtime.instance_variable_set(:@api, api)
      runtime.instance_variable_set(:@library, Object.new)
      %i[bridge result host].each { |name| runtime.instance_variable_set(:"@#{name}_registered", true) }
      children = []
      entered, resume = Queue.new, Queue.new
      output = Object.new
      output.define_singleton_method(:write) do |*|
        if ARGV.first == "kill"
          entered << true
          resume.pop(timeout: 5) or raise "spawn output was not interrupted"
        elsif children.empty?
          children << Thread.new { sleep }
        end
      end
      output.define_singleton_method(:flush) {}
      $stderr = output
      worker = Thread.new { runtime.initialize! }
      if ARGV.first == "kill"
        entered.pop(timeout: 5) or abort "spawn output did not run"
        worker.kill
      end
      abort "initialization did not return" unless worker.join(1)
      $stderr = STDERR
      abort "callback registration was lost" unless runtime.instance_variable_get(:@spawn_registered)
      if ARGV.first == "kill"
        abort "killed initialization was marked complete" if runtime.instance_variable_get(:@initialized)
        runtime.initialize!
      end
      abort "registration repeated" unless registrations == 1
      abort "initialization did not complete once" unless initializations == 1
      at_exit do
        children.each do |child|
          child.kill
          abort "spawn callback passed its interrupt mask to a child" unless child.join(1)
        end
        puts "finished"
      end
    RUBY
    %w[child kill].each do |mode|
      Open3.popen3(RbConfig.ruby, "-I", BRIDGE_LIB, "-e", source, mode) do |stdin, stdout, stderr, child|
        stdin.close
        exited = child.join(15)
        Process.kill("KILL", child.pid) unless exited
        status = child.value
        output, diagnostic = stdout.read, stderr.read
        assert exited, "spawn callback subprocess hung (#{mode}):\n#{output}\n#{diagnostic}"
        assert status.success?, "#{output}\n#{diagnostic}"
        assert_equal "finished\n", output
      end
    end
  end

  def test_spawn_callback_logs_errors_and_panics_and_exits_unless_cancelled
    source = <<~'RUBY'
      require "baml/bridge"
      wire = BamlBridge::Cffi::V1
      result, cancelled = ARGV
      bytes = wire::BamlOutboundResult.encode(wire::BamlOutboundResult.new(
        result.to_sym => { value: { string_value: "spawn failed" } }
      ))
      pointer = FFI::MemoryPointer.from_string(bytes)
      runtime = Baml::Bridge.const_get(:ProcessRuntime, false).new
      at_exit { puts "exit-hook" }
      runtime.send(:spawn_callback).call(pointer, bytes.bytesize, Integer(cancelled))
      puts "returned"
    RUBY
    %w[error panic].product([0, 1]).each do |result, cancelled|
      Open3.popen3(RbConfig.ruby, "-I", BRIDGE_LIB, "-e", source, result, cancelled.to_s) do |stdin, stdout, stderr, child|
        stdin.close
        exited = child.join(15)
        Process.kill("KILL", child.pid) unless exited
        status = child.value
        output, diagnostic = stdout.read, stderr.read
        assert exited, "spawn callback subprocess hung (#{result}, #{cancelled}):\n#{output}\n#{diagnostic}"
        assert_equal cancelled.zero? ? 1 : 0, status.exitstatus, diagnostic
        assert_equal cancelled.zero? ? "" : "returned\nexit-hook\n", output
        assert_includes diagnostic, "spawn failed"
        assert_includes diagnostic, result == "panic" ? "Baml::Panic" : "Baml::Error"
      end
    end
  end

  def test_spawn_callback_does_not_swallow_process_interrupts
    callback = @runtime.instance_variable_get(:@spawn_callback)
    pointer = FFI::MemoryPointer.from_string("")
    [Interrupt.new, SignalException.new("TERM")].each do |error|
      Baml::Bridge::Protocol.stub(:decode_result, ->(*) { raise error }) do
        assert_same error, assert_raises(error.class) { callback.call(pointer, 0, 1) }
      end
    end
  end

  def test_handle_mutex_waits_are_killable
    owner = Baml::Handle.new(@api, 41, :ADT_TAGGED_HEAP_HANDLE)
    mutex = owner.instance_variable_get(:@mutex)
    scope = Baml::Bridge::EncodingScope.new
    [:close, :clone_for_wire].each do |operation|
      worker = nil
      mutex.synchronize do
        worker = Thread.new { operation == :close ? owner.close : owner.clone_for_wire(scope) }
        Timeout.timeout(5) { Thread.pass while worker.alive? && worker.status != "sleep" }
        assert worker.alive?
        worker.kill
        assert worker.join(1), "#{operation} could not be killed while waiting for the mutex"
        assert @api.released_handles.empty?
      end
    ensure
      worker&.kill
      worker&.join(5)
    end
  ensure
    scope&.close
    owner&.close
  end

  def test_application_exit_hook_can_call_before_runtime_shutdown
    source = <<~'RUBY'
      require "baml/bridge"
      runtime = Baml::Bridge.instance_variable_get(:@process_runtime)
      api = Object.new
      def api.initialize_runtime(*); end
      def api.shutdown_runtime; @shutdown = true; puts "shutdown"; end
      def api.new_function_call
        raise Baml::Bridge::Error, "call IDs are exhausted" if @shutdown
        1
      end
      def api.call_function(*); puts "call"; end
      def api.release_function_call(*); end
      runtime.instance_variable_set(:@api, api)
      runtime.instance_variable_set(:@library, Object.new)
      runtime.instance_variable_set(:@bridge_registered, true)
      runtime.instance_variable_set(:@result_registered, true)
      runtime.instance_variable_set(:@spawn_registered, true)
      runtime.instance_variable_set(:@host_registered, true)
      runtime.register_program("bytecode")
      at_exit { Baml::Bridge.call_async("user.probe", {}) }
      Baml::Bridge.call_async("user.probe", {})
    RUBY
    stdout, stderr, status = Open3.capture3(RbConfig.ruby, "-I", BRIDGE_LIB, "-e", source)
    assert status.success?, stderr
    assert_equal "call\ncall\nshutdown\n", stdout
  end

  def test_shutdown_skips_an_uninitialized_api_and_rejects_later_calls
    runtime = Baml::Bridge.const_get(:ProcessRuntime, false).new
    api = Object.new
    def api.shutdown_runtime; raise "uninitialized API was shut down"; end
    runtime.instance_variable_set(:@api, api)
    runtime.send(:shutdown!)
    @runtime.send(:shutdown!)
    error = assert_raises(Baml::Bridge::Error) { @runtime.call_async("user.probe", {}) }
    assert_equal "BAML runtime has shut down", error.message
  end

  def test_exit_panic_flushes_piped_output_when_decoded
    source = <<~'RUBY'
      require "baml/bridge"
      protocol = Baml::Bridge::Protocol
      result = protocol::Wire::BamlOutboundResult.new(panic: { is_exit_panic: true, exit_code: 7 })
      captured = protocol::Result.new(result, nil, nil)
      at_exit { puts "UNREACHABLE" }
      print "before exit"
      warn "before exit on stderr"
      protocol.decode_result(captured)
    RUBY
    stdout, stderr, status = Open3.capture3(RbConfig.ruby, "-I", BRIDGE_LIB, "-e", source)
    assert_equal 7, status.exitstatus, stderr
    assert_equal "before exit", stdout
    assert_equal "before exit on stderr\n", stderr
  end

  def test_exit_panic_flushes_original_streams_when_globals_are_replaced
    source = <<~'RUBY'
      require "baml/bridge"
      require "stringio"
      STDOUT.sync = STDERR.sync = false
      STDOUT.print "original stdout"
      STDERR.print "original stderr"
      $stdout = StringIO.new
      $stderr = StringIO.new
      Baml::Bridge::Protocol.exit_process(7)
    RUBY
    stdout, stderr, status = Open3.capture3(RbConfig.ruby, "-I", BRIDGE_LIB, "-e", source)
    assert_equal 7, status.exitstatus, stderr
    assert_equal "original stdout", stdout
    assert_equal "original stderr", stderr
  end

  def test_cancel_waiting_for_the_mutex_is_interruptible
    pending = Baml::Call.new(@api, 123)
    mutex = pending.instance_variable_get(:@mutex)
    entered, release = Queue.new, Queue.new
    holder = Thread.new { mutex.synchronize { entered << true; release.pop } }
    Timeout.timeout(5) { entered.pop }
    assert_raises(Timeout::Error) { Timeout.timeout(0.05) { pending.cancel } }
    assert_empty @api.cancelled_calls
    refute pending.instance_variable_get(:@cancel_requested)
    release << true
    holder.join
    assert pending.cancel
    assert_equal [123], @api.cancelled_calls
  ensure
    release << true if release
    holder&.join(5)
  end

  def test_handle_close_defers_kill_until_the_key_is_cleared
    owner = Baml::Handle.new(@api, 41, :ADT_TAGGED_HEAP_HANDLE)
    sibling = Baml::Handle.new(@api, 41, :ADT_TAGGED_HEAP_HANDLE)
    entered, resume = Queue.new, Queue.new
    @api.define_singleton_method(:handle_release) do |key|
      super(key)
      if Thread.current[:kill_after_release]
        entered << true
        resume.pop
      end
    end
    closer = Thread.new do
      Thread.current[:kill_after_release] = true
      owner.close
    end
    Timeout.timeout(5) { entered.pop }
    closer.kill
    resume << true
    assert closer.join(5), "handle close did not finish"
    assert_equal 41, released_handle
    owner.close
    assert @api.released_handles.empty?, "the killed close released its reference twice"
    scope = Baml::Bridge::EncodingScope.new
    assert_equal [41, :ADT_TAGGED_HEAP_HANDLE], sibling.clone_for_wire(scope)
  ensure
    scope&.close
    owner&.close
    sibling&.close
    resume << true if resume
    closer&.kill
    closer&.join(5)
  end

  def test_cancel_defers_kill_until_native_cancellation
    entered, resume = Queue.new, Queue.new
    @api.define_singleton_method(:cancel_function_call) do |id|
      entered << true
      resume.pop
      super(id)
    end
    pending = Baml::Call.new(@api, 123)
    worker = Thread.new { pending.cancel }
    Timeout.timeout(5) { entered.pop }
    worker.kill
    resume << true
    assert worker.join(5), "cancellation did not finish"
    assert_equal [123], @api.cancelled_calls
    refute pending.cancel
  ensure
    resume << true if resume
    worker&.kill
    worker&.join(5)
  end

  def test_cancel_defers_an_interrupt_until_native_cancellation
    @api.define_singleton_method(:cancel_function_call) do |id|
      Thread.current.raise Interrupt
      super(id)
    end
    pending = Baml::Call.new(@api, 123)
    assert_raises(Interrupt) { pending.cancel }
    assert_equal [123], @api.cancelled_calls
    refute pending.cancel
  end

  def test_unregistered_error_classes_preserve_message_trace_fields_and_handle_ownership
    payload = { class_value: { name: "ai.errors.Unregistered", fields: [
      { key: "message", value: { string_value: "render failed" } },
      { key: "nested", value: { class_value: { name: "ai.errors.Nested", fields: [
        { key: "detail", value: { string_value: "nested detail" } },
        { key: "handle", value: { handle_value: { key: 41, handle_type: :ADT_TAGGED_HEAP_HANDLE } } }
      ] } } }
    ] } }
    trace = ['File "test.baml", line 3, in render']
    { error: Baml::Error, panic: Baml::Panic }.each do |kind, klass|
      @api.dispatch = ->(_args, id) { @api.deliver(id, **{ kind => { value: payload, trace: trace } }) }
      failure = assert_raises(klass) { call }
      assert_equal "ai.errors.Unregistered", failure.type_name
      assert_equal "render failed", failure.message
      assert_equal trace, failure.baml_trace
      assert failure.value.frozen?
      assert failure.value.fetch("nested").frozen?
      assert_equal "nested detail", failure.value.fetch("nested").fetch("detail")
      owner = failure.value.fetch("nested").fetch("handle")
      assert_instance_of Baml::Handle, owner
      assert @api.released_handles.empty?
      owner.close
      assert_equal 41, released_handle
      assert @api.released_handles.empty?
    end
    @api.dispatch = ->(_args, id) { @api.deliver(id, ok: payload) }
    assert_raises(Baml::Bridge::UnsupportedTypeError) { call }
    assert_equal 41, released_handle
    assert @api.released_handles.empty?
  end

  def test_registered_error_with_an_unregistered_child_falls_back_without_double_adopting
    protocol = Baml::Bridge::Protocol
    type = Struct.new(:handle, :nested, keyword_init: true)
    protocol.define_class("user.RegisteredErrorParent", type, fields: {
      "handle" => [:handle, [:unknown]], "nested" => [:nested, [:unknown]]
    })
    @api.dispatch = ->(_args, id) { @api.deliver(id, error: { value: { class_value: {
      name: "user.RegisteredErrorParent", fields: [
        { key: "handle", value: { handle_value: { key: 42, handle_type: :ADT_TAGGED_HEAP_HANDLE } } },
        { key: "nested", value: { class_value: { name: "user.UnregisteredChild", fields: [] } } }
      ]
    } } }) }
    failure = assert_raises(Baml::Error) { call }
    assert_instance_of Hash, failure.value
    assert failure.value.frozen?
    assert_equal({}, failure.value.fetch("nested"))
    assert failure.value.fetch("nested").frozen?
    assert @api.released_handles.empty?
    failure.value.fetch("handle").close
    assert_equal 42, released_handle
    assert @api.released_handles.empty?
  end

  def test_unregistered_error_message_uses_detail_then_type_name
    [ [{ key: "detail", value: { string_value: "detail only" } }, "detail only"],
      [{ key: "message", value: { int_value: 42 } }, "ai.errors.Unregistered"] ].each do |field, message|
      @api.dispatch = ->(_args, id) { @api.deliver(id, error: { value: { class_value: {
        name: "ai.errors.Unregistered", fields: [field]
      } } }) }
      assert_equal message, assert_raises(Baml::Error) { call }.message
    end
  end

  def test_stream_each_propagates_stop_iteration_from_the_consumer
    stream = Baml::Stream.new(@api, 99, :ADT_TAGGED_HEAP_HANDLE)
    done = Class.new
    values = [nil, "partial", done.new]
    stream.define_singleton_method(:next) { values.shift }
    Baml::Bridge::Protocol.stub(:registered_type, done) do
      failure = StopIteration.new("consumer failed")
      assert_same failure, assert_raises(StopIteration) { stream.each { raise failure } }
      assert_equal 1, values.length
      assert_same stream, stream.each { flunk "done must not be yielded" }
      assert_instance_of Enumerator, stream.each
    end
  ensure
    stream&.close
  end

  def test_callback_ids_wrap_and_skip_pending_ids
    first = Object.new
    last = Object.new
    wrapped = Object.new
    assert_equal 1, @runtime.send(:register_call, first)
    @runtime.instance_variable_set(:@next_callback_id, 0xffff_ffff)
    assert_equal 0xffff_ffff, @runtime.send(:register_call, last)
    assert_equal 2, @runtime.send(:register_call, wrapped)
    pending = @runtime.instance_variable_get(:@pending)
    assert_same first, pending.fetch(1)
    assert_same last, pending.fetch(0xffff_ffff)
    assert_same wrapped, pending.fetch(2)
  end

  def test_result_handles_are_owned_before_wait_and_after_partial_decode
    handle = { handle_value: { key: 41, handle_type: :ADT_TAGGED_HEAP_HANDLE } }
    @api.dispatch = ->(_args, id) { @api.deliver(id, ok: { list_value: { items: [handle, handle] } }) }
    Thread.new { @runtime.call_async("user.test", {}); nil }.join
    Timeout.timeout(5) { GC.start until @api.released_handles.size == 2 }
    assert_equal [41, 41], 2.times.map { released_handle }

    @api.dispatch = lambda do |_args, id|
      @api.deliver(id, ok: { list_value: { items: [handle, { prompt_ast_value: {} }, handle] } })
    end
    assert_raises(Baml::Bridge::UnsupportedTypeError) { call }
    assert_equal 41, released_handle # The occurrence decoding never reached.
    Timeout.timeout(5) { GC.start until @api.released_handles.size == 1 }
    assert_equal 41, released_handle

    @api.dispatch = ->(_args, id) { @api.deliver(id, ok: { list_value: { items: [handle, handle] } }) }
    owners = call
    refute_same owners.first, owners.last
    GC.start
    assert @api.released_handles.empty?
    owners.each(&:close)
    assert_equal [41, 41], 2.times.map { released_handle }
  end

  def test_reflected_types_release_every_live_head_on_refusal_and_abandonment
    reflected = { ty_value: { function: {
      params: [{ ty: { map: {
        key: { enum: { live: 41 } },
        value: { class_ty: { live: 42, type_args: [{ type_alias: { live: 41 } }] } }
      } } }],
      ret: { interface: { live: 43, bindings: [{ name: "Item", ty: { enum_variant: { live: 44, variant: "V" } } }] } },
      throws: { optional: { inner: { union: { options: [{ enum: { live: 45 } }, { class_ty: { anonymous_label: "display" } }] } } } }
    } } }
    @api.dispatch = ->(_args, id) { @api.deliver(id, ok: reflected) }
    assert_raises(Baml::Bridge::UnsupportedTypeError) { call }
    assert_equal [41, 41, 42, 43, 44, 45], 6.times.map { released_handle }.sort

    Thread.new { @runtime.call_async("user.reflected", {}); nil }.join
    Timeout.timeout(5) { GC.start until @api.released_handles.size == 6 }
    assert_equal [41, 41, 42, 43, 44, 45], 6.times.map { released_handle }.sort
    GC.start
    assert @api.released_handles.empty?
  end

  def test_reflected_type_references_are_released_after_partial_decode
    @api.dispatch = lambda do |_args, id|
      @api.deliver(id, ok: { list_value: { items: [
        { handle_value: { key: 40, handle_type: :ADT_TAGGED_HEAP_HANDLE } },
        { ty_value: { list: { item: { class_ty: { live: 41 } } } } },
        { handle_value: { key: 42, handle_type: :ADT_TAGGED_HEAP_HANDLE } }
      ] } })
    end
    assert_raises(Baml::Bridge::UnsupportedTypeError) { call }
    assert_equal [41, 42], 2.times.map { released_handle }.sort
    Timeout.timeout(5) { GC.start until @api.released_handles.size == 1 }
    assert_equal 40, released_handle
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

  def test_callback_argument_decode_releases_unadopted_live_types_and_later_handles
    protocol = Baml::Bridge::Protocol
    scope = Baml::Bridge::EncodingScope.new
    key = Baml::Bridge::HostValues.register(->(*) { flunk "decoding must fail before invoking Ruby" }, scope,
      [:function, [], [:int], [:never]])
    scope.submitted!
    retained = []
    decode = protocol.method(:decode_value)
    protocol.stub(:decode_value, ->(*args, **kwargs) { value = decode.call(*args, **kwargs); retained << value if value.is_a?(Baml::Handle); value }) do
      frame = Wire::HostInvocation.new(host_value_key: key, callback_id: 123, effective_state: 77,
        application_args: Wire::BamlToHostCall.encode(Wire::BamlToHostCall.new(args: [
          { value: { handle_value: { key: 40, handle_type: :ADT_TAGGED_HEAP_HANDLE } } },
          { value: { ty_value: { class_ty: { live: 41 } } } },
          { value: { handle_value: { key: 42, handle_type: :ADT_TAGGED_HEAP_HANDLE } } }
        ])))
      Baml::Bridge::HostCallbacks.new(@api).send(:invoke, frame)
    end
    id, completion = @api.host_completions.pop
    assert_equal 123, id
    assert_equal :bridge_failure, completion.outcome
    assert_includes completion.bridge_failure, "ty_value is not yet supported"
    assert_equal [41, 42, 77], 3.times.map { released_handle }.sort
    GC.start
    assert @api.released_handles.empty?, "adopted handle must stay owned by its Ruby wrapper"
    assert_equal 1, retained.length
    retained.first.close
    assert_equal 40, released_handle
    assert @api.released_handles.empty?
  ensure
    retained&.each(&:close)
    Baml::Bridge::HostValues.handle_release(key) if key
    scope&.close
  end

  class CallbackField < T::Struct
    const :callback, T.any(T.proc.params(value: Integer).returns(Integer), Baml::Closure)
  end

  def test_non_proc_host_callable_in_a_struct_field_decodes_as_a_proc
    callable = Object.new
    callable.define_singleton_method(:call) { |value| value + 1 }
    scope = Baml::Bridge::EncodingScope.new
    key = Baml::Bridge::HostValues.register(callable, scope, [:function, [[nil, [:int], false]], [:int], [:never]])
    scope.submitted!
    Baml::Bridge.define_class("user.CallbackField", CallbackField,
      fields: { "callback" => [:callback, [:function, [[nil, [:int], false]], [:int], [:never]]] })
    handle = { handle_value: { key: key, handle_type: :HOST_VALUE_CALLABLE } }
    @api.dispatch = ->(_args, id) { @api.deliver(id, ok: { class_value: {
      name: "user.CallbackField", fields: [{ key: "callback", value: handle }]
    } }) }
    result = call
    Baml::Bridge::HostValues.handle_release(key)
    key = nil
    assert_instance_of CallbackField, result
    assert_instance_of Proc, result.callback
    assert_equal 42, result.callback.call(41)
    assert_same callable, result.callback.binding.receiver
  ensure
    Baml::Bridge::HostValues.handle_release(key) if key
    scope&.close
  end

  def test_killed_host_worker_completes_and_releases_once
    entered = Queue.new
    scope = Baml::Bridge::EncodingScope.new
    key = Baml::Bridge::HostValues.register(-> { entered << Thread.current; sleep }, scope,
      [:function, [], [:null], [:never]])
    scope.submitted!
    frame = Wire::HostInvocation.encode(Wire::HostInvocation.new(
      host_value_key: key, callback_id: 123, effective_state: 77,
      application_args: Wire::BamlToHostCall.encode(Wire::BamlToHostCall.new)
    ))
    pointer = FFI::MemoryPointer.from_string(frame)
    worker = nil
    [false, true].each do |masked_dispatch|
      if masked_dispatch
        Thread.handle_interrupt(Object => :never) { @api.host_dispatch.call(pointer, frame.bytesize) }
      else
        @api.host_dispatch.call(pointer, frame.bytesize)
      end
      worker = entered.pop(timeout: 5)
      assert worker, "host callable did not run"
      worker.report_on_exception = false
      worker.kill
      assert worker.join(5), "host worker did not stop"
      assert_equal 1, @api.host_completions.size
      id, completion = @api.host_completions.pop(timeout: 5)
      assert_equal [123, :bridge_failure], [id, completion.outcome]
      refute_empty completion.bridge_failure
      assert_equal 77, @api.released_handles.pop(timeout: 5)
      assert @api.released_handles.empty?, "worker released the effective state twice"
    end
  ensure
    worker&.kill
    worker&.join(5)
    Baml::Bridge::HostValues.handle_release(key) if key
    scope&.close
  end

  def test_host_worker_killed_during_result_clone_rolls_back_and_completes_once
    entered, resume = Queue.new, Queue.new
    @api.define_singleton_method(:handle_clone) do |key|
      entered << Thread.current
      resume.pop(timeout: 5) or raise "result clone did not resume"
      super(key)
    end
    handle = Baml::Handle.new(@api, 42, :ADT_TAGGED_HEAP_HANDLE)
    scope = Baml::Bridge::EncodingScope.new
    key = Baml::Bridge::HostValues.register(-> { handle }, scope, [:function, [], [:unknown], [:never]])
    scope.submitted!
    frame = Wire::HostInvocation.encode(Wire::HostInvocation.new(
      host_value_key: key, callback_id: 123, effective_state: 77,
      application_args: Wire::BamlToHostCall.encode(Wire::BamlToHostCall.new)
    ))
    pointer = FFI::MemoryPointer.from_string(frame)
    @api.host_dispatch.call(pointer, frame.bytesize)
    worker = entered.pop(timeout: 5)
    assert worker, "result handle was not cloned"
    worker.kill
    resume << true
    assert worker.join(5), "host worker did not stop"
    assert_equal 1, @api.host_completions.size
    id, completion = @api.host_completions.pop(timeout: 5)
    assert_equal [123, :bridge_failure], [id, completion.outcome]
    refute_empty completion.bridge_failure
    assert_equal [42, 77], 2.times.map { @api.released_handles.pop(timeout: 5) }.compact.sort
    assert @api.released_handles.empty?, "worker released the clone or effective state twice"
    handle.close
    assert_equal 42, @api.released_handles.pop(timeout: 5)
    assert @api.released_handles.empty?
  ensure
    resume << true if resume
    worker&.kill
    worker&.join(5)
    handle&.close
    Baml::Bridge::HostValues.handle_release(key) if key
    scope&.close
  end

  def test_host_raise_after_result_commit_releases_discarded_clone_and_completes_once
    check_host_raise_after_result_commit(release_fails: false)
  end

  def test_release_failure_after_host_raise_still_completes_and_releases_state
    check_host_raise_after_result_commit(release_fails: true)
  end

  def test_host_raise_after_error_commit_releases_discarded_clone_and_completes_once
    check_host_raise_after_result_commit(release_fails: false, callable_error: Baml::Error)
  end

  def test_host_raise_after_panic_commit_releases_discarded_clone_and_completes_once
    check_host_raise_after_result_commit(release_fails: false, callable_error: Baml::Panic)
  end

  def check_host_raise_after_result_commit(release_fails:, callable_error: nil)
    entered = Queue.new
    interrupted_outcome = callable_error ? :thrown : :returned
    callbacks = Baml::Bridge::HostCallbacks.new(@api)
    encode = callbacks.method(:encode_completion)
    callbacks.define_singleton_method(:encode_completion) do |value, slot, outcome, scope|
      completion = encode.call(value, slot, outcome, scope)
      if outcome == interrupted_outcome
        entered << Thread.current
        sleep
      end
      completion
    end
    if release_fails
      @api.define_singleton_method(:handle_release) do |key|
        super(key)
        raise "release failed" if key == 42 && Thread.current != Thread.main
      end
    end
    handle = Baml::Handle.new(@api, 42, :ADT_TAGGED_HEAP_HANDLE)
    scope = Baml::Bridge::EncodingScope.new
    callable = callable_error ? -> { raise callable_error.new(handle) } : -> { handle }
    throws = callable_error ? [:unknown] : [:never]
    key = Baml::Bridge::HostValues.register(callable, scope, [:function, [], [:unknown], throws])
    scope.submitted!
    frame = Wire::HostInvocation.encode(Wire::HostInvocation.new(
      host_value_key: key, callback_id: 123, effective_state: 77,
      application_args: Wire::BamlToHostCall.encode(Wire::BamlToHostCall.new)
    ))
    callbacks.instance_variable_get(:@dispatch).call(FFI::MemoryPointer.from_string(frame), frame.bytesize)
    worker = entered.pop(timeout: 5)
    assert worker, "result encoding did not commit"
    worker.raise RuntimeError, "host raise after commit"
    assert worker.join(5), "host worker did not stop"
    assert_equal 1, @api.host_completions.size
    id, completion = @api.host_completions.pop(timeout: 5)
    assert_equal 123, id
    if release_fails
      assert_equal :bridge_failure, completion.outcome
      assert_equal "release failed", completion.bridge_failure
    else
      assert_equal :raised, completion.outcome
      assert_equal "host raise after commit", completion.raised.message
      assert_equal "RuntimeError", completion.raised.class_name
      raised_key = completion.raised.handle
      assert_instance_of RuntimeError, Baml::Bridge::HostValues.fetch(raised_key)
    end
    assert_equal [42, 77], Array.new(@api.released_handles.size) { @api.released_handles.pop }.sort
    assert @api.released_handles.empty?, "worker released a reference twice"
    handle.close
    assert_equal 42, @api.released_handles.pop(timeout: 5)
    assert @api.released_handles.empty?
  ensure
    worker&.kill
    worker&.join(5)
    handle&.close
    Baml::Bridge::HostValues.handle_release(raised_key) if raised_key
    Baml::Bridge::HostValues.handle_release(key) if key
    scope&.close
  end

  def test_host_worker_killed_during_commit_failure_close_releases_every_clone
    entered, resume = Queue.new, Queue.new
    @api.define_singleton_method(:handle_clone) { |key| key == 43 ? raise("clone failed") : key }
    @api.define_singleton_method(:handle_release) do |key|
      super(key)
      if key == 41 && Thread.current != Thread.main
        entered << Thread.current
        resume.pop(timeout: 5) or raise "scope close did not resume"
      end
    end
    handles = [41, 42, 43].map { |key| Baml::Handle.new(@api, key, :ADT_TAGGED_HEAP_HANDLE) }
    scope = Baml::Bridge::EncodingScope.new
    key = Baml::Bridge::HostValues.register(-> { handles }, scope, [:function, [], [:unknown], [:never]])
    scope.submitted!
    frame = Wire::HostInvocation.encode(Wire::HostInvocation.new(
      host_value_key: key, callback_id: 123, effective_state: 77,
      application_args: Wire::BamlToHostCall.encode(Wire::BamlToHostCall.new)
    ))
    @api.host_dispatch.call(FFI::MemoryPointer.from_string(frame), frame.bytesize)
    worker = entered.pop(timeout: 5)
    assert worker, "commit failure did not close its scope"
    worker.kill
    resume << true
    assert worker.join(5), "host worker did not stop"
    assert_equal 1, @api.host_completions.size
    id, completion = @api.host_completions.pop(timeout: 5)
    assert_equal [123, :bridge_failure], [id, completion.outcome]
    refute_empty completion.bridge_failure
    assert_equal [41, 42, 77], Array.new(@api.released_handles.size) { @api.released_handles.pop }.sort
    scope.close
    assert @api.released_handles.empty?, "worker released a reference twice"
  ensure
    resume << true if resume
    worker&.kill
    worker&.join(5)
    handles&.each(&:close)
    Baml::Bridge::HostValues.handle_release(key) if key
    scope&.close
  end

  def test_host_worker_release_failure_after_kill_still_completes_and_releases_state
    entered, resume = Queue.new, Queue.new
    failure = RuntimeError.new("release failed")
    @api.define_singleton_method(:handle_clone) do |key|
      entered << Thread.current
      resume.pop(timeout: 5) or raise "result clone did not resume"
      key
    end
    @api.define_singleton_method(:handle_release) do |key|
      super(key)
      raise failure if key == 42 && Thread.current != Thread.main
    end
    handle = Baml::Handle.new(@api, 42, :ADT_TAGGED_HEAP_HANDLE)
    scope = Baml::Bridge::EncodingScope.new
    key = Baml::Bridge::HostValues.register(-> { handle }, scope, [:function, [], [:unknown], [:never]])
    scope.submitted!
    frame = Wire::HostInvocation.encode(Wire::HostInvocation.new(
      host_value_key: key, callback_id: 123, effective_state: 77,
      application_args: Wire::BamlToHostCall.encode(Wire::BamlToHostCall.new)
    ))
    @api.host_dispatch.call(FFI::MemoryPointer.from_string(frame), frame.bytesize)
    worker = entered.pop(timeout: 5)
    assert worker, "result handle was not cloned"
    worker.report_on_exception = false
    worker.kill
    resume << true
    assert_same failure, assert_raises(RuntimeError) { worker.join(5) }
    assert_equal 1, @api.host_completions.size
    id, completion = @api.host_completions.pop(timeout: 5)
    assert_equal [123, :bridge_failure], [id, completion.outcome]
    refute_empty completion.bridge_failure
    assert_equal [42, 77], Array.new(@api.released_handles.size) { @api.released_handles.pop }.sort
    assert @api.released_handles.empty?, "worker released a reference twice"
    handle.close
    assert_equal 42, @api.released_handles.pop(timeout: 5)
    assert @api.released_handles.empty?
  ensure
    resume << true if resume
    worker&.kill
    begin
      worker&.join(5)
    rescue RuntimeError
      # The worker reports the release error after completing its callback.
    end
    handle&.close
    Baml::Bridge::HostValues.handle_release(key) if key
    scope&.close
  end

  def test_host_worker_bookkeeping_defers_kill_until_completion
    entered, resume = Queue.new, Queue.new
    arguments = Object.new
    arguments.define_singleton_method(:close) { entered << Thread.current; resume.pop(timeout: 5) or raise "cleanup did not resume" }
    callbacks = Baml::Bridge::HostCallbacks.new(@api)
    scope = Baml::Bridge::EncodingScope.new
    key = Baml::Bridge::HostValues.register(-> { nil }, scope, [:function, [], [:null], [:never]])
    scope.submitted!
    frame = Wire::HostInvocation.encode(Wire::HostInvocation.new(
      host_value_key: key, callback_id: 123, effective_state: 77,
      application_args: Wire::BamlToHostCall.encode(Wire::BamlToHostCall.new)
    ))
    pointer = FFI::MemoryPointer.from_string(frame)
    worker = nil
    callbacks.stub(:argument_result, arguments) do
      callbacks.instance_variable_get(:@dispatch).call(pointer, frame.bytesize)
      worker = entered.pop(timeout: 5)
      assert worker, "worker did not start cleanup"
      worker.kill
      resume << true
      assert worker.join(5), "host worker did not stop"
    end
    assert_equal 1, @api.host_completions.size
    id, completion = @api.host_completions.pop(timeout: 5)
    assert_equal [123, :returned], [id, completion.outcome]
    assert_equal 77, @api.released_handles.pop(timeout: 5)
    assert @api.released_handles.empty?, "worker released the effective state twice"
  ensure
    resume << true if resume
    worker&.kill
    worker&.join(5)
    Baml::Bridge::HostValues.handle_release(key) if key
    scope&.close
  end

  def test_host_worker_killed_before_its_body_completes_and_releases_once
    before_body, resume = Queue.new, Queue.new
    scope = Baml::Bridge::EncodingScope.new
    key = Baml::Bridge::HostValues.register(-> { flunk "killed worker must not invoke Ruby" }, scope,
      [:function, [], [:null], [:never]])
    scope.submitted!
    frame = Wire::HostInvocation.encode(Wire::HostInvocation.new(
      host_value_key: key, callback_id: 123, effective_state: 77,
      application_args: Wire::BamlToHostCall.encode(Wire::BamlToHostCall.new)
    ))
    pointer = FFI::MemoryPointer.from_string(frame)
    start_thread = Thread.method(:new)
    worker = nil
    starting = Mutex.new
    trace = TracePoint.new(:thread_begin) do
      # FFI's callback dispatcher may also start here. Only pause the worker
      # returned by dispatch's Thread.new, once its identity has been published.
      next unless starting.synchronize { Thread.current.equal?(worker) }

      before_body << true
      resume.pop(timeout: 5) or raise "worker start did not resume"
    end
    starting.synchronize do
      trace.enable
      Thread.stub(:new, ->(&block) { worker = start_thread.call(&block) }) do
        @api.host_dispatch.call(pointer, frame.bytesize)
      end
    end
    assert before_body.pop(timeout: 5), "host worker did not start"
    trace.disable
    worker.kill
    worker.join(0.1)
    resume << true
    assert worker.join(5), "host worker did not stop"
    assert_equal 1, @api.host_completions.size
    id, completion = @api.host_completions.pop(timeout: 5)
    assert_equal [123, :bridge_failure], [id, completion.outcome]
    assert_equal 77, @api.released_handles.pop(timeout: 5)
    assert @api.released_handles.empty?, "worker released the effective state twice"
  ensure
    trace&.disable
    resume << true if resume
    worker&.kill
    worker&.join(5)
    Baml::Bridge::HostValues.handle_release(key) if key
    scope&.close
  end

  def test_unmasked_dispatch_interrupt_after_thread_creation_does_not_take_worker_ownership
    entered, resume, after_new, handoff = Queue.new, Queue.new, Queue.new, Queue.new
    scope = Baml::Bridge::EncodingScope.new
    key = Baml::Bridge::HostValues.register(-> { entered << true; resume.pop(timeout: 5); nil }, scope,
      [:function, [], [:null], [:never]])
    scope.submitted!
    frame = Wire::HostInvocation.encode(Wire::HostInvocation.new(
      host_value_key: key, callback_id: 123, effective_state: 77,
      application_args: Wire::BamlToHostCall.encode(Wire::BamlToHostCall.new)
    ))
    pointer = FFI::MemoryPointer.from_string(frame)
    start_thread = Thread.method(:new)
    create_worker = lambda do |&block|
      worker = start_thread.call(&block)
      after_new << worker
      handoff.pop(timeout: 5) or raise "worker handoff did not resume"
      worker
    end
    worker = dispatcher = nil
    Thread.stub(:new, create_worker) do
      dispatcher = start_thread.call do
        @api.host_dispatch.call(pointer, frame.bytesize)
      rescue Interrupt
        # The dispatcher owns the interrupt, but no longer owns the frame.
      end
      worker = after_new.pop(timeout: 5)
      assert worker, "dispatch did not create a worker"
      assert entered.pop(timeout: 5), "host callable did not run"
      dispatcher.raise Interrupt, "interrupt after worker creation"
      handoff << true
      assert dispatcher.join(5), "dispatch did not return"
      assert @api.host_completions.empty?, "dispatcher completed the worker's frame"
      assert @api.released_handles.empty?, "dispatcher released the worker's frame"
      resume << true
      assert worker.join(5), "host worker did not finish"
    end
    assert_equal 1, @api.host_completions.size
    id, completion = @api.host_completions.pop(timeout: 5)
    assert_equal [123, :returned], [id, completion.outcome]
    assert_equal 77, @api.released_handles.pop(timeout: 5)
    assert @api.released_handles.empty?, "frame was released twice"
  ensure
    handoff << true if handoff
    resume << true if resume
    dispatcher&.kill
    dispatcher&.join(5)
    worker&.kill
    worker&.join(5)
    Baml::Bridge::HostValues.handle_release(key) if key
    scope&.close
  end

  def test_host_exception_message_is_read_once_and_never_under_the_bookkeeping_mask
    entered, resume = Queue.new, Queue.new
    reads = class_reads = 0
    error = StandardError.new
    error.define_singleton_method(:class) { class_reads += 1; StandardError }
    error.define_singleton_method(:message) do
      reads += 1
      entered << Thread.current if reads == 1
      raise self if reads == 1

      resume.pop(timeout: 5) or raise "message did not resume"
      "failure"
    end
    scope = Baml::Bridge::EncodingScope.new
    key = Baml::Bridge::HostValues.register(-> { raise error }, scope, [:function, [], [:null], [:never]])
    scope.submitted!
    frame = Wire::HostInvocation.encode(Wire::HostInvocation.new(
      host_value_key: key, callback_id: 123, effective_state: 77,
      application_args: Wire::BamlToHostCall.encode(Wire::BamlToHostCall.new)
    ))
    pointer = FFI::MemoryPointer.from_string(frame)
    @api.host_dispatch.call(pointer, frame.bytesize)
    worker = entered.pop(timeout: 5)
    assert worker, "exception message was not read"
    worker.join(0.2)
    worker.kill
    assert worker.join(0.2), "exception message ran under the bookkeeping mask"
    assert_equal 1, reads
    assert_equal 1, class_reads
    assert_equal 1, @api.host_completions.size
    id, completion = @api.host_completions.pop(timeout: 5)
    assert_equal [123, :raised], [id, completion.outcome]
    assert_equal "StandardError", completion.raised.message
    assert_equal "StandardError", completion.raised.class_name
    Baml::Bridge::HostValues.handle_release(completion.raised.handle)
    assert_equal 77, @api.released_handles.pop(timeout: 5)
    assert @api.released_handles.empty?, "frame was released twice"
  ensure
    resume << true if resume
    worker&.kill
    worker&.join(5)
    Baml::Bridge::HostValues.handle_release(key) if key
    scope&.close
  end

  def test_pending_dispatch_interrupt_completes_and_releases_once
    scope = Baml::Bridge::EncodingScope.new
    key = Baml::Bridge::HostValues.register(-> { nil }, scope, [:function, [], [:null], [:never]])
    scope.submitted!
    frame = Wire::HostInvocation.encode(Wire::HostInvocation.new(
      host_value_key: key, callback_id: 123, effective_state: 77,
      application_args: Wire::BamlToHostCall.encode(Wire::BamlToHostCall.new)
    ))
    pointer = FFI::MemoryPointer.from_string(frame)
    start_thread = Thread.method(:new)
    worker = dispatcher = nil
    [:before_dispatch, :worker_started].each do |interrupt_at|
      start_with_interrupt = lambda do |&block|
        # Leave an interrupt pending just as Thread.new returns. Switching back
        # to :immediate here must not make dispatch finish the worker's frame.
        Thread.handle_interrupt(Object => :never) do
          worker = start_thread.call(&block)
          Thread.current.raise Interrupt if interrupt_at == :worker_started
          worker
        end
      end
      Thread.stub(:new, start_with_interrupt) do
        dispatcher = start_thread.call do
          Thread.handle_interrupt(Object => :never) do
            Thread.current.raise Interrupt if interrupt_at == :before_dispatch
            @api.host_dispatch.call(pointer, frame.bytesize)
          end
        rescue Interrupt
          # The dispatching thread still owns its deferred interrupt.
        end
        assert dispatcher.join(5), "dispatch did not return"
        assert worker, "dispatch did not start a worker"
        assert worker.join(5), "host worker did not finish"
      end
      assert_equal 1, @api.host_completions.size
      id, completion = @api.host_completions.pop(timeout: 5)
      assert_equal [123, :returned], [id, completion.outcome]
      assert_equal 77, @api.released_handles.pop(timeout: 5)
      assert @api.released_handles.empty?, "dispatch released the effective state twice"
    end
  ensure
    dispatcher&.kill
    dispatcher&.join(5)
    worker&.kill
    worker&.join(5)
    Baml::Bridge::HostValues.handle_release(key) if key
    scope&.close
  end

  def test_host_callback_threads_are_killable_at_exit_after_masked_dispatch
    source = <<~'RUBY'
      require "baml/bridge"
      wire = BamlBridge::Cffi::V1
      children, completed = Queue.new, Queue.new
      api = Object.new
      api.define_singleton_method(:register_host_callbacks) { |dispatch, *_| @dispatch = dispatch }
      api.define_singleton_method(:dispatch) { @dispatch }
      api.define_singleton_method(:complete_host_call) { |*| completed << true }
      scope = Baml::Bridge::EncodingScope.new
      callable = -> { children << Thread.new { sleep }; nil }
      key = Baml::Bridge::HostValues.register(callable, scope, [:function, [], [:null], [:never]])
      scope.submitted!
      Baml::Bridge::HostCallbacks.new(api).register!
      frame = wire::HostInvocation.encode(wire::HostInvocation.new(
        host_value_key: key, callback_id: 1,
        application_args: wire::BamlToHostCall.encode(wire::BamlToHostCall.new)
      ))
      pointer = FFI::MemoryPointer.from_string(frame)
      Thread.handle_interrupt(Object => :never) { api.dispatch.call(pointer, frame.bytesize) }
      child = children.pop(timeout: 5) or abort "host callback did not start a child"
      completed.pop(timeout: 5) or abort "host callback did not complete"
      at_exit do
        child.kill
        abort "host callback passed its interrupt mask to a child" unless child.join(1)
        puts "killed"
      end
    RUBY
    stdout = assert_subprocess_exits("-I", BRIDGE_LIB, "-e", source)
    assert_equal "killed\n", stdout
  end

  def test_callback_bytes_dispatch_exits_with_a_cold_timeout_worker
    stdout = assert_subprocess_exits(__FILE__, "--name", "test_callback_returns_and_closure_arguments_preserve_bytes")
    assert_includes stdout, "0 failures, 0 errors"
  end

  def test_callback_returns_and_closure_arguments_preserve_bytes
    Baml::Bridge.define_function("user.bytes_callback", params: [["cb", [:function, [], [:bytes], [:never]]]])
    callback_key = callback_id = nil
    @api.dispatch = lambda do |args, id|
      callback_key, callback_id = args.kwargs.first.value.handle.key, id
      key = callback_key
      frame = Wire::HostInvocation.encode(Wire::HostInvocation.new(
        host_value_key: key, callback_id: 123,
        application_args: Wire::BamlToHostCall.encode(Wire::BamlToHostCall.new)
      ))
      pointer = FFI::MemoryPointer.from_string(frame)
      @api.host_dispatch.call(pointer, frame.bytesize)
    end
    pending = @runtime.call_async("user.bytes_callback", { "cb" => -> { "\0bytes".b } })
    _, completion = Timeout.timeout(5) { @api.host_completions.pop }
    assert_equal :returned, completion.outcome
    value = completion.returned
    assert_equal :uint8array_value, value.value
    @api.host_release.call(callback_key)
    @api.deliver(callback_id, ok: { uint8array_value: value.uint8array_value })
    assert_equal "\0bytes".b, pending.wait

    @api.dispatch = lambda do |_args, id|
      @api.deliver(id, ok: { handle_value: { key: 77, handle_type: :FUNCTION_REF, ty: { function: {
        params: [{ name: "data", mode: :BAML_TY_FUNCTION_PARAM_MODE_REQUIRED,
                   ty: { primitive: { kind: :BAML_TY_PRIMITIVE_BYTES } } }],
        ret: { primitive: { kind: :BAML_TY_PRIMITIVE_BYTES } }, throws: { never: {} }
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

  def test_closure_parameters_keep_names_modes_and_complete_descriptors
    @api.dispatch = lambda do |_args, id|
      @api.deliver(id, ok: { handle_value: { key: 77, handle_type: :FUNCTION_REF, ty: { function: {
        params: [{ mode: :BAML_TY_FUNCTION_PARAM_MODE_REQUIRED, ty: { primitive: { kind: :BAML_TY_PRIMITIVE_FLOAT } } },
                 { name: "count", mode: :BAML_TY_FUNCTION_PARAM_MODE_OPTIONAL, ty: { primitive: { kind: :BAML_TY_PRIMITIVE_INT } } }],
        ret: { list: { item: { primitive: { kind: :BAML_TY_PRIMITIVE_FLOAT } } } }, throws: { never: {} }
      } } } })
    end
    closure = call
    assert_equal({ "arg0" => [:float], "count" => [:int] }, closure.parameter_descriptors)
    @api.dispatch = lambda do |args, id|
      assert_equal 77, args.function_handle
      assert_equal 5.0, args.kwargs.first.value.float_value
      @api.deliver(id, ok: { int_value: args.kwargs.length })
    end
    Baml::Bridge.stub(:call_async, ->(target, args, **options) { @runtime.call_async(target, args, **options) }) do
      assert_equal 1, closure.call(5)
      assert_equal 2, closure.call(arg0: 5, count: 2)
      assert_raises(ArgumentError) { closure.call(5, 2) }
      assert_raises(TypeError) { closure.call(5, count: "wrong") }
      assert @api.released_handles.size == 2, "refused arguments must not clone the closure target"
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
    Baml::Bridge.define_function("user.namespace.OriginalName", params: [["callback", [:function, [["value", [:int], false]], [:int], [:never]]]])
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
    Timeout.timeout(5) { Thread.pass while interrupted.alive? && interrupted.status != "sleep" }
    assert interrupted.alive?
    interrupted.raise(Interrupt)
    assert_instance_of Interrupt, interrupted.value
    refute result.done?

    waiters = 3.times.map { Thread.new { result.wait(timeout: 5) } }
    Timeout.timeout(5) do
      Thread.pass while waiters.all?(&:alive?) && waiters.any? { |thread| thread.status != "sleep" }
    end
    assert waiters.all?(&:alive?)
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

  def test_thread_start_failure_releases_callback_arguments_and_completes
    frame = Wire::HostInvocation.new(
      host_value_key: 999, callback_id: 124, effective_state: 77,
      application_args: Wire::BamlToHostCall.encode(Wire::BamlToHostCall.new(args: [
        { value: { handle_value: { key: 66, handle_type: :ADT_TAGGED_HEAP_HANDLE } } }
      ])),
      cancel: { handle_value: { key: 88, handle_type: :ADT_TAGGED_HEAP_HANDLE } }
    )
    bytes = Wire::HostInvocation.encode(frame)
    pointer = FFI::MemoryPointer.new(:uint8, bytes.bytesize)
    pointer.put_bytes(0, bytes)
    Thread.stub(:new, ->(*) { raise ThreadError, "thread limit" }) do
      @api.host_dispatch.call(pointer, bytes.bytesize)
    end
    id, completion = @api.host_completions.pop
    assert_equal 124, id
    assert_equal :bridge_failure, completion.outcome
    assert_equal "BAML host callback dispatch failed", completion.bridge_failure
    assert_equal [66, 77, 88], 3.times.map { released_handle }.sort
    assert @api.released_handles.empty?
    assert @api.host_completions.empty?
  end

  def test_cancelled_host_callback_keeps_resources_until_proc_returns
    registry = Baml::Bridge.const_get(:HostValues, false)
    scope = Baml::Bridge.const_get(:EncodingScope, false).new
    entered = Queue.new
    unblock = Queue.new
    exited = Queue.new
    cb = ->(value) { entered << value; unblock.pop; exited << true; "finished" }
    Baml::Bridge.define_function("user.cancel_callback", params: [["cb", [:function, [["value", [:int], false]], [:string], [:never]]]])
    request = Wire::CallFunctionArgs.decode(Baml::Bridge::Protocol.encode_call(1, "user.cancel_callback", { "cb" => cb }, scope: scope))
    key = request.kwargs.first.value.handle.key
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
      id, completion = @api.host_completions.pop
      assert_equal [123, :returned, "finished"], [id, completion.outcome, completion.returned.string_value]
      assert_equal [77, 88], 2.times.map { released_handle }.sort
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

  def assert_subprocess_exits(*args)
    Open3.popen3(RbConfig.ruby, *args) do |stdin, stdout, stderr, child|
      stdin.close
      exited = child.join(15)
      Process.kill("KILL", child.pid) unless exited
      status = child.value
      output, diagnostic = stdout.read, stderr.read
      assert exited, "subprocess hung at exit:\n#{output}\n#{diagnostic}"
      assert status.success?, "#{output}\n#{diagnostic}"
      output
    end
  end

  def call(arguments = {})
    @runtime.call_async("user.namespace.OriginalName", arguments).wait
  end

  def pending
    @runtime.instance_variable_get(:@pending)
  end
end
