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
      @aliases = {}
      @normalized_aliases = {}
      @crossings = {}
      @params = {}
    end

    def define(type, name, fields = nil, crossing: :fields, type_params: [])
      @params[name] = type_params
      Baml::Types.register(type, name, fields ? type_params.length : nil)
      @types[type] = [name, fields]
      @fields[name] = fields
      @crossings[name] = crossing
    end

    def class_type_params(name)
      @params.fetch(name)
    end

    def class_crossing(name)
      @crossings.fetch(name, :fields)
    end

    def define_alias(name, body, recursive: false)
      @aliases[name] = [body, recursive]
      @normalized_aliases.clear
    end

    def normalized_alias(name)
      @normalized_aliases[name] ||= yield @aliases[name]
    end

    def alias_definition(name)
      @aliases[name]
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
    }, type_params: ["T"])
  end

  def test_scope_close_defers_kill_until_every_release_and_stays_idempotent
    entered, resume = Queue.new, Queue.new
    api = Api.new
    api.define_singleton_method(:handle_release) do |key|
      super(key)
      if key == 41
        entered << Thread.current
        resume.pop(timeout: 5) or raise "scope close did not resume"
      end
    end
    scope = Baml::Bridge::EncodingScope.new
    [41, 42, 43].each { |key| scope.own(api, key) }
    worker = Thread.new { scope.close }
    assert_same worker, entered.pop(timeout: 5)
    worker.kill
    resume << true
    assert worker.join(5), "scope close did not finish"
    assert_equal [41, 42, 43], api.released
    scope.close
    assert_equal [41, 42, 43], api.released
  ensure
    resume << true if resume
    worker&.kill
    worker&.join(5)
    scope&.close
  end

  def test_scope_attempts_every_release_before_raising_the_first_error
    api = Api.new
    failure = RuntimeError.new("release failed")
    api.define_singleton_method(:handle_release) do |key|
      super(key)
      raise failure if key == 1
    end
    scope = Baml::Bridge::EncodingScope.new
    [1, 2, 3].each { |key| scope.own(api, key) }
    assert_same failure, assert_raises(RuntimeError) { scope.close }
    assert_equal [1, 2, 3], api.released
    scope.close
    assert_equal [1, 2, 3], api.released
  end

  def test_untyped_binary_strings_are_bytes_even_when_valid_utf8
    ["\xff".b, "café".b].each do |value|
      assert_equal value, encode(value, [:unknown]).uint8array_value
      assert_equal value, encode([value], [:list, [:unknown]]).list_value.values.first.uint8array_value
    end
  end

  def test_string_slots_validate_binary_utf8_values_and_keys
    assert_equal "café", encode("café".b, [:string]).string_value
    assert_raises(TypeError) { encode("\xff".b, [:string]) }
    slot = [:map, [:string], [:int]]
    assert_equal "café", encode({ "café".b => 1 }, slot).map_value.entries.first.string_key
    assert_raises(TypeError) { encode({ "\xff".b => 1 }, slot) }
  end

  def test_alias_normalization_is_cached_per_registry_and_invalidated_on_definition
    @registry.define_alias("user.A", [:list, [:int]])
    @registry.define_alias("user.B", [:map, [:string], [:alias, "user.A"]])
    first = Encode::Slot.stated_form([:alias, "user.B"], @registry)
    assert_same first, Encode::Slot.stated_form([:alias, "user.B"], @registry)
    @registry.define_alias("user.A", [:string])
    assert_equal [:map, [:string], [:string]], Encode::Slot.stated_form([:alias, "user.B"], @registry)
    other = Registry.new
    other.define_alias("user.B", [:bool])
    assert_equal [:bool], Encode::Slot.stated_form([:alias, "user.B"], other)
  end

  def test_alias_cache_does_not_publish_a_normalization_from_before_redefinition
    name = "user.CacheRace"
    Protocol.define_alias(name, [:int])
    entered, resume = Queue.new, Queue.new
    normalizer = Thread.new do
      Protocol.normalized_alias(name) do |definition|
        entered << true
        resume.pop
        definition.first
      end
    end
    Timeout.timeout(5) { entered.pop }
    Protocol.define_alias(name, [:string])
    resume << true
    assert_equal [:int], normalizer.value
    assert_equal [:string], Encode::Slot.stated_form([:alias, name], Protocol)
  ensure
    resume << true if resume
    normalizer&.join(5)
  end

  def test_expanding_aliases_spend_the_planning_budget_even_with_a_warm_cache
    @registry.define_alias("user.A0", [:int])
    1.upto(14) do |index|
      previous = [:alias, "user.A#{index - 1}"]
      @registry.define_alias("user.A#{index}", union([:list, previous], [:map, [:string], previous]))
    end
    tree = Encode::Read.new(@registry).read([], "alias")
    2.times do
      Timeout.timeout(1) do
        error = assert_raises(Encode::Fault) { Encode::Plan.new(@registry, budget: 10).plan(tree, [:alias, "user.A14"]) }
        assert_includes error.message, "planning-step budget"
      end
    end
    # Prime a smaller alias, then ensure cache hits cannot evade later union work.
    Encode::Slot.stated_form([:alias, "user.A6"], @registry)
    Timeout.timeout(1) do
      assert_raises(Encode::Fault) { Encode::Plan.new(@registry, budget: 10).plan(tree, [:alias, "user.A6"]) }
    end
  end

  def expanding_aliases(depth)
    @registry.define_alias("user.Deep0", [:int])
    1.upto(depth) do |index|
      previous = [:alias, "user.Deep#{index - 1}"]
      @registry.define_alias("user.Deep#{index}", union([:list, previous], [:map, [:string], previous]))
    end
  end

  def plan_alias_or_refuse(tree, slot)
    assert_equal :list, Encode::Plan.new(@registry).plan(tree, slot).kind
  rescue Encode::Fault => error
    assert_includes error.message, "planning-step budget"
  end

  def test_default_budget_bounds_direct_aliases_and_warm_cache_repeats
    expanding_aliases(16)
    tree = Encode::Read.new(@registry).read([], "alias")
    2.times do
      Timeout.timeout(0.75) do
        assert_equal :list, Encode::Plan.new(@registry).plan(tree, [:alias, "user.Deep14"]).kind
        plan_alias_or_refuse(tree, [:alias, "user.Deep16"])
      end
      Timeout.timeout(0.1) do
        error = assert_raises(Encode::Fault) { Encode::Plan.new(@registry, budget: 10).plan(tree, [:alias, "user.Deep14"]) }
        assert_includes error.message, "planning-step budget"
      end
    end
  end

  def test_class_field_alias_normalization_spends_the_budget
    expanding_aliases(18)
    klass = Class.new(Struct.new(:items))
    @registry.define(klass, "user.DeepField", { "items" => [:items, [:alias, "user.Deep18"]] })
    tree = Encode::Read.new(@registry).read(klass.new([]), "field")
    2.times do
      Timeout.timeout(0.1) do
        error = assert_raises(Encode::Fault) { Encode::Plan.new(@registry, budget: 10).plan(tree, [:class, "user.DeepField", []]) }
        assert_includes error.message, "planning-step budget"
      end
    end
  end

  def test_generic_class_field_alias_normalization_spends_the_budget
    expanding_aliases(18)
    klass = Class.new(Struct.new(:value, :items))
    @registry.define(klass, "user.DeepGeneric", {
      "value" => [:value, [:param, 0, "T"]],
      "items" => [:items, [:alias, "user.Deep18"]]
    }, type_params: ["T"])
    tree = Encode::Read.new(@registry).read(klass.new(1, []), "generic")
    [[:hole], [:int]].each do |arg|
      2.times do
        Timeout.timeout(0.1) do
          error = assert_raises(Encode::Fault) { Encode::Plan.new(@registry, budget: 10).plan(tree, [:class, "user.DeepGeneric", [arg]]) }
          assert_includes error.message, "planning-step budget"
        end
      end
    end
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

  def test_bigint_literal_types_are_decimal_while_values_stay_hex
    [42, -42, 2**80].each do |value|
      slot = [:literal, :bigint, value]
      wire = Encode::Slot.to_wire(slot)
      assert_equal value.to_s(10), wire.literal.bigint_value
      assert_equal slot, Encode::Slot.from_wire(Wire::BamlTy.new(literal: { bigint_value: value.to_s(10) }))
      assert_equal value.to_s(16), encode(value, slot).bigint_value
      assert_equal value, Protocol.decode_value(Wire::BamlOutboundValue.new(literal_value: { bigint_value: value.to_s(16) }))
    end
  end

  def test_media_equality_includes_kind_and_every_content_field
    image = Baml::Image.from_url("https://example.com/image", mime_type: "image/png")
    same = Baml::Image.from_url("https://example.com/image", mime_type: "image/png")
    assert_equal image, same
    assert image.eql?(same)
    assert_equal image.hash, same.hash
    assert_equal 1, { image => 1 }[same]
    [Baml::Audio.from_url(image.url, mime_type: image.mime_type),
     Baml::Image.from_url(image.url), Baml::Image.from_url("other", mime_type: image.mime_type),
     Baml::Image.from_base64("YQ=="), Baml::Image.from_file_content("image.png", "YQ==")].each do |other|
      refute_equal image, other
      refute image.eql?(other)
    end
    refute_equal Baml::Image.from_base64("YQ=="), Baml::Image.from_base64("Yg==")
    refute_equal Baml::Image.from_file_content("a.png", "YQ=="), Baml::Image.from_file_content("b.png", "YQ==")
  end

  def test_bigints_bytes_and_literal_kinds_follow_the_slot
    [7, 2**62, -(2**80)].each do |value|
      result = encode(value, [:bigint])
      assert_equal value.to_s(16), result.bigint_value
      assert_nil result.value_type
    end
    assert_equal :bigint_value, encode(2**62).value
    assert_equal :int_value, encode(7, union([:bigint], [:int])).value
    assert_equal "2a", encode(42, [:literal, :bigint, 42]).bigint_value
    assert_raises(TypeError) { encode(43, [:literal, :bigint, 42]) }
    assert_equal :int_value, encode(42, union([:literal, :bigint, 42], [:literal, 42])).value
    assert_equal :uint8array_value, encode("ab".b).value
    assert_equal :string_value, encode("ab".b, [:string]).value
    assert_equal :uint8array_value, encode("ab", [:bytes]).value
    assert_equal :string_value, encode("ab", union([:bytes], [:string])).value
    assert_equal :uint8array_value, encode("ab".b, union([:string], [:bytes])).value
    assert_equal [:list, [:bytes]], type_of(["ab".b])
  end

  def test_bytes_convert_to_text_only_when_valid_utf8_including_map_keys
    assert_raises(Encode::Refusal) { encode("\xff".b, [:string]) }
    assert_raises(Encode::Refusal) { encode({ "\xff".b => 1 }) }
    assert_equal "café", encode("café".b, [:string]).string_value
    [[:unknown], [:map, [:string], [:int]]].each do |slot|
      result = encode({ "café".b => 1 }, slot)
      assert_equal "café", result.map_value.entries.first.string_key
      assert_equal Encoding::UTF_8, result.map_value.entries.first.string_key.encoding
    end
    assert_equal :uint8array_value, encode("\xff".b).value
  end

  def test_binary_strings_do_not_convert_to_enums
    assert_raises(Encode::Refusal) { encode("red".b, [:enum, "user.Color"]) }
    assert_equal :string_value, encode("red".b, union([:enum, "user.Color"], [:string])).value
  end

  def test_enum_variants_admit_only_the_named_variant
    [Color::Red, "red"].each do |value|
      assert_equal "red", encode(value, [:enum_variant, "user.Color", "red"]).enum_value.value
      assert_raises(TypeError) { encode(value, [:enum_variant, "user.Color", "blue"]) }
      assert_equal "red", encode(value, union([:enum_variant, "user.Color", "blue"], [:enum_variant, "user.Color", "red"])).enum_value.value
    end
  end

  def test_recursive_aliases_are_judged_by_their_bodies_and_terminate
    rec = [:alias, "user.Rec"]
    @registry.define_alias("user.Rec", union([:int], [:list, rec]), recursive: true)
    result = encode([1, [2]], rec)
    assert_equal [:list, rec], Encode::Slot.from_wire(result.value_type)
    assert_equal [:list, rec], Encode::Slot.from_wire(result.list_value.values.last.value_type)
    assert_raises(TypeError) { encode(["not an int"], rec) }
    @registry.define_alias("user.Ints", [:list, [:int]])
    assert_equal [:list, [:int]], type_of([1], [:alias, "user.Ints"])
    @registry.define_alias("user.Number", [:int])
    assert_equal [:list, [:int]], Encode::Slot.stated_form([:list, [:alias, "user.Number"]], @registry)

    a, b = [:alias, "user.A"], [:alias, "user.B"]
    @registry.define_alias("user.A", b, recursive: true)
    @registry.define_alias("user.B", a, recursive: true)
    assert_raises(TypeError) { encode(1, a) }
    @registry.define_alias("user.A", union(b, [:list, a]), recursive: true)
    assert_equal [:list, a], type_of([[]], a)
    assert_raises(TypeError) { encode([1], a) }
    @registry.define_alias("user.A", b)
    @registry.define_alias("user.B", a)
    assert_raises(Encode::Fault) { encode(1, a) }
  end

  def test_default_budget_bounds_overlapping_mutually_recursive_alias_trials
    a, b = [:alias, "user.A"], [:alias, "user.B"]
    @registry.define_alias("user.A", union([:list, a], [:list, b], [:int]), recursive: true)
    @registry.define_alias("user.B", union([:list, a], [:list, b], [:string]), recursive: true)
    tree = Encode::Read.new(@registry).read(30.times.reduce(1) { |value, _| [value] }, "recursive")
    error = Timeout.timeout(15) do
      assert_raises(Encode::Fault) { Encode::Plan.new(@registry).plan(tree, a) }
    end
    assert_includes error.message, "planning-step budget"
  end

  def test_overlapping_mutually_recursive_alias_trials_share_the_planning_budget
    a, b = [:alias, "user.A"], [:alias, "user.B"]
    @registry.define_alias("user.A", union([:list, a], [:list, b], [:int]), recursive: true)
    @registry.define_alias("user.B", union([:list, a], [:list, b], [:string]), recursive: true)
    tree = Encode::Read.new(@registry).read(8.times.reduce(1) { |value, _| [value] }, "recursive")
    error = assert_raises(TypeError) { Encode::Plan.new(@registry, budget: 100).plan(tree, a) }
    assert_includes error.message, "planning-step budget"
    error = assert_raises(TypeError) { encode([[1]], a) }
    assert_includes error.message, "fits more than one"
  end

  def test_leaves_fit_their_kind_any_open_slot_and_open_union_members
    @registry.define(Class.new, "user.ImageWrapper", {}, crossing: :media_image)
    image = Baml::Image.from_url("https://example.com/asset", mime_type: "image/png")
    [[:media, :image], [:media, :generic], [:class, "user.ImageWrapper", []],
     [:unknown], [:interface, "user.I"], union([:int], [:media, :image]), union([:int], [:unknown])].each do |slot|
      result = encode(image, slot)
      assert_nil result.value_type
      assert_equal :IMAGE, result.media_value.media
      assert_equal image.url, result.media_value.url
    end
    assert_raises(TypeError) { encode(image, [:media, :audio]) }
    assert_raises(TypeError) { encode(image, [:class, "user.ImageWrapper", [[:int]]]) }
    assert_equal "scan.pdf", encode(Baml::Pdf.from_file_content("/tmp/scan.pdf", "JVBERg==")).media_value.file_content.name
    assert_equal "YWJj", encode(Baml::Audio.from_base64("YWJj")).media_value.base64
    opaque = Class.new(Struct.new(:value))
    @registry.define(opaque, "user.Opaque", { "value" => [:value, [:int]] }, crossing: :opaque)
    error = assert_raises(TypeError) { encode(opaque.new(1)) }
    assert_includes error.message, "built only by BAML"
    api = Api.new
    handle = Baml::Handle.new(api, 41, :ADT_TAGGED_HEAP_HANDLE)
    scope = Baml::Bridge::EncodingScope.new
    [[:int], [:unknown], union([:int], [:string])].each do |slot|
      assert_equal 41, encode(handle, slot, scope: scope).handle.key
    end
    assert_equal [41, 41, 41], api.cloned
  ensure
    scope&.close
    handle&.close
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

  def test_canonical_union_order_does_not_depend_on_array_sharing
    int = [:int]
    left = union([:map, int, int], [:map, int, [:string]])
    right = union([:map, [:int], [:string]], [:map, [:int], [:int]])
    assert Encode::Slot.equivalent?(left, right)
    assert_equal left, Encode::Slot.union([left, right])
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

  def test_generic_class_arguments_come_from_the_slot_the_value_or_the_fields
    box = Box.new(value: 3)
    assert_equal [:class, "user.Box", [[:int]]], type_of(box)
    assert_empty Baml::Types.instance_args(box)
    assert_equal [:class, "user.Box", [[:float]]], type_of(box, [:class, "user.Box", [[:float]]])
    Baml.bind_types(box, Integer)
    assert_equal [:class, "user.Box", [[:int]]], type_of(box)
    assert_raises(TypeError) { encode(box, [:class, "user.Box", [[:float]]]) }
    assert_equal [:class, "user.Box", [[:int]]], type_of(Box.new(value: 3), union(
      [:class, "user.Box", [[:int]]], [:class, "user.Box", [[:string]]]
    ))
    assert_raises(TypeError) do
      encode(Box.new(value: []), union([:class, "user.Box", [[:list, [:int]]]], [:class, "user.Box", [[:list, [:string]]]]))
    end

    param = [:param, 0, "T"]
    pair = Class.new(Struct.new(:a, :b))
    @registry.define(pair, "user.Pair", { "a" => [:a, [:list, param]], "b" => [:b, param] }, type_params: ["T"])
    result = encode(pair.new([], 1))
    assert_equal [:class, "user.Pair", [[:int]]], Encode::Slot.from_wire(result.value_type)
    assert_equal [:list, [:int]], Encode::Slot.from_wire(result.class_value.fields.first.value.value_type)
    error = assert_raises(Encode::TypeArgConflict) { encode(pair.new([1], "x")) }
    assert_includes error.message, "user.Pair"
    assert_includes error.message, "T"
    assert_raises(Encode::TypeArgConflict) { encode(pair.new(["x"], 1)) }
    assert_raises(Encode::MissingTypeArgs) { encode(pair.new([], [])) }
    @registry.define(pair, "user.Pair", { "a" => [:a, param], "b" => [:b, [:param, 1, "U"]] }, type_params: %w[T U])
    result = encode(pair.new("x", 1), [:class, "user.Pair", [param, [:float]]])
    assert_equal [:class, "user.Pair", [[:string], [:float]]], Encode::Slot.from_wire(result.value_type)
    assert_equal 1.0, result.class_value.fields.last.value.float_value
    phantom = Class.new(Struct.new(:value))
    @registry.define(phantom, "user.Phantom", { "value" => [:value, [:int]] }, type_params: ["T"])
    assert_raises(Encode::MissingTypeArgs) { encode(phantom.new(1)) }
  end

  def test_fixed_literal_union_arm_does_not_contribute_parameter_evidence
    param = [:param, 0, "T"]
    klass = Class.new(Struct.new(:value, :status))
    @registry.define(klass, "user.LiteralStatus", {
      "value" => [:value, param], "status" => [:status, union(param, [:literal, "none"])]
    }, type_params: ["T"])
    [[:unknown], [:class, "user.LiteralStatus", [param]]].each do |slot|
      assert_equal [:class, "user.LiteralStatus", [[:int]]], type_of(klass.new(1, "none"), slot)
      assert_equal [:class, "user.LiteralStatus", [[:string]]], type_of(klass.new("a", "none"), slot)
      assert_raises(Encode::TypeArgConflict) { encode(klass.new(1, "other"), slot) }
    end
  end

  def test_fixed_literal_union_arms_survive_list_and_map_inference
    param = [:param, 0, "T"]
    klass = Class.new(Struct.new(:value, :status))
    [[:list, union(param, [:literal, "none"])],
     [:map, [:string], union(param, [:literal, "none"])]].each do |container|
      @registry.define(klass, "user.ContainerStatus", {
        "value" => [:value, param], "status" => [:status, container]
      }, type_params: ["T"])
      wrap = ->(items) { container[0] == :list ? items : items.each_with_index.to_h { |item, i| [i.to_s, item] } }
      [[1, ["none"], [:int]], [1, [2, "none"], [:int]],
       ["a", ["none"], [:string]], [1, ["none", 2], [:int]]].each do |value, items, expected|
        [[:unknown], [:class, "user.ContainerStatus", [param]]].each do |slot|
          assert_equal [:class, "user.ContainerStatus", [expected]], type_of(klass.new(value, wrap.call(items)), slot)
        end
      end
      assert_raises(Encode::TypeArgConflict) { encode(klass.new(1, wrap.call(["other"]))) }
      @registry.define(klass, "user.ContainerStatus", { "status" => [:status, container] }, type_params: ["T"])
      assert_equal [:class, "user.ContainerStatus", [[:never]]], type_of(klass.new(nil, wrap.call(["none"])))
    end
  end

  def test_fixed_integer_arm_alone_leaves_the_parameter_never_but_a_value_binds_it
    param = [:param, 0, "T"]
    klass = Class.new(Struct.new(:value, :status))
    status = [:status, union(param, [:literal, 1])]
    @registry.define(klass, "user.IntegerStatus", { "status" => status }, type_params: ["T"])
    assert_equal [:class, "user.IntegerStatus", [[:never]]], type_of(klass.new(nil, 1))
    @registry.define(klass, "user.IntegerStatus", { "status" => status, "value" => [:value, param] }, type_params: ["T"])
    assert_equal [:class, "user.IntegerStatus", [[:int]]], type_of(klass.new(1, 1))
    @registry.define(klass, "user.IntegerStatus", {
      "status" => [:status, union(param, [:param, 1, "U"]) ]
    }, type_params: %w[T U])
    assert_raises(Encode::MissingTypeArgs) { encode(klass.new(nil, 1)) }
  end

  def test_fixed_enum_variant_arms_do_not_learn_the_widened_enum_type
    param = [:param, 0, "T"]
    status = union(param, [:enum_variant, "user.Color", "red"])
    klass = Class.new(Struct.new(:value, :status))
    [[status, ->(value) { value }],
     [[:list, status], ->(value) { [value] }],
     [[:map, [:string], status], ->(value) { { "status" => value } }]].each do |slot, wrap|
      @registry.define(klass, "user.EnumStatus", {
        "value" => [:value, param], "status" => [:status, slot]
      }, type_params: ["T"])
      [[1, [:int]], [Color::Blue, [:enum, "user.Color"]]].each do |value, expected|
        result = encode(klass.new(value, wrap.call(Color::Red)))
        assert_equal [:class, "user.EnumStatus", [expected]], Encode::Slot.from_wire(result.value_type)
        bound = Baml.bind_types(klass.new(value, wrap.call(Color::Red)), expected)
        assert_equal encode(bound), result
      end
      assert_raises(Encode::TypeArgConflict) { encode(klass.new(1, wrap.call(Color::Blue))) }
    end
  end

  def test_equivalent_fixed_and_open_arms_keep_the_existing_wire_output
    param = [:param, 0, "T"]
    slot = [:list, union([:list, param], [:list, [:int]])]
    expected = [:list, union([:list, [:never]], [:list, [:int]])]
    result = encode([[1]], slot)
    assert_equal expected, Encode::Slot.from_wire(result.value_type)
    assert_equal [:list, [:int]], Encode::Slot.from_wire(result.list_value.values.first.value_type)
    assert_equal encode([[1]], expected), result
  end

  def test_empty_and_nonempty_fixed_containers_are_absorbed_in_lists_and_maps
    param = [:param, 0, "T"]
    [[[:list, [:int]], [], [1]],
     [[:map, [:string], [:int]], {}, { "n" => 1 }]].each do |fixed, empty, full|
      member = union(param, fixed)
      [[[:list, member], ->(items) { items }],
       [[:map, [:string], member], ->(items) { items.each_with_index.to_h { |item, i| [i.to_s, item] } }]].each do |slot, wrap|
        [[empty], [full], [empty, full]].each do |items|
          expected = Encode::Slot.substitute(slot, [[:never]])
          assert_equal expected, type_of(wrap.call(items), slot)
          assert_equal encode(wrap.call(items), expected), encode(wrap.call(items), slot)
        end
        [[empty, "a"], ["a", full]].each do |items|
          assert_equal Encode::Slot.substitute(slot, [[:string]]), type_of(wrap.call(items), slot)
        end
        assert_raises(Encode::UnboundTypeArgs) { encode(wrap.call([]), slot) }
      end
    end
  end

  def test_fixed_container_fields_do_not_conflict_with_other_parameter_evidence
    param = [:param, 0, "T"]
    klass = Class.new(Struct.new(:value, :status))
    [[[:list, [:int]], []], [[:map, [:string], [:int]], {}],
     [[:list, [:literal, "none"]], ["none"]],
     [[:map, [:string], [:literal, "none"]], { "status" => "none" }]].each do |fixed, status|
      fields = { "status" => [:status, union(param, fixed)] }
      @registry.define(klass, "user.FixedContainer", fields, type_params: ["T"])
      assert_equal [:class, "user.FixedContainer", [[:never]]], type_of(klass.new(nil, status))
      @registry.define(klass, "user.FixedContainer", fields.merge("value" => [:value, param]), type_params: ["T"])
      result = encode(klass.new(1, status))
      assert_equal [:class, "user.FixedContainer", [[:int]]], Encode::Slot.from_wire(result.value_type)
      assert_equal encode(Baml.bind_types(klass.new(1, status), Integer)), result
    end
  end

  def test_fixed_arm_choices_survive_recursive_container_learning
    param = [:param, 0, "T"]
    klass = Class.new(Struct.new(:value, :status))
    status = [:list, [:map, [:string], [:list, union(param, [:enum_variant, "user.Color", "red"])]]]
    @registry.define(klass, "user.NestedStatus", {
      "value" => [:value, param], "status" => [:status, status]
    }, type_params: ["T"])
    [[{ "red" => [Color::Red] }],
     [{ "red" => [Color::Red] }, { "int" => [2] }],
     [{ "int" => [2] }, { "red" => [Color::Red] }]].each do |items|
      result = encode(klass.new(1, items))
      assert_equal [:class, "user.NestedStatus", [[:int]]], Encode::Slot.from_wire(result.value_type)
      assert_equal encode(Baml.bind_types(klass.new(1, items), Integer)), result
    end
    @registry.define(klass, "user.NestedStatus", { "status" => [:status, status] }, type_params: ["T"])
    assert_equal [:class, "user.NestedStatus", [[:never]]], type_of(klass.new(nil, [{ "red" => [Color::Red] }]))
  end

  def test_fixed_arm_choices_survive_nested_class_argument_learning
    param = [:param, 0, "T"]
    inner = Class.new(Struct.new(:status))
    outer = Class.new(Struct.new(:value, :inner))
    @registry.define(inner, "user.InnerStatus", {
      "status" => [:status, union(param, [:enum_variant, "user.Color", "red"])]
    }, type_params: ["T"])
    @registry.define(outer, "user.OuterStatus", {
      "value" => [:value, param], "inner" => [:inner, [:class, "user.InnerStatus", [param]]]
    }, type_params: ["T"])
    result = encode(outer.new(1, inner.new(Color::Red)))
    assert_equal [:class, "user.OuterStatus", [[:int]]], Encode::Slot.from_wire(result.value_type)
    assert_equal encode(Baml.bind_types(outer.new(1, inner.new(Color::Red)), Integer)), result
    assert_raises(Encode::TypeArgConflict) { encode(outer.new(1, inner.new(Color::Blue))) }
    nested = [:class, "user.InnerStatus", [[:never]]]
    assert_equal [:class, "user.OuterStatus", [nested]], type_of(outer.new(inner.new(Color::Red), inner.new(Color::Red)))

    member = [:class, "user.InnerStatus", [param]]
    [[[:list, member], ->(items) { items }],
     [[:map, [:string], member], ->(items) { items.each_with_index.to_h { |item, i| [i.to_s, item] } }]].each do |slot, wrap|
      [ [Color::Red, Color::Blue], [Color::Blue, Color::Red] ].each do |colors|
        values = wrap.call(colors.map { |color| inner.new(color) })
        expected = Encode::Slot.substitute(slot, [[:enum, "user.Color"]])
        assert_equal expected, type_of(values, slot)
        assert_equal encode(values, expected), encode(values, slot)
      end
    end
  end

  def test_fixed_unions_in_nested_class_arguments_do_not_bind_the_parent_parameter
    param = [:param, 0, "T"]
    klass = Class.new(Struct.new(:value, :status))
    [[[:int], 1, "a", [:string]], [[:string], "s", 1, [:int]],
     [[:list, [:int]], [1], "a", [:string]],
     [[:map, [:string], [:int]], { "n" => 1 }, "a", [:string]]].each do |fixed, status, value, expected|
      field = [:status, [:class, "user.Box", [union(param, fixed)]]]
      fields = { "value" => [:value, param], "status" => field }
      [fields, fields.to_a.reverse.to_h].each do |ordered|
        @registry.define(klass, "user.NestedArgument", ordered, type_params: ["T"])
        result = encode(klass.new(value, Box.new(value: status)))
        assert_equal [:class, "user.NestedArgument", [expected]], Encode::Slot.from_wire(result.value_type)
        assert_equal encode(Baml.bind_types(klass.new(value, Box.new(value: status)), expected)), result
      end
      @registry.define(klass, "user.NestedArgument", { "status" => field }, type_params: ["T"])
      result = encode(klass.new(nil, Box.new(value: status)))
      assert_equal [:class, "user.NestedArgument", [[:never]]], Encode::Slot.from_wire(result.value_type)
      assert_equal encode(Baml.bind_types(klass.new(nil, Box.new(value: status)), [:never])), result
    end
  end

  def test_fixed_unions_nested_inside_class_argument_containers_are_absorbed
    param = [:param, 0, "T"]
    klass = Class.new(Struct.new(:value, :status))
    [[[:list, union(param, [:int])], [1]],
     [[:map, [:string], union(param, [:int])], { "n" => 1 }],
     [union([:list, param], [:list, [:int]]), [1]]].each do |argument, status|
      @registry.define(klass, "user.NestedArgumentContainer", {
        "value" => [:value, param], "status" => [:status, [:class, "user.Box", [argument]]]
      }, type_params: ["T"])
      result = encode(klass.new("a", Box.new(value: status)))
      assert_equal [:class, "user.NestedArgumentContainer", [[:string]]], Encode::Slot.from_wire(result.value_type)
      assert_equal encode(Baml.bind_types(klass.new("a", Box.new(value: status)), String)), result
      assert_raises(Encode::Refusal) { encode(klass.new("a", Box.new(value: true))) }
    end
  end

  def test_absorbed_evidence_beats_empty_list_and_map_siblings_in_either_order
    param = [:param, 0, "T"]
    red = [:enum_variant, "user.Color", "red"]
    [[[:list, [:list, union(param, red)]], [[], [Color::Red]], [[], [Color::Red], [1]]],
     [[:map, [:string], [:map, [:string], union(param, red)]],
      [{}, { "r" => Color::Red }], [{}, { "r" => Color::Red }, { "n" => 1 }]]].each do |slot, absorbed, concrete|
      wrap = ->(items) { slot[0] == :list ? items : items.each_with_index.to_h { |item, i| [i.to_s, item] } }
      [[absorbed, [:never]], [concrete, [:int]]].each do |items, binding|
        [items, items.reverse].each do |ordered|
          expected = Encode::Slot.substitute(slot, [binding])
          result = encode(wrap.call(ordered), slot)
          assert_equal expected, Encode::Slot.from_wire(result.value_type)
          assert_equal encode(wrap.call(ordered), expected), result
        end
      end
    end
  end

  def test_absorbed_evidence_beats_empty_class_fields_in_either_order
    param = [:param, 0, "T"]
    klass = Class.new(Struct.new(:a, :b))
    red = union(param, [:enum_variant, "user.Color", "red"])
    [[[:list, red], [], [Color::Red]],
     [[:map, [:string], red], {}, { "r" => Color::Red }]].each do |slot, empty, absorbed|
      fields = { "a" => [:a, slot], "b" => [:b, slot] }
      [fields, fields.to_a.reverse.to_h].each do |ordered|
        @registry.define(klass, "user.EmptyAndAbsorbed", ordered, type_params: ["T"])
        [[empty, absorbed], [absorbed, empty]].each do |values|
          result = encode(klass.new(*values))
          assert_equal [:class, "user.EmptyAndAbsorbed", [[:never]]], Encode::Slot.from_wire(result.value_type)
          assert_equal encode(Baml.bind_types(klass.new(*values), [:never])), result
        end
        assert_raises(Encode::MissingTypeArgs) { encode(klass.new(empty, empty)) }
      end
    end
  end

  def test_a_fixed_arm_does_not_determine_two_open_union_arms
    t, u = [:param, 0, "T"], [:param, 1, "U"]
    klass = Class.new(Struct.new(:status))
    @registry.define(klass, "user.TwoOpen", {
      "status" => [:status, union(t, u, [:enum_variant, "user.Color", "red"])]
    }, type_params: %w[T U])
    assert_raises(Encode::MissingTypeArgs) { encode(klass.new(Color::Red)) }
    assert_raises(Encode::UnboundTypeArgs) { encode([Color::Red], [:list, union(t, u, [:enum_variant, "user.Color", "red"])]) }
  end

  def test_fixed_arm_inference_shares_the_budget_and_scales_with_the_elements
    param = [:param, 0, "T"]
    slot = [:list, [:map, [:string], union(param, [:enum_variant, "user.Color", "red"])]]
    tree = Encode::Read.new(@registry).read(Array.new(2_000) { { "status" => Color::Red } }, "fixed")
    assert_raises(Encode::Fault) { Encode::Plan.new(@registry, budget: 100).plan_root(tree, slot) }
    planned = Timeout.timeout(5) { Encode::Plan.new(@registry, budget: 100_000).plan_root(tree, slot) }
    assert_equal 2_000, planned.children.length
    assert_equal Encode::Slot.substitute(slot, [[:never]]), planned.ty
  end

  def test_stored_bindings_match_normalized_decoded_bindings
    klass = Class.new(T::Struct) do
      include Baml::ValueEquality
      const :value, T.nilable(Integer)
    end
    fields = { "value" => [:value, [:param, 0, "T"]] }
    @registry.define(klass, "user.NormalizedBox", fields, type_params: ["T"])
    Protocol.define_class("user.NormalizedBox", klass, fields: fields, type_params: ["T"])
    [Baml::Types.optional(Baml::Types.optional(Integer)), union([:int], [:int], [:null], [:null])].each do |binding|
      original = Baml.bind_types(klass.new(value: 1), binding)
      encoded = encode(original)
      outbound = Wire::BamlOutboundValue.new(class_value: {
        name: "user.NormalizedBox", type_args: encoded.value_type.class_ty.type_args.to_a,
        fields: [{ key: "value", value: { int_value: 1 } }]
      })
      decoded = Protocol.decode_value(Wire::BamlOutboundValue.decode(Wire::BamlOutboundValue.encode(outbound)))
      assert_equal original, decoded
      assert original.eql?(decoded)
      assert_equal original.hash, decoded.hash
      assert_equal [union([:int], [:null])], Baml::Types.instance_args(original)
    end
  end

  def test_alias_and_inline_field_templates_infer_the_same_never_argument
    param = [:param, 0, "T"]
    @registry.define_alias("user.Name", [:string])
    [[:alias, "user.Name"], [:string]].each do |known|
      box = Class.new(Struct.new(:value))
      @registry.define(box, "user.AliasBox", { "value" => [:value, union(param, known)] }, type_params: ["T"])
      assert_equal [:class, "user.AliasBox", [[:never]]], type_of(box.new("hi"))
    end
  end

  def test_a_solved_element_slot_is_normalized_once_not_for_every_child
    param = [:param, 0, "T"]
    value = (1..30).map { |size| [Array.new(size, 1), ["a"]] }
    tree = Encode::Read.new(@registry).read(value, "solved")
    normalizations = Hash.new(0)
    original = Encode::Slot.method(:stated_form)
    Encode::Slot.stub(:stated_form, ->(slot, *args, **kwargs, &work) { normalizations[slot.object_id] += 1; original.call(slot, *args, **kwargs, &work) }) do
      planned = Encode::Plan.new(@registry).plan_root(tree, [:list, param])
      assert_equal 30, planned.children.length
    end
    assert_equal 1, normalizations.values.max
  end

  def test_a_known_sibling_taking_the_value_leaves_the_variable_never
    param = [:param, 0, "T"]
    slot = [:list, union(param, [:null])]
    opt = Class.new(Struct.new(:items))
    @registry.define(opt, "user.Opt", { "items" => [:items, slot] }, type_params: ["T"])
    assert_equal [:class, "user.Opt", [[:never]]], type_of(opt.new([nil]))
    [ [nil, 1], [1, nil] ].each do |items|
      result = encode(opt.new(items))
      assert_equal [:class, "user.Opt", [[:int]]], Encode::Slot.from_wire(result.value_type)
      assert_equal [:list, union([:int], [:null])], Encode::Slot.from_wire(result.class_value.fields.first.value.value_type)
    end
    assert_equal [:list, union([:never], [:null])], type_of([nil], slot)
    assert_raises(Encode::MissingTypeArgs) { encode(opt.new([])) }
    assert_equal [:list, union([:class, "user.Box", [[:int]]], [:null])], type_of(
      [Box.new(value: 1), nil], [:list, union([:class, "user.Box", [param]], [:null])]
    )
  end

  def test_a_handle_nested_under_an_open_variable_is_not_typed_by_a_sibling
    param = [:param, 0, "T"]
    slot = [:list, [:list, param]]
    api = Api.new
    handle = Baml::Handle.new(api, 41, :ADT_TAGGED_HEAP_HANDLE)
    scope = Baml::Bridge::EncodingScope.new
    [[[handle], [1]], [[1], [handle]]].each do |value|
      error = assert_raises(Encode::UnboundTypeArgs) { encode(value, slot, scope: scope) }
      assert_includes error.message, "could not infer a type"
    end
    assert_empty api.cloned
    assert_equal [:list, [:list, [:int]]], type_of([[1], [2]], slot)
    assert_equal [:list, [:list, [:int]]], type_of([[], [1]], slot)
    [ [[], param], [[], [:list, param]], [{}, [:map, [:string], param]] ].each do |value, open|
      assert_raises(Encode::UnboundTypeArgs) { encode(value, open) }
    end
    assert_equal [:map, [:string], [:int]], type_of({ "x" => 1 }, [:map, param, [:int]])
    boxes = [Baml.bind_types(Box.new(value: 1), Integer), Baml.bind_types(Box.new(value: "x"), String)]
    assert_raises(Encode::UnboundTypeArgs) { encode(boxes, [:list, [:class, "user.Box", [param]]]) }
    assert_equal 41, encode(handle, param, scope: scope).handle.key
  ensure
    scope&.close
    handle&.close
  end

  def test_arguments_are_keyed_by_declared_name_and_type_args_substitute
    param = [:param, 0, "T"]
    Protocol.define_function("user.bound_args", params: [["a", [:int]], ["p", [:list, param]]], type_params: ["T"])
    types = Protocol.prepare_type_args("user.bound_args", {}, { T: String })
    result = Wire::CallFunctionArgs.decode(Protocol.encode_call(1, "user.bound_args", { "p" => ["s"], "a" => 1 }, type_args: types))
    assert_equal %w[p a], result.kwargs.map(&:string_key)
    assert_equal [:list, [:string]], Encode::Slot.from_wire(result.kwargs.first.value.value_type)
    error = assert_raises(TypeError) { Protocol.encode_call(1, "user.bound_args", { "p" => [1] }, type_args: types) }
    assert_includes error.message, "user.bound_args: p[0]"
    [{ Wrong: Integer }, { "T" => Integer }, []].each do |types|
      assert_raises(TypeError) { Protocol.prepare_type_args("user.bound_args", {}, types) }
    end

    Protocol.define_class("user.Receiver", Box, fields: { "value" => [:value, param], "note" => [:note, [:null]] }, type_params: ["T"])
    Protocol.define_function("user.Receiver.method", params: [["self", [:class, "user.Receiver", [param]]], ["p", [:list, param]],
      ["other", [:param, 1, "U"]]], class_type_params: ["T"], type_params: ["U"])
    receiver = Baml.bind_types(Box.new(value: 1), Float)
    arguments = { "self" => receiver, "p" => [2], "other" => "s" }
    types = Protocol.prepare_type_args("user.Receiver.method", arguments, { U: String })
    assert_equal %w[T U], types.map(&:type_var)
    result = Wire::CallFunctionArgs.decode(Protocol.encode_call(1, "user.Receiver.method", arguments, type_args: types))
    assert_equal 1.0, result.kwargs.first.value.class_value.fields.first.value.float_value
    assert_equal [:list, [:float]], Encode::Slot.from_wire(result.kwargs[1].value.value_type)
    signature = [:function, [["x", param, false]], [:list, param], [:param, 1, "U"]]
    assert_equal [:function, [["x", [:float], false]], [:list, [:float]], [:string]],
      Encode::Slot.substitute(signature, { "T" => [:float], "U" => [:string] }, indexed: false)
  end

  def test_type_tokens_and_instance_arguments_are_immutable_descriptors
    type = Baml::Types.generic(Box, Baml::Types.optional(Baml::Types.list(Integer)))
    assert_equal [:class, "user.Box", [[:union, [[:list, [:int]], [:null]]]]], type
    assert type.frozen?
    assert type[2].first[1].first[1].frozen?
    assert_raises(TypeError) { Baml::Types.generic(Box) }
    assert_raises(TypeError) { Baml::Types.token(Box) }
    value = Box.new(value: [])
    assert_same value, Baml.bind_types(value, type)
    assert_equal [type], Baml::Types.instance_args(value)
    assert Baml::Types.instance_args(value).frozen?
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

  def test_outbound_map_keys_keep_their_existing_decode_types
    [["7", [:int], 7], ["18446744073709551616", [:bigint], 2**64],
     ["true", [:bool], true], ["false", [:literal, false], false],
     ["42", [:literal, :bigint, 42], 42], ["text", [:literal, "text"], "text"]].each do |key, ty, expected|
      assert_equal expected, Protocol.decode_map_key(key, Encode::Slot.to_wire(ty))
    end
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
    @registry.define(left, "user.Left", { "child" => [:child, union([:null], c, d)] }, type_params: ["T"])
    @registry.define(right, "user.Right", { "child" => [:child, union([:null], a, b)] }, type_params: ["T"])
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
    @registry.define(type, "user.ReadOnce", { "value" => [:value, [:param, 0, "T"]] }, type_params: ["T"])
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

  def test_unset_arguments_are_omitted_before_reading_and_nil_is_explicit
    Protocol.define_function("user.encode_defaults", params: [["omitted", [:int]], ["explicit", [:union, [[:int], [:null]]]]])
    args = Wire::CallFunctionArgs.decode(Protocol.encode_call(1, "user.encode_defaults", {
      "omitted" => Baml::UNSET, "explicit" => nil, "undeclared" => Baml::UNSET
    }))
    assert_equal ["explicit"], args.kwargs.map(&:string_key)
    assert_nil args.kwargs.first.value.value
  end

  def test_invocation_cancel_uses_the_walker_and_keeps_the_class_check
    token_type = Class.new(Struct.new(:handle))
    Protocol.define_class("baml.spawn.CancelToken", token_type, fields: { "handle" => [:handle, [:other]] })
    api = Api.new
    handle = Baml::Handle.new(api, 41, :ADT_TAGGED_HEAP_HANDLE)
    scope = Baml::Bridge::EncodingScope.new
    invocation = Protocol.encode_invocation(1, Baml::Options.new(cancel: token_type.new(handle)), api, scope: scope)
    assert_equal [:class, "baml.spawn.CancelToken", []], Encode::Slot.from_wire(invocation.cancel.value_type)
    assert_equal 41, invocation.cancel.class_value.fields.first.value.handle.key
    assert_equal [41], api.cloned
    error = assert_raises(TypeError) do
      Protocol.encode_invocation(1, Baml::Options.new(cancel: handle), api, scope: scope)
    end
    assert_includes error.message, "cancel must be a generated baml.spawn.CancelToken"
    error = assert_raises(TypeError) do
      Protocol.encode_invocation(1, Baml::Options.new(cancel: token_type.new(Object.new)), api, scope: scope)
    end
    assert_includes error.message, "_baml.cancel.handle"
    assert_equal [41], api.cloned
    scope.close
    assert_equal [41], api.released
  ensure
    scope&.close
    handle&.close
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
    return_type, parameter_types, options = native::FUNCTION_SIGNATURES.fetch(:complete_host_call)
    function = FFI::Function.new(return_type, parameter_types, table[:complete_host_call], **(options || {}))
    api.instance_variable_set(:@functions, { complete_host_call: function })
    completion = Wire::HostCallCompletion.encode(Wire::HostCallCompletion.new(returned: { int_value: 5 }))
    api.complete_host_call(7, completion)
    api.complete_host_call(8, "".b)
    assert_equal [[7, completion], [8, "".b]], received
  end

  def test_type_descriptors_round_trip_and_refuse_nonstatic_heads
    slots = [[:null], [:never], [:unknown], [:enum, "user.Color"], [:interface, "user.I"],
             [:bigint], [:bytes], [:media, :image], [:media, :generic], [:literal, :bigint, 42],
             [:enum_variant, "user.Color", "red"], [:alias, "user.Rec"],
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
    plan = Encode::Plan.new(@registry).plan_root(tree, slot)
    Encode::Commit.check_depth!(Encode::Commit.depth(plan), "argument")
    result = Encode::Commit.commit(plan, scope: scope)
    assert_equal Encode::Commit.depth(plan), Encode::Slot.wire_depth(result)
    result
  end

  def type_of(value, slot = [:unknown])
    Encode::Slot.from_wire(encode(value, slot).value_type)
  end
end
