# frozen_string_literal: true

require "minitest/autorun"
require "timeout"
require "sorbet-runtime"
$LOAD_PATH.unshift File.expand_path("../../../../sdks/ruby/bridge_ruby/lib", __dir__)
require "baml/bridge"

class BridgeEncodeTest < Minitest::Test
  Encode = Baml::Bridge::Encode
  Wire = BamlBridge::Cffi::V1
  Protocol = Baml::Bridge::Protocol

  class Color < T::Enum
    enums do
      Red = new("red")
      Blue = new("blue")
    end
  end
  Box = Struct.new(:value, :note, keyword_init: true)

  class Registry
    def initialize
      @types = {}
      @fields = {}
    end

    def define(type, name, fields = nil)
      @types[type] = [name, fields]
      @fields[name] = fields
    end

    def encoding_type(type)
      @types.fetch(type, [nil, nil])
    end

    def encoding_fields(name)
      @fields.fetch(name)
    end

    def enum_values(_name)
      %w[red blue]
    end
  end

  class Api
    attr_reader :cloned, :released

    def initialize
      @cloned = []
      @released = []
    end

    def handle_clone(key)
      @cloned << key
      key
    end

    def handle_release(key)
      @released << key
    end
  end

  def setup
    @registry = Registry.new
    @registry.define(Color, "user.Color")
    @registry.define(Box, "user.Box", {
      "value" => [:value, [:param, 0, "T"]],
      "note" => [:note, [:union, [[:null], [:string]]]]
    })
  end

  def test_scalars_follow_the_slot_and_the_hosts_rules
    [[nil, :null, nil], [true, :bool, :bool_value], [false, :bool, :bool_value],
     [7, :int, :int_value], [2.5, :float, :float_value], ["text", :string, :string_value]].each do |value, kind, arm|
      encoded = encode(value, [kind])
      arm ? assert_equal(arm, encoded.value) : assert_nil(encoded.value)
      assert_nil encoded.value_type
      assert_equal value, encoded.public_send(arm) if arm
    end
    assert_equal 7.0, encode(7, [:float]).float_value
    assert_raises(TypeError) { encode(true, [:int]) }
    assert_raises(TypeError) { encode(1.0, [:int]) }
    assert_raises(TypeError) { encode(2**62, [:int]) }
    assert_equal -(2**62), encode(-(2**62), [:int]).int_value
    assert_equal 2**62 - 1, encode(2**62 - 1, [:int]).int_value
    assert_raises(TypeError) { encode(Object.new) }
  end

  def test_integer_to_float_conversion_is_exact_at_any_magnitude
    assert_raises(TypeError) { encode(2**53 + 1, [:float]) }
    [2**53, 2**63, 2**80, -(2**80)].each do |value|
      assert_equal value.to_f, encode(value, [:float]).float_value
    end
    assert_raises(TypeError) { encode(2**1024, [:float]) }
  end

  def test_enums_take_members_as_is_and_strings_by_conversion
    [Color::Red, "red"].each do |value|
      encoded = encode(value, [:enum, "user.Color"])
      assert_equal "user.Color", encoded.enum_value.name
      assert_equal "red", encoded.enum_value.value
      assert_nil encoded.value_type
    end
    assert_raises(TypeError) { encode("green", [:enum, "user.Color"]) }
    assert_raises(TypeError) { encode(Color::Red, [:enum, "user.Other"]) }
  end

  def test_literals_are_matched_exactly
    [7, "draft", true, false].each do |value|
      assert_nil encode(value, [:literal, value]).value_type
    end
    assert_raises(TypeError) { encode(8, [:literal, 7]) }
    assert_raises(TypeError) { encode(1, [:literal, true]) }
    assert_raises(TypeError) { encode(1.0, [:literal, 1.0]) }
    assert_equal :int_value, encode(1, union([:literal, 1.0], [:literal, 1])).value
  end

  def test_unions_rank_identity_over_natural_shape_over_conversion
    assert_equal :int_value, encode(5, union([:float], [:int])).value
    assert_equal :float_value, encode(5, union([:float], [:string])).value
    assert_equal :int_value, encode(5, union([:float], [:unknown])).value
    assert_equal :string_value, encode("red", union([:enum, "user.Color"], [:string])).value
    assert_equal :enum_value, encode(Color::Red, union([:unknown], [:enum, "user.Color"])).value
    list = encode([5], union([:unknown], [:list, [:float]]))
    assert_equal [:list, [:float]], Encode::Slot.from_wire(list.value_type)
    assert_equal 5.0, list.list_value.values.first.float_value
    assert_nil encode(nil, union([:int], [:null])).value
    assert_equal :string_value, encode("text", union([:int], union([:float], union([:null], [:string])))).value
  end

  def test_several_fits_collapse_when_identical_and_are_ambiguous_otherwise
    assert_nil encode(nil, union(union([:int], [:null]), union([:string], [:null]))).value
    assert_equal :int_value, encode(5, union([:literal, 5], [:int])).value
    natural = encode([1], union([:list, union([:int], [:string])], [:list, [:int]]))
    assert_equal [:list, [:int]], Encode::Slot.from_wire(natural.value_type)
    error = assert_raises(TypeError) { encode([], union([:list, [:int]], [:list, [:string]])) }
    assert_includes error.message, "fits more than one"
    assert Encode::Slot.equivalent?(union([:int], [:string]), union([:string], [:int]))
  end

  def test_distinct_mixed_scalar_lists_plan_in_linear_time
    scalars = [1, 1.5, "s", true, nil]
    subsets = (1...(1 << scalars.length)).map do |mask|
      scalars.each_index.select { |index| mask[index] == 1 }.map { |index| scalars[index] }
    end
    value = subsets.combination(3).first(4_000)
    tree = Encode::Read.new(@registry).read(value, "mixed")
    planned = Timeout.timeout(10) { Encode::Plan.new(@registry).plan(tree, [:unknown]) }
    assert_equal 4_000, planned.children.length
    assert_equal :union, planned.ty[1][0]
    assert_equal 4_000, planned.ty[1][1].length
  end

  def test_type_identity_recurses_through_every_composite_and_function_part
    left = union([:int], union([:string], [:int]))
    right = union([:string], [:int])
    wrap = ->(ty) { [:map, [:string], [:list, [:class, "user.Box", [ty]]]] }
    signature = ->(ty) { [:function, [["value", wrap.call(ty), false]], wrap.call(ty), ty] }
    a, b = signature.call(left), signature.call(right)
    assert Encode::Slot.equivalent?(a, b)
    assert_equal a, Encode::Slot.union([a, b])
    changed_name = signature.call(right)
    changed_name[1][0][0] = "other"
    refute Encode::Slot.equivalent?(a, changed_name)
    changed_mode = signature.call(right)
    changed_mode[1][0][2] = true
    refute Encode::Slot.equivalent?(a, changed_mode)
    refute Encode::Slot.equivalent?(a, [:function, [], wrap.call(right), right])
    refute Encode::Slot.equivalent?(a, [:function, b[1], [:int], right])
    refute Encode::Slot.equivalent?(a, [:function, b[1], b[2], [:never]])
  end

  def test_invalid_nonbinary_strings_are_refused_while_reading
    invalid = "\xff".b.force_encoding(Encoding::UTF_8)
    [invalid, { invalid => 1 }].each do |value|
      error = assert_raises(Encode::Refusal) { Encode::Read.new(@registry).read(value, "invalid") }
      assert_includes error.message, "invalid string"
    end
    assert_equal :string_value, encode("café").value
  end

  def test_unstatable_types_are_bridge_faults_and_still_type_errors_for_calls
    [[:other], [:handle], [:literal, 1.5]].each do |slot|
      assert_raises(Encode::Fault) { Encode::Slot.to_wire(slot) }
    end
    Protocol.define_function("user.unstatable", params: [["items", [:list, [:other]]]])
    assert_raises(TypeError) { Protocol.encode_call(1, "user.unstatable", { "items" => [] }) }
  end

  def test_open_slots_take_the_values_own_type_and_fill_holes_with_unknown
    assert_equal [:list, [:unknown]], type_of([])
    assert_equal [:map, [:string], [:unknown]], type_of({})
    assert_equal [:list, union([:int], [:string])], type_of([1, "a"])
    result = encode([[], [1]])
    assert_equal [:list, [:list, [:int]]], Encode::Slot.from_wire(result.value_type)
    assert_equal [:list, [:int]], Encode::Slot.from_wire(result.list_value.values.first.value_type)
    assert_equal [:list, [:list, [:list, [:int]]]], type_of([[[]], [[1]]])
    assert_equal [:list, [:int]], type_of([1], [:interface, "user.Anything"])
    mixed = [:list, union([:list, [:int]], [:list, [:string]])]
    assert_equal mixed, type_of([[1], ["a"]])
    assert_equal mixed, type_of([[], [1], ["a"]])
    assert_equal [:list, union([:map, [:string], [:int]], [:map, [:string], [:string]])],
                 type_of([{ "a" => 1 }, { "b" => "text" }])
  end

  def test_realized_containers_state_the_slot_not_the_narrower_elements
    slot = [:list, union([:int], [:string])]
    assert_equal slot, type_of([1, 2], slot)
    assert_equal [:list, [:float]], type_of([], [:list, [:float]])
    map = encode({ "a" => 3 }, [:map, [:string], [:float]])
    assert_equal [:map, [:string], [:float]], Encode::Slot.from_wire(map.value_type)
    assert_equal 3.0, map.map_value.entries.first.value.float_value
  end

  def test_fixed_class_arguments_drive_fields_and_optional_fields_are_present
    result = encode(Box.new(value: 3), [:class, "user.Box", [[:float]]])
    assert_equal [:class, "user.Box", [[:float]]], Encode::Slot.from_wire(result.value_type)
    assert_equal %w[value note], result.class_value.fields.map(&:string_key)
    assert_equal 3.0, result.class_value.fields.first.value.float_value
    assert_nil result.class_value.fields.last.value.value
    assert_raises(TypeError) { encode({ "value" => 3 }, [:class, "user.Box", [[:int]]]) }
  end

  def test_map_keys_are_strings_and_errors_carry_the_entry
    [1, true, :symbol, Color::Red].each do |key|
      error = assert_raises(TypeError) { encode({ key => 1 }) }
      assert_includes error.message, "argument[key]"
      assert_includes error.message, "map keys must be string"
    end
    error = assert_raises(TypeError) { encode({ "bad" => "not int" }, [:map, [:string], [:int]]) }
    assert_includes error.message, 'argument["bad"]'
    assert_raises(TypeError) { encode({ "red" => 1 }, [:map, [:enum, "user.Color"], [:int]]) }
  end

  def test_a_value_the_wire_cannot_carry_is_refused_before_anything_is_created
    api = Api.new
    handle = Baml::Handle.new(api, 41, :ADT_TAGGED_HEAP_HANDLE)
    scope = Baml::Bridge::EncodingScope.new
    nested = 34.times.reduce(handle) { |value, _| { "nested" => value } }
    error = assert_raises(TypeError) { encode(nested, scope: scope) }
    assert_includes error.message, "nested too deeply"
    assert_empty api.cloned
    assert_raises(TypeError) { encode([handle], union([:list, [:int]], [:list, [:string]]), scope: scope) }
    assert_empty api.cloned
    result = encode([1, handle], scope: scope)
    assert_equal [:list, [:unknown]], Encode::Slot.from_wire(result.value_type)
    assert_equal [41], api.cloned
    cyclic = []
    cyclic << cyclic
    assert_raises(TypeError) { encode(cyclic) }
    handle.close
  ensure
    scope&.close
  end

  def test_an_endless_container_is_refused_at_the_node_budget
    endless = Class.new(Array) do
      def each
        loop { yield 1 }
      end
    end.new
    reader = Encode::Read.new(@registry, budget: 16)
    error = assert_raises(TypeError) { reader.read(endless, "endless") }
    assert_includes error.message, "node budget"
    assert_equal 1 << 20, Encode::Read::NODE_BUDGET
    reader = Encode::Read.new(@registry, budget: 2)
    reader.read(1, "first")
    assert_raises(TypeError) { reader.read([1], "second") }
  end

  def test_planning_trials_share_one_budget_even_for_recursive_class_unions
    left = Class.new(Struct.new(:child))
    right = Class.new(Struct.new(:child))
    a = [:class, "user.Left", [[:int]]]
    b = [:class, "user.Left", [[:unknown]]]
    c = [:class, "user.Right", [[:int]]]
    d = [:class, "user.Right", [[:unknown]]]
    @registry.define(left, "user.Left", { "child" => [:child, union([:null], c, d)] })
    @registry.define(right, "user.Right", { "child" => [:child, union([:null], a, b)] })
    value = 8.times.reduce(nil) { |child, index| (index.even? ? right : left).new(child) }
    tree = Encode::Read.new(@registry).read(value, "recursive")
    error = assert_raises(TypeError) { Encode::Plan.new(@registry, budget: 100).plan(tree, union(a, b)) }
    assert_includes error.message, "planning-step budget"
    assert_equal 1 << 20, Encode::Plan::STEP_BUDGET
    assert_raises(TypeError) { encode(1, [:alias, "unsupported.cycle"]) }
  end

  def test_reading_happens_once_and_host_exceptions_are_not_reclassified
    reads = 0
    type = Class.new do
      define_method(:value) { reads += 1; 1 }
    end
    @registry.define(type, "user.ReadOnce", { "value" => [:value, [:param, 0, "T"]] })
    slot = union([:class, "user.ReadOnce", [[:int]]], [:class, "user.ReadOnce", [[:string]]])
    assert_equal :class_value, encode(type.new, slot).value
    assert_equal 1, reads
    type.define_method(:value) { raise "getter failed" }
    error = assert_raises(RuntimeError) { encode(type.new, slot) }
    assert_equal "getter failed", error.message
  end

  def test_encode_call_plans_and_checks_every_argument_before_cloning
    api = Api.new
    handle = Baml::Handle.new(api, 41, :ADT_TAGGED_HEAP_HANDLE)
    scope = Baml::Bridge::EncodingScope.new
    Protocol.define_function("user.encode_test", params: [["bad", [:int]], ["value", [:float]]])
    error = assert_raises(TypeError) do
      Protocol.encode_call(1, "user.encode_test", { "handle" => handle, "bad" => "wrong" }, scope: scope)
    end
    assert_match(/\Auser.encode_test: bad:/, error.message)
    assert_empty api.cloned
    nested = 34.times.reduce(1) { |value, _| { "nested" => value } }
    assert_raises(TypeError) do
      Protocol.encode_call(1, "user.encode_test", { "handle" => handle, "deep" => nested }, scope: scope)
    end
    assert_empty api.cloned
    args = Wire::CallFunctionArgs.decode(Protocol.encode_call(2, "user.encode_test", { "value" => 5, "handle" => handle }, scope: scope))
    assert_equal 5.0, args.kwargs.first.value.float_value
    assert_equal [41], api.cloned
    scope.close
    assert_equal [41], api.released
    handle.close
  end

  def test_complete_host_call_uses_the_abi5_signature
    native = Baml::Bridge.const_get(:Native, false)
    received = []
    callback = FFI::Function.new(:void, %i[uint32 pointer size_t]) do |id, pointer, length|
      received << [id, length.zero? ? "".b : pointer.read_bytes(length)]
    end
    table = native::ApiV1.new
    table[:complete_host_call] = callback
    api = native::Api.allocate
    api.instance_variable_set(:@owner_pid, Process.pid)
    api.instance_variable_set(:@functions, {})
    api.instance_variable_set(:@table, table)
    completion = Wire::HostCallCompletion.encode(Wire::HostCallCompletion.new(returned: { int_value: 5 }))
    api.complete_host_call(7, completion)
    api.complete_host_call(8, "".b)
    assert_equal [[7, completion], [8, "".b]], received
  end

  def test_type_descriptors_round_trip_and_refuse_nonstatic_heads
    slots = [[:null], [:never], [:unknown], [:enum, "user.Color"], [:interface, "user.I"],
             [:literal, 3], [:literal, true], [:literal, "x"], [:class, "user.Box", [[:int]]],
             [:map, [:string], [:list, union([:int], [:null])]]]
    slots.each { |slot| assert_equal slot, Encode::Slot.from_wire(Encode::Slot.to_wire(slot)) }
    %i[class_ty enum interface enum_variant type_alias].each do |kind|
      klass = Wire.const_get({ class_ty: :BamlTyClass, enum: :BamlTyEnum, interface: :BamlTyInterface,
                              enum_variant: :BamlTyEnumVariant, type_alias: :BamlTyTypeAlias }.fetch(kind))
      [:live, :anonymous_label].each do |head|
        ty = Wire::BamlTy.new(**{ kind => klass.new(**{ head => head == :live ? 41 : "display" }) })
        assert_equal [:other], Encode::Slot.from_wire(ty)
        assert_raises(TypeError) { Encode::Slot.to_wire(Encode::Slot.from_wire(ty)) }
      end
    end
  end

  def test_wire_depth_counts_statements_entries_and_the_argument_envelope
    assert_equal 0, Encode::Slot.wire_depth(encode(1))
    assert_equal 4, Encode::Slot.wire_depth(encode([], [:list, [:int]]))
    assert_equal 4, Encode::Slot.wire_depth(encode({ "x" => 1 }))
    assert_equal 1, Encode::Slot.wire_depth(encode(Color::Red))
    nested = 48.times.reduce(1) { |value, _| [value] }
    assert_equal 98, Encode::Slot.wire_depth(encode(nested))
    assert_raises(TypeError) { encode([nested]) }
  end

  private

  def union(*members)
    [:union, members]
  end

  def encode(value, slot = [:unknown], scope: nil)
    tree = Encode::Read.new(@registry).read(value, "argument")
    plan = Encode::Plan.new(@registry).plan(tree, slot)
    Encode::Commit.check_depth!(Encode::Commit.depth(plan), "argument")
    result = Encode::Commit.commit(plan, scope: scope)
    assert_equal Encode::Commit.depth(plan), Encode::Slot.wire_depth(result)
    result
  end

  def type_of(value, slot = [:unknown])
    Encode::Slot.from_wire(encode(value, slot).value_type)
  end
end
