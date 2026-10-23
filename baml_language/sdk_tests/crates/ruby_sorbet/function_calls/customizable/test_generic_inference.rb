# frozen_string_literal: true

require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class GenericInferenceTest < Minitest::Test
  G = BamlSdk::GenericTests
  Types = Baml::Types

  def bound(klass, *types, **fields)
    Baml.bind_types(klass.new(**fields), *types)
  end

  def assert_type_error(*needles)
    error = assert_raises(TypeError) { yield }
    needles.each { |needle| assert_includes error.message, needle }
  end

  def test_generic_inference_identity_infers_primitives
    assert_equal 5, G.identity(5)
    assert_equal "hi", G.identity("hi")
    assert_same true, G.identity(true)
  end

  def test_generic_inference_identity_infers_user_class
    pair = G::StringIntPair.new(my_string: "a", my_int: 1)
    assert_equal pair, G.identity(pair)
  end

  def test_generic_inference_identity_infers_generic_instance
    box = bound(G::GenericBox, Integer, value: 5)
    assert_equal box, G.identity(box)
    nested = bound(G::GenericBox, Types.generic(G::GenericBox, String), value: bound(G::GenericBox, String, value: "hello"))
    assert_equal nested, G.identity(nested)
  end

  def test_generic_inference_identity_async_infers
    assert_equal 7, G.identity_async(7).wait
  end

  def test_generic_inference_identity_null_round_trips
    assert_nil G.identity(nil)
  end

  def test_generic_inference_identity_unbound_generic_instance_round_trips
    unbound = G::GenericBox.new(value: 5)
    assert_equal unbound, G.identity(unbound)
  end

  def test_generic_inference_make_triple_infers_multiple_typevars
    triple = G.make_triple(1, ["a", "b"], { "k" => true })
    assert_instance_of G::GenericTriple, triple
    assert_equal 1, triple.first
    assert_equal ["a", "b"], triple.second
    assert_equal({ "k" => true }, triple.third)
  end

  def test_generic_inference_second_of_infers_from_nested_generic
    assert_equal "hi", G.second_of(bound(G::GenericPair, Integer, String, first: 1, second: "hi"))
    pair = G::StringIntPair.new(my_string: "z", my_int: 9)
    assert_equal pair, G.second_of(bound(G::GenericPair, Integer, G::StringIntPair, first: 0, second: pair))
  end

  def test_generic_inference_read_items_infers_from_instance_wire_args
    container = bound(G::ContainerShapes, Integer, item: 1, items: [1, 2, 3], by_key: { "k" => 4 }, maybe: nil, mixed: nil)
    assert_equal [1, 2, 3], G.read_items(container)
    empty_fields = bound(G::ContainerShapes, Integer, item: 1, items: [], by_key: {}, maybe: nil, mixed: nil)
    assert_equal [], G.read_items(empty_fields)
  end

  def test_generic_inference_list_head_infers_from_recursive_generic
    linked = bound(G::GenericRecursive, Integer, value: 7, next_: bound(G::GenericRecursive, Integer, value: 8, next_: nil))
    assert_equal 7, G.list_head(linked)
  end

  def test_generic_inference_extract_infers_four_typevars_from_nesting
    pair = bound(G::GenericPair, Types.generic(G::GenericPair, Integer, String), Types.generic(G::GenericPair, T::Boolean, Float),
                 first: bound(G::GenericPair, Integer, String, first: 1, second: "a"),
                 second: bound(G::GenericPair, T::Boolean, Float, first: true, second: 1.5))
    assert_equal "int | string | bool | float", G.extract(pair)
  end

  def test_generic_inference_choose_infers_unified_typevar
    assert_equal 5, G.choose(5, 6)
    assert_equal "a", G.choose("a", "b")
  end

  def test_generic_inference_choose_infers_divergent_union
    assert_equal 5, G.choose(5, "asdf")
  end

  def test_generic_inference_make_triple_partial_explicit_then_infer
    triple = G.make_triple(1, ["x", "y"], { "k" => true }, _types: { A: Integer })
    assert_instance_of G::GenericTriple, triple
    assert_equal 1, triple.first
    assert_equal ["x", "y"], triple.second
    assert_equal({ "k" => true }, triple.third)
  end

  def test_generic_inference_wrap_infers_and_returns_generic
    box = G.wrap(5)
    assert_instance_of G::GenericBox, box
    assert_equal 5, box.value
  end

  def test_generic_inference_genericbox_pair_with_infers_method_typevar
    assert_equal "int | string", bound(G::GenericBox, Integer, value: 5).pair_with("hello world")
  end

  def test_generic_inference_generic_static_infers_own_typevar
    box = G::GenericBox.baml_new(5)
    assert_instance_of G::GenericBox, box
    assert_equal 5, box.value
  end

  def test_generic_inference_named_static_infers_distinct_typevars
    assert_equal "int | string", G::NamedStatic.make(1, "x")
  end

  def test_generic_inference_union_with_concrete_sibling_infers_typevar
    assert_equal "int", G.tag_or_value(5)
  end

  def test_generic_inference_union_concrete_sibling_absorbs_value_binds_rust_type
    assert_equal "$rust_type", G.tag_or_value("hi")
  end

  def test_generic_inference_union_null_actual_binds_rust_type
    assert_equal "$rust_type", G.tag_or_value(nil)
  end

  def test_generic_inference_return_only_var_still_requires_binding
    assert_type_error("could not infer a type", "parse_as") { G.parse_as("42") }
  end

  def test_generic_inference_body_only_var_still_requires_binding
    assert_type_error("could not infer a type", "one_type_arg") { G.one_type_arg }
  end

  def test_generic_inference_pair_invariant_list_conflict_rejects
    assert_type_error("can't be reconciled", "pair", "int", "string") { G.pair([1, 2], ["a", "b"]) }
  end

  def test_generic_inference_pair_invariant_list_agree_binds
    assert_equal "int", G.pair([1, 2], [3, 4])
  end

  def test_generic_inference_choose_union_outside_container_is_sound
    assert_equal [1, 2], G.choose([1, 2], ["a"])
  end

  def test_generic_inference_merge_invariant_map_value_conflict_rejects
    assert_type_error("can't be reconciled", "merge") { G.merge({ "k" => 1 }, { "k" => "a" }) }
  end

  def test_generic_inference_combine_invariant_class_arg_conflict_rejects
    assert_type_error("can't be reconciled", "combine") do
      G.combine(bound(G::GenericBox, Integer, value: 1), bound(G::GenericBox, String, value: "x"))
    end
  end

  def test_generic_inference_glue_invariant_vs_covariant_conflict_rejects
    assert_type_error("can't be reconciled", "glue") { G.glue(1, ["a"]) }
  end

  def test_generic_inference_glue_invariant_and_covariant_agree_binds
    assert_equal "int", G.glue(1, [2, 3])
  end

  def test_generic_inference_two_typevar_union_is_uninferrable_rejects
    assert_type_error("two_in_union") { G.two_in_union("hello") }
  end

  def test_generic_inference_triple_choose_three_covariant_join
    assert_equal "int | string | bool", G.triple_choose(5, "asdf", true)
  end

  def test_generic_inference_make_triple_heterogeneous_list_element_unions
    triple = G.make_triple(1, [1, "x"], { "k" => true })
    assert_instance_of G::GenericTriple, triple
    assert_equal [1, "x"], triple.second
  end

  def test_generic_inference_choose_divergent_generic_instances_union
    left = bound(G::GenericBox, Integer, value: 1)
    assert_equal left, G.choose(left, bound(G::GenericBox, String, value: "x"))
  end

  def test_generic_inference_tag_or_value_binds_generic_instance
    rendered = G.tag_or_value(bound(G::GenericBox, String, value: "asdf"))
    assert_includes rendered, "GenericBox"
    assert_includes rendered, "string"
  end

  def test_generic_inference_first_or_empty_list_round_trips_none
    assert_nil G.first_or([])
  end

  def test_generic_inference_first_or_nonempty_infers_element
    assert_equal 7, G.first_or([7, 8, 9])
  end

  def test_generic_inference_values_of_empty_map_round_trips_empty_list
    assert_equal [], G.values_of({})
  end

  def test_generic_inference_values_of_nonempty_returns_values
    assert_equal [1, 2], G.values_of({ "a" => 1, "b" => 2 })
  end

  # SDK_PARITY_LINT(skip): Ruby uses named _types hashes and has no callable-subscript arity surface
  def test_generic_inference_make_triple_partial_subscript_requires_full_arity
    skip "Ruby has no callable-subscript arity surface"
  end

  def test_generic_inference_make_triple_subscript_fully_bound
    triple = G.make_triple(5, ["x"], { "k" => true }, _types: { A: Integer, B: String, C: T::Boolean })
    assert_instance_of G::GenericTriple, triple
    assert_equal 5, triple.first
    assert_equal ["x"], triple.second
    assert_equal({ "k" => true }, triple.third)
  end

  def test_generic_inference_one_type_arg_explicit_types_succeeds
    assert_equal "int", G.one_type_arg(_types: { T: Integer })
  end

  def test_generic_inference_parse_as_explicit_types_succeeds
    assert_equal 42, G.parse_as("42", _types: { T: Integer })
  end

  def test_generic_inference_second_of_unbound_instance_recovers_field_type
    assert_equal "hi", G.second_of(G::GenericPair.new(first: 1, second: "hi"))
  end

  def test_generic_inference_identity_nested_unbound_round_trips
    nested = G::GenericBox.new(value: G::GenericBox.new(value: "hello"))
    assert_equal nested, G.identity(nested)
  end

  def test_generic_inference_wrap_infers_and_returns_bound_generic
    assert_equal bound(G::GenericBox, Integer, value: 5), G.wrap(5)
  end

  def test_generic_inference_maybe_id_present_value_infers
    assert_equal 5, G.maybe_id(5)
  end

  def test_generic_inference_maybe_id_null_round_trips
    assert_nil G.maybe_id(nil)
  end

  def test_generic_inference_identity_enum_round_trips
    result = G.identity(G::SomeEnum::VARIANT)
    assert_same G::SomeEnum::VARIANT, result
    assert_instance_of G::SomeEnum, result
  end

  def test_generic_inference_host_only_object_not_encodable_from_python
    error = assert_raises(TypeError) { G.identity(Object.new) }
    assert_includes error.message, "Cannot encode"
  end

  def test_generic_inference_genericbox_get_infers_class_var_from_receiver
    assert_equal "int", bound(G::GenericBox, Integer, value: 5).get
  end

  def test_generic_inference_genericbox_pair_with_unbound_receiver_recovers_class_var
    assert_equal "int | string", G::GenericBox.new(value: 5).pair_with("x")
  end

  def test_generic_inference_make_triple_types_kwarg_contradicted_by_actual_rejects
    assert_type_error("make_triple") { G.make_triple("nope", ["x"], { "k" => true }, _types: { A: Integer }) }
  end

  def test_generic_inference_make_triple_full_subscript_contradicted_by_actual_rejects
    assert_type_error("make_triple") { G.make_triple("nope", ["x"], { "k" => true }, _types: { A: Integer, B: String, C: T::Boolean }) }
  end

  def test_generic_inference_elem_type_heterogeneous_array_unifies
    assert_equal "int | string", G.elem_type([1, "x"])
  end

  def test_generic_inference_elem_type_homogeneous_array_is_single_type
    assert_equal "int", G.elem_type([1, 2, 3])
  end

  def test_generic_inference_elem_type_three_way_heterogeneous_array_unifies
    rendered = G.elem_type([1, "x", true])
    %w[int string bool].each { |type| assert_includes rendered, type }
  end

  def test_generic_inference_read_items_unbound_container_recovers_t_from_fields
    unbound = G::ContainerShapes.new(item: 1, items: [1, 2, 3], by_key: { "k" => 4 }, maybe: nil, mixed: nil)
    assert_equal [1, 2, 3], G.read_items(unbound)
  end

  def test_generic_inference_list_head_unbound_recursive_recovers_t_from_fields
    unbound = G::GenericRecursive.new(value: 7, next_: G::GenericRecursive.new(value: 8, next_: nil))
    assert_equal 7, G.list_head(unbound)
  end

  def test_generic_inference_extract_fully_unbound_nested_pair_recovers_all_vars
    pair = G::GenericPair.new(first: G::GenericPair.new(first: 1, second: "a"), second: G::GenericPair.new(first: true, second: 1.5))
    assert_equal "int | string | bool | float", G.extract(pair)
  end

  def test_generic_inference_triple_choose_join_includes_concrete_class
    rendered = G.triple_choose(5, G::StringIntPair.new(my_string: "a", my_int: 1), "x")
    %w[int StringIntPair string].each { |type| assert_includes rendered, type }
  end

  def test_generic_inference_triple_choose_join_includes_enum_variant
    rendered = G.triple_choose(5, G::SomeEnum::VARIANT, G::StringIntPair.new(my_string: "a", my_int: 1))
    %w[int SomeEnum StringIntPair].each { |type| assert_includes rendered, type }
  end
end
