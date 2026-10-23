# frozen_string_literal: true

require "minitest/autorun"
require "baml_sdk"

ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class HostCallablesTest < Minitest::Test
  Fns = BamlSdk::HostCallableTests

  class MyDomainError < StandardError
    attr_reader :code

    def initialize(message, code)
      @code = code
      super(message)
    end
  end

  def test_host_callables_simple_sync_callable_returns_string
    cb = Object.new
    cb.define_singleton_method(:call) { |x| "got #{x}" }
    assert_equal "got 5", Fns.call_with_callback(cb, 5)
  end

  def test_host_callables_two_arg_callable_unpacks_positional_args
    assert_equal "answer:7", Fns.call_with_two_args(->(x, prefix) { "#{prefix}:#{x}" }, 7, "answer")
  end

  def test_host_callables_int_return_callable_round_trip
    assert_equal 42, Fns.call_int_callback(->(x) { x * 2 }, 21)
  end

  def test_baml_closure_is_a_native_callable_with_host_language_arguments
    add_ten = T.let(Fns.make_adder(10), T.any(T.proc.params(value: Integer).returns(Integer), Baml::Closure))
    assert_respond_to add_ten, :call
    assert_instance_of Baml::Closure, add_ten
    assert_equal 15, add_ten.call(5)
    assert_equal 17, add_ten.call_async(7).wait
  ensure
    add_ten&.close
  end

  def test_baml_closure_decodes_multiple_args_and_structured_return_values
    build = Fns.make_pair_builder(30)
    first = build.call(12, "Ada")
    assert_instance_of Fns::Person, first
    assert_equal ["Ada", 42], [first.name, first.age]
    second = build.call(delta: 5, label: "Grace")
    assert_instance_of Fns::Person, second
    assert_equal ["Grace", 35], [second.name, second.age]
  ensure
    build&.close
  end

  def test_baml_closure_is_reusable_and_retains_mutable_captures
    next_value = Fns.make_counter(40)
    assert_equal 41, next_value.call
    assert_equal 42, next_value.call
  ensure
    next_value&.close
  end

  def test_host_callable_void_signatures
    seen = []
    one = ->(value) { seen << [value]; nil }
    two = ->(value, label) { seen << [value, label]; nil }
    three = ->(value, label, flag) { seen << [value, label, flag]; nil }
    assert_equal 1, Fns.call_void_callback(one, 1)
    assert_equal 2, Fns.call_void_callback_two_args(two, 2, "two")
    assert_equal 3, Fns.call_void_callback_three_args(three, 3, "three", true)
    assert_nil Fns.call_throwing_void_callback(one, 4)
    raised = ArgumentError.new("void failed")
    assert_same raised, assert_raises(ArgumentError) {
      Fns.call_throwing_void_callback(->(_value) { raise raised }, 4)
    }
    void = Fns.make_void_callback
    assert_nil void.call
    forward = Fns.make_void_forwarder(two)
    assert_nil forward.call(5, "five")
    assert_nil forward.call(value: 6, label: "six")
    assert_equal [[1], [2, "two"], [3, "three", true], [4], [5, "five"], [6, "six"]], seen
  ensure
    void&.close
    forward&.close
  end

  def test_host_callables_throwing_callable_round_trips_original_python_exception
    raised = ArgumentError.new("nope")
    caught = assert_raises(ArgumentError) { Fns.call_with_callback(->(_x) { raise raised }, 1) }
    assert_same raised, caught
    assert_equal "nope", caught.message
  end

  def test_host_callables_throwing_callable_keyerror_round_trips_with_identity
    raised = KeyError.new("missing")
    assert_same raised, assert_raises(KeyError) { Fns.call_with_callback(->(_x) { raise raised }, 1) }
  end

  def test_host_callables_throwing_callable_custom_python_exception_round_trips_with_identity
    raised = MyDomainError.new("custom domain failure", 42)
    caught = assert_raises(MyDomainError) { Fns.call_with_callback(->(_x) { raise raised }, 1) }
    assert_same raised, caught
    assert_equal 42, caught.code
  end

  def test_host_callables_throwing_callable_bamlerror_wrapping_codegenned_class_is_caught_in_baml
    cb = ->(_x) {
      raise Baml::Error.new(Fns::ValidationError.new(code: 4, message: "bad shape", fields: %w[name age email phone]))
    }
    assert_equal "caught: bad shape", Fns.call_with_typed_throws(cb, 1)
  end

  def test_host_callables_throwing_callable_bamlerror_propagates_back_with_typed_fields
    raised = Baml::Error.new(Fns::ValidationError.new(code: 7, message: "propagated through", fields: %w[x y]))
    caught = assert_raises(Baml::Error) { Fns.call_with_typed_throws_propagating(->(_x) { raise raised }, 1) }
    decoded = caught.value
    assert_instance_of Fns::ValidationError, decoded
    assert_equal 7, decoded.code
    assert_equal "propagated through", decoded.message
    assert_equal %w[x y], decoded.fields
  end

  def test_host_callables_throwing_async_callable_round_trips_original_python_exception
    # Like Java, join asynchronous work inside the callback; Ruby has no async def.
    raised = ArgumentError.new("async nope")
    cb = ->(_x) {
      worker = Thread.new { Thread.current.report_on_exception = false; raise raised }
      worker.value
    }
    assert_same raised, assert_raises(ArgumentError) { Fns.call_with_callback(cb, 1) }
  end

  def test_host_callables_multiple_throws_in_flight_do_not_collide_in_registry
    first = ArgumentError.new("first")
    second = ArgumentError.new("second")
    caught_first = assert_raises(ArgumentError) { Fns.call_with_callback(->(_x) { raise first }, 1) }
    caught_second = assert_raises(ArgumentError) { Fns.call_with_callback(->(_x) { raise second }, 2) }
    assert_same first, caught_first
    assert_same second, caught_second
    refute_same caught_first, caught_second
  end

  # Ruby GC does not trigger engine GC, matching Python's xfail and Java's Disabled.
  # SDK_PARITY_LINT(skip): callable release coverage depends on host weak-reference support and remains nondeterministic
  def test_host_callables_release_fires_on_drop_of_callable
    skip "Callable release follows engine GC, not Ruby GC or call completion"
  end

  def test_host_callables_lambda_round_trip
    assert_equal "lambda-99", Fns.call_with_callback(->(x) { "lambda-#{x}" }, 99)
  end

  def test_host_callables_async_callable_runs_to_completion
    # Resolve asynchronous work inside the callback, as the Java port does.
    cb = ->(x) { Thread.new { Thread.pass; "async-#{x}" }.value }
    assert_equal "async-4", Fns.call_with_callback(cb, 4)
  end

  def test_host_callables_multiple_callable_keys_are_distinct
    counter = { a: 0, b: 0 }
    a = ->(x) { counter[:a] += 1; "a:#{x}" }
    b = ->(x) { counter[:b] += 1; "b:#{x}" }
    assert_equal "a:1", Fns.call_with_callback(a, 1)
    assert_equal "b:2", Fns.call_with_callback(b, 2)
    assert_equal({ a: 1, b: 1 }, counter)
  end

  def test_host_callables_class_callback_round_trips_pydantic_model
    person = Fns::Person.new(name: "Ada", age: 37)
    assert_equal "Ada is 37", Fns.call_with_class_callback(->(p) { "#{p.name} is #{p.age}" }, person)
  end

  def test_host_callables_call_repeatedly_invokes_callback_n_times
    invocations = []
    cb = ->(x) { invocations << x; "item-#{x}" }
    assert_equal 5.times.map { |i| "item-#{i}" }, Fns.call_repeatedly(cb, 5)
    assert_equal (0...5).to_a, invocations
  end

  def test_host_callables_call_repeatedly_with_zero_n_returns_empty_list
    invocations = []
    assert_equal [], Fns.call_repeatedly(->(x) { invocations << x; "" }, 0)
    assert_equal [], invocations
  end

  def test_host_callables_call_with_throwing_in_baml_catches_host_callable_error
    assert_equal "caught:RuntimeError", Fns.call_with_throwing(->(_x) { raise "boom from host" }, 1)
  end

  def optional_args_cb
    ->(x, y: 8, z: 9) { x * 100 + y * 10 + z }
  end

  def test_host_callables_optional_args_all_unset_apply_host_defaults
    assert_equal [589], Fns.call_callback_with_optional_args_all_unset(optional_args_cb, 5)
  end

  def test_host_callables_optional_args_partially_set_deliver_by_name
    assert_equal [529, 583], Fns.call_callback_with_optional_args_partially_set(optional_args_cb, 5)
  end

  def test_host_callables_optional_args_all_set_deliver_both
    assert_equal [523], Fns.call_callback_with_optional_args_all_set(optional_args_cb, 5)
  end
end
