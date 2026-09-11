# frozen_string_literal: true

require "ffi"
require "thread"
require "timeout"
require "tmpdir"
require_relative "../../../../sdks/ruby/bridge_ruby/lib/baml/bridge"

ENV["BAML_FAKE_RUNTIME_VERSION"] = Baml::Bridge::VERSION

FIXTURE = ENV.fetch("BAML_RUBY_TEST_FIXTURE")
MISSING_GETTER = ENV.fetch("BAML_RUBY_TEST_MISSING_GETTER")
INVALID_LIBRARY = ENV.fetch("BAML_RUBY_TEST_INVALID_LIBRARY")
THREAD_FIXTURE = ENV.fetch("BAML_RUBY_TEST_THREAD_FIXTURE")
REAL_RUNTIME = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")
REAL_BYTECODE = ENV.fetch("BAML_RUBY_TEST_REAL_BYTECODE")
VALID_PROGRAM = "valid-bytecode".b.freeze

module ScenarioAssertions
  module_function

  def assert(condition, message = "assertion failed")
    raise message unless condition
  end

  def assert_equal(expected, actual)
    raise "expected #{expected.inspect}, got #{actual.inspect}" unless expected == actual
  end

  def assert_raises(error_class)
    yield
  rescue error_class => error
    return error
  rescue Exception => error
    raise "expected #{error_class}, got #{error.class}: #{error.message}"
  end
end

include ScenarioAssertions

class FixtureInspection
  def initialize(path)
    flags = FFI::DynamicLibrary::RTLD_NOW | FFI::DynamicLibrary::RTLD_LOCAL
    @library = FFI::DynamicLibrary.open(path, flags)
  end

  def call_keyed(key)
    table_class = Baml::Bridge.const_get(:Native, false).const_get(:ApiV1, false)
    getter = FFI::Function.new(:pointer, [], @library.find_function("baml_get_api_v1"))
    table = table_class.new(getter.call)
    call = FFI::Function.new(:void, %i[uint64 pointer size_t uint32], table[:call_function_for_runtime])
    call.call(key, nil, 0, 1)
    FFI::Function.new(:uint64, [], @library.find_function("baml_test_last_call_key")).call
  end

  def count(symbol)
    FFI::Function.new(:uint32, [], @library.find_function(symbol)).call
  end
end

def fixture_inspection
  @fixture_inspection ||= FixtureInspection.new(FIXTURE)
end

def wait_for_child(child, timeout: 2)
  Timeout.timeout(timeout) { Process.wait2(child).last }
rescue Timeout::Error
  Process.kill("KILL", child)
  Process.wait(child)
  raise "forked child #{child} did not exit promptly"
end

def use_fixture
  ENV["BAML_RUNTIME_PATH"] = FIXTURE
  ENV.delete("BAML_FAKE_NATIVE_MODE")
  ENV.delete("BAML_FAKE_NULL_FIELD")
end

def initialize_fixture(program = VALID_PROGRAM)
  use_fixture
  Baml::Bridge.initialize!(program)
end

def terminal_failure(mode: nil, null_field: nil, path: FIXTURE)
  ENV["BAML_RUNTIME_PATH"] = path
  ENV["BAML_FAKE_NATIVE_MODE"] = mode
  ENV["BAML_FAKE_NULL_FIELD"] = null_field
  first = assert_raises(Baml::Bridge::IncompatibleRuntimeError) do
    Baml::Bridge.initialize!(VALID_PROGRAM)
  end

  ENV["BAML_RUNTIME_PATH"] = FIXTURE
  ENV.delete("BAML_FAKE_NATIVE_MODE")
  ENV.delete("BAML_FAKE_NULL_FIELD")
  second = assert_raises(Baml::Bridge::IncompatibleRuntimeError) do
    Baml::Bridge.initialize!(VALID_PROGRAM)
  end
  assert_equal(first.message, second.message)

  expected_free_count = case mode
                        when "version-mismatch", "invalid-version-utf8", "null-version-pointer"
                          1
                        when "registration-rejected"
                          2
                        else
                          0
                        end
  assert_equal(expected_free_count, fixture_inspection.count("baml_test_free_count")) if path == FIXTURE
  first
end

case ARGV.fetch(0)
when "configuration_retry"
  ENV.delete("BAML_RUNTIME_PATH")
  assert_raises(Baml::Bridge::RuntimeConfigurationError) do
    Baml::Bridge.initialize!(VALID_PROGRAM)
  end
  ENV["BAML_RUNTIME_PATH"] = "relative/library"
  assert_raises(Baml::Bridge::RuntimeConfigurationError) do
    Baml::Bridge.initialize!(VALID_PROGRAM)
  end
  ENV["BAML_RUNTIME_PATH"] = File.join(Dir.tmpdir, "missing-baml-runtime")
  assert_raises(Baml::Bridge::RuntimeConfigurationError) do
    Baml::Bridge.initialize!(VALID_PROGRAM)
  end
  initialize_fixture
  assert_equal(1, fixture_inspection.count("baml_test_initialize_count"))
  assert_equal(4, fixture_inspection.count("baml_test_free_count"))
when "open_retry"
  ENV["BAML_RUNTIME_PATH"] = INVALID_LIBRARY
  assert_raises(Baml::Bridge::RuntimeLoadError) do
    Baml::Bridge.initialize!(VALID_PROGRAM)
  end

  if Process.respond_to?(:fork)
    read_pipe, write_pipe = IO.pipe
    child = fork do
      read_pipe.close
      begin
        initialize_fixture
        write_pipe.write("ok")
        exit! 0
      rescue Exception => error
        write_pipe.write("#{error.class}: #{error.message}")
        exit! 1
      end
    end
    write_pipe.close
    status = wait_for_child(child)
    result = read_pipe.read
    assert(status.success?, result)
    assert_equal("ok", result)
  end

  initialize_fixture
  assert_equal(1, fixture_inspection.count("baml_test_initialize_count"))
  assert_equal(4, fixture_inspection.count("baml_test_free_count"))
when "concurrent_open_failure_preserves_claim"
  runtime_class = Baml::Bridge.const_get(:ProcessRuntime, false)
  runtime = runtime_class.new
  first_claimed = Queue.new
  second_claimed = Queue.new
  first_open_failed = Queue.new

  runtime.define_singleton_method(:configured_runtime_path!) do
    Thread.current[:baml_test_runtime_path]
  end
  runtime.define_singleton_method(:claim_process!) do
    super()
    claim_count = (Thread.current[:baml_test_claim_count] || 0) + 1
    Thread.current[:baml_test_claim_count] = claim_count
    if claim_count == 1
      if Thread.current[:baml_test_runtime_path] == INVALID_LIBRARY
        first_claimed << true
        second_claimed.pop
      else
        second_claimed << true
        first_open_failed.pop
      end
    end
  end

  first_error = Queue.new
  first = Thread.new do
    Thread.current[:baml_test_runtime_path] = INVALID_LIBRARY
    runtime.initialize!(VALID_PROGRAM)
  rescue Exception => error
    first_error << error
    first_open_failed << true
  end
  first_claimed.pop

  second_error = Queue.new
  second = Thread.new do
    Thread.current[:baml_test_runtime_path] = FIXTURE
    runtime.initialize!(VALID_PROGRAM)
  rescue Exception => error
    second_error << error
  end

  [first, second].each(&:join)
  error = first_error.pop
  assert(error.is_a?(Baml::Bridge::RuntimeLoadError), error.inspect)
  assert(second_error.empty?, "surviving initializer failed")
  assert_equal(Process.pid, runtime.instance_variable_get(:@owner_pid))

  if Process.respond_to?(:fork)
    read_pipe, write_pipe = IO.pipe
    child = fork do
      read_pipe.close
      error = assert_raises(Baml::Bridge::ForkSafetyError) do
        runtime.initialize!(VALID_PROGRAM)
      end
      write_pipe.write(error.message)
      exit! 0
    end
    write_pipe.close
    status = wait_for_child(child)
    message = read_pipe.read
    assert(status.success?, message)
    assert(message.include?("must exec before using BAML"), message)
  end
when "terminal_mode"
  terminal_failure(mode: ARGV.fetch(1))
when "terminal_missing_getter"
  error = terminal_failure(path: MISSING_GETTER)
  assert_equal(
    "Unable to resolve #{MISSING_GETTER.inspect}!baml_get_api_v1: symbol not found",
    error.message
  )
when "terminal_null_field"
  terminal_failure(null_field: ARGV.fetch(1))
when "invalid_bytecode_retry_and_identity"
  use_fixture
  original = VALID_PROGRAM.dup
  assert_raises(Baml::Bridge::ProgramInitializationError) do
    Baml::Bridge.initialize!("invalid".b)
  end
  key = Baml::Bridge.initialize!(original)
  original.replace("changed-program")
  Baml::Bridge.initialize!(VALID_PROGRAM.dup)
  assert_raises(Baml::Bridge::ProgramInitializationError) do
    Baml::Bridge.initialize!(original, runtime_key: key)
  end
  assert_equal(2, fixture_inspection.count("baml_test_initialize_count"))
  assert_equal(1, fixture_inspection.count("baml_test_register_count"))
  assert_equal(7, fixture_inspection.count("baml_test_free_count"))
  other = Baml::Bridge.initialize!("other-bytecode".b)
  assert(other != key, "distinct programs must have independent registrations")
  assert_equal(3, fixture_inspection.count("baml_test_initialize_count"))
  assert_equal(9, fixture_inspection.count("baml_test_free_count"))
  assert(key > (1 << 53), "generated keys must exercise all uint64 bits")
  assert_equal(key, fixture_inspection.call_keyed(key))
  assert_equal(other, fixture_inspection.call_keyed(other))
when "concurrent_initialization"
  use_fixture
  ENV["BAML_FAKE_INIT_DELAY_MS"] = "25"
  errors = Queue.new
  threads = Array.new(12) do
    Thread.new do
      Baml::Bridge.initialize!(VALID_PROGRAM.dup)
    rescue Exception => error
      errors << error
    end
  end
  threads.each(&:join)
  assert(errors.empty?, "concurrent initialization errors: #{errors.size}")
  assert_equal(1, fixture_inspection.count("baml_test_initialize_count"))
  assert_equal(1, fixture_inspection.count("baml_test_register_count"))
  assert_equal(4, fixture_inspection.count("baml_test_free_count"))
when "fork_before_native_use"
  unless Process.respond_to?(:fork)
    exit 0
  end
  read_pipe, write_pipe = IO.pipe
  child = fork do
    read_pipe.close
    begin
      initialize_fixture
      write_pipe.write("ok")
      exit! 0
    rescue Exception => error
      write_pipe.write("#{error.class}: #{error.message}")
      exit! 1
    end
  end
  write_pipe.close
  status = wait_for_child(child)
  result = read_pipe.read
  assert(status.success?, result)
  assert_equal("ok", result)
when "fork_after_native_use"
  unless Process.respond_to?(:fork)
    exit 0
  end
  initialize_fixture
  read_pipe, write_pipe = IO.pipe
  child = fork do
    read_pipe.close
    error = assert_raises(Baml::Bridge::ForkSafetyError) do
      Baml::Bridge.initialize!(VALID_PROGRAM)
    end
    write_pipe.write(error.message)
    exit! 0
  end
  write_pipe.close
  status = wait_for_child(child)
  message = read_pipe.read
  assert(status.success?, message)
  assert(message.include?("must exec before using BAML"), message)
  Baml::Bridge.initialize!(VALID_PROGRAM.dup)
  assert_equal(1, fixture_inspection.count("baml_test_initialize_count"))
when "fork_during_initialization"
  unless Process.respond_to?(:fork)
    exit 0
  end
  use_fixture
  ENV["BAML_FAKE_INIT_DELAY_MS"] = "750"
  worker_errors = Queue.new
  worker = Thread.new do
    Baml::Bridge.initialize!(VALID_PROGRAM)
  rescue Exception => error
    worker_errors << error
  end
  deadline = Process.clock_gettime(Process::CLOCK_MONOTONIC) + 5
  until fixture_inspection.count("baml_test_initialize_started") == 1
    raise "native initialization did not start" if Process.clock_gettime(Process::CLOCK_MONOTONIC) >= deadline

    sleep 0.01
  end

  read_pipe, write_pipe = IO.pipe
  child = fork do
    read_pipe.close
    error = assert_raises(Baml::Bridge::ForkSafetyError) do
      Baml::Bridge.initialize!(VALID_PROGRAM)
    end
    write_pipe.write(error.message)
    exit! 0
  end
  write_pipe.close
  status = wait_for_child(child)
  message = read_pipe.read
  assert(status.success?, message)
  assert(message.include?("must exec before using BAML"), message)
  worker.join
  assert(worker_errors.empty?, "parent initialization failed")
  Baml::Bridge.initialize!(VALID_PROGRAM.dup)
  assert_equal(1, fixture_inspection.count("baml_test_initialize_count"))
when "fork_during_open_failure"
  unless Process.respond_to?(:fork)
    exit 0
  end

  runtime_class = Baml::Bridge.const_get(:ProcessRuntime, false)
  runtime = runtime_class.new
  open_started = Queue.new
  release_open = Queue.new
  open_failed = Queue.new
  release_failure = Queue.new
  runtime.define_singleton_method(:open_library) do |path, flags|
    if path == INVALID_LIBRARY
      open_started << true
      release_open.pop
    end
    super(path, flags)
  end
  runtime.define_singleton_method(:load_api!) do |path|
    super(path)
  rescue Baml::Bridge::RuntimeLoadError
    open_failed << true
    release_failure.pop
    raise
  end

  ENV["BAML_RUNTIME_PATH"] = INVALID_LIBRARY
  worker_error = Queue.new
  worker = Thread.new do
    runtime.initialize!(VALID_PROGRAM)
  rescue Exception => error
    worker_error << error
  end
  open_started.pop

  read_pipe, write_pipe = IO.pipe
  child = fork do
    read_pipe.close
    error = assert_raises(Baml::Bridge::ForkSafetyError) do
      runtime.initialize!(VALID_PROGRAM)
    end
    write_pipe.write(error.message)
    exit! 0
  end
  write_pipe.close
  status = wait_for_child(child)
  message = read_pipe.read
  assert(status.success?, message)
  assert(message.include?("must exec before using BAML"), message)

  release_open << true
  open_failed.pop
  ENV["BAML_RUNTIME_PATH"] = FIXTURE
  read_pipe, write_pipe = IO.pipe
  child = fork do
    read_pipe.close
    begin
      runtime.initialize!(VALID_PROGRAM)
      write_pipe.write("ok")
      exit! 0
    rescue Exception => error
      write_pipe.write("#{error.class}: #{error.message}")
      exit! 1
    end
  end
  write_pipe.close
  status = wait_for_child(child)
  result = read_pipe.read
  assert(status.success?, result)
  assert_equal("ok", result)

  release_failure << true
  worker.join
  error = worker_error.pop
  assert(error.is_a?(Baml::Bridge::RuntimeLoadError), error.inspect)
when "registered_callback_retention"
  initialize_fixture
  runtime = Baml::Bridge.instance_variable_get(:@process_runtime)
  callback = runtime.instance_variable_get(:@result_callback)
  assert(!callback.nil?, "process runtime did not retain its result callback")
  callback = nil
  3.times { GC.start(full_mark: true, immediate_sweep: true) }

  flags = FFI::DynamicLibrary::RTLD_NOW | FFI::DynamicLibrary::RTLD_LOCAL
  library = FFI::DynamicLibrary.open(FIXTURE, flags)
  invoke = FFI::Function.new(
    :int32,
    %i[uint32 pointer size_t],
    library.find_function("baml_test_invoke_registered_callback_on_thread"),
    blocking: true
  )
  payload_bytes = "retained\x00\xff".b
  payload = FFI::MemoryPointer.new(:uint8, payload_bytes.bytesize)
  payload.put_bytes(0, payload_bytes)
  caller_thread = Thread.current.object_id
  assert_equal(0, invoke.call(73, payload, payload_bytes.bytesize))
  call_id, copied, callback_thread = runtime.send(:pop_result_event)
  assert_equal(73, call_id)
  assert_equal(payload_bytes, copied)
  assert(copied.frozen?, "registered callback bytes were not frozen")
  assert_equal(Encoding::BINARY, copied.encoding)
  assert(callback_thread != caller_thread, "registered callback did not use a foreign thread")
  retained_callback = runtime.instance_variable_get(:@result_callback)
  assert(!retained_callback.nil?, "process runtime released its result callback")
  assert_equal(nil, retained_callback.pop_error)
when "foreign_thread_callback"
  native = Baml::Bridge.const_get(:Native, false)
  callbacks = Queue.new
  callback_class = native.const_get(:BorrowedBytesCallback, false)
  callback = callback_class.new do |call_id, bytes|
    callbacks << [call_id, bytes, Thread.current.object_id]
  end
  flags = FFI::DynamicLibrary::RTLD_NOW | FFI::DynamicLibrary::RTLD_LOCAL
  library = FFI::DynamicLibrary.open(THREAD_FIXTURE, flags)
  invoke = FFI::Function.new(
    :void,
    %i[pointer uint32 pointer size_t],
    library.find_function("baml_test_invoke_on_native_thread"),
    blocking: true
  )
  payload_bytes = "a\x00\xffz".b
  payload = FFI::MemoryPointer.new(:uint8, payload_bytes.bytesize)
  payload.put_bytes(0, payload_bytes)
  caller_thread = Thread.current.object_id
  invoke.call(callback.function, 41, payload, payload_bytes.bytesize)
  call_id, copied, callback_thread = callbacks.pop
  assert_equal(41, call_id)
  assert_equal(payload_bytes, copied)
  assert(copied.frozen?, "callback bytes were not frozen")
  assert_equal(Encoding::BINARY, copied.encoding)
  assert(callback_thread != caller_thread, "callback did not enter Ruby from a foreign thread")
  assert_equal(nil, callback.pop_error)

  failing = callback_class.new { raise "callback boom" }
  invoke.call(failing.function, 42, payload, payload_bytes.bytesize)
  error = failing.pop_error
  assert(error.is_a?(RuntimeError), "callback exception was not contained")
  assert_equal("callback boom", error.message)
when "real_runtime_initialization"
  ENV["BAML_RUNTIME_PATH"] = REAL_RUNTIME
  bytecode = File.binread(REAL_BYTECODE).freeze
  Baml::Bridge.initialize!(bytecode)
  Baml::Bridge.initialize!(bytecode.dup)
when "type_validation"
  assert_raises(TypeError) { Baml::Bridge.initialize!(Object.new) }
else
  raise "unknown scenario #{ARGV.first.inspect}"
end
