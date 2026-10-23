# frozen_string_literal: true

require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class GenericCallsTest < Minitest::Test
  G = BamlSdk::GenericTests
  Types = Baml::Types

  def bound(klass, *types, **fields)
    Baml.bind_types(klass.new(**fields), *types)
  end

  def test_generic_calls_identity_explicit
    assert_equal 5, G.identity(5, _types: { T: Integer })
    assert_equal "hi", G.identity("hi", _types: { T: String })
    pair = G::StringIntPair.new(my_string: "a", my_int: 1)
    assert_equal pair, G.identity(pair, _types: { T: G::StringIntPair })
    box = bound(G::GenericBox, Types.generic(G::GenericBox, String), value: bound(G::GenericBox, String, value: "hello"))
    assert_equal box, G.identity(box, _types: { T: Types.generic(G::GenericBox, Types.generic(G::GenericBox, String)) })
    triple = bound(G::GenericTriple, Types.generic(G::GenericBox, String), Float, T::Boolean,
                   first: bound(G::GenericBox, String, value: "hello"), second: [1.1, 2.2], third: { "lorem" => true, "ipsum" => false })
    assert_equal triple, G.identity(triple, _types: { T: Types.generic(G::GenericTriple, Types.generic(G::GenericBox, String), Float, T::Boolean) })
  end

  def test_generic_calls_identity_async_explicit
    assert_equal 7, G.identity_async(7, _types: { T: Integer }).wait
  end

  def test_generic_calls_tag_or_value_explicit
    assert_equal "int", G.tag_or_value(5, _types: { T: Integer })
    assert_equal "string", G.tag_or_value("plain", _types: { T: String })
    pair = G::StringIntPair.new(my_string: "b", my_int: 2)
    assert_includes G.tag_or_value(pair, _types: { T: G::StringIntPair }), "StringIntPair"
  end

  def test_generic_calls_make_triple_explicit
    triple = G.make_triple(1, ["a", "b"], { "k" => true }, _types: { A: Integer, B: String, C: T::Boolean })
    assert_instance_of G::GenericTriple, triple
    assert_equal 1, triple.first
    assert_equal ["a", "b"], triple.second
    assert_equal({ "k" => true }, triple.third)
  end

  def test_generic_calls_one_type_arg_explicit
    assert_equal "int", G.one_type_arg(_types: { T: Integer })
    assert_equal "string", G.one_type_arg(_types: { T: String })
    nested = G.one_type_arg(_types: { T: Types.generic(G::GenericBox, Integer) })
    assert_includes nested, "GenericBox"
    assert_includes nested, "int"
  end

  def test_generic_calls_two_type_args_explicit
    assert_equal "int | string", G.two_type_args(_types: { A: Integer, B: String })
  end

  def test_generic_calls_generic_free_fn_requires_binding
    error = assert_raises(TypeError) { G.one_type_arg }
    assert_includes error.message, "could not infer a type"
    assert_includes error.message, "one_type_arg"
    error = assert_raises(TypeError) { G.two_type_args }
    assert_includes error.message, "could not infer a type"
  end

  def test_generic_calls_subscript_wrong_arity_raises
    # Like Java, named bindings permit partial arity; body-only B cannot infer.
    assert_raises(TypeError) { G.two_type_args(_types: { A: Integer }) }
  end

  def test_generic_calls_consume_int_wrapper_baseline
    assert_equal 9, G.consume_int_wrapper(bound(G::GenericBox, Integer, value: 9))
  end

  def test_generic_calls_genericbox_get_explicit
    assert_equal "int", bound(G::GenericBox, Integer, value: 5).get
  end

  def test_generic_calls_genericbox_pair_with_explicit
    box = bound(G::GenericBox, Integer, value: 5)
    assert_equal "int | string", box.pair_with("hello world", _types: { U: String })
  end

  def test_generic_calls_genericbox_new_static_explicit
    box = G::GenericBox.baml_new(5, _types: { V: Integer })
    assert_instance_of G::GenericBox, box
    assert_equal 5, box.value
  end

  def test_generic_calls_generic_static_infers_binding
    box = G::GenericBox.baml_new(5)
    assert_instance_of G::GenericBox, box
    assert_equal 5, box.value
  end

  def test_generic_calls_named_static_distinct_typevar_names
    assert_equal "int | string", G::NamedStatic.make(1, "x", _types: { D: Integer, E: String })
  end

  def test_generic_calls_instance_method_unparameterized_receiver_raises
    error = assert_raises(TypeError) { G::GenericBox.new(value: 5).pair_with("x", _types: { U: String }) }
    assert_includes error.message, "receiver"
  end

  def test_generic_calls_extract_explicit
    pair = bound(G::GenericPair, Types.generic(G::GenericPair, Integer, String), Types.generic(G::GenericPair, T::Boolean, Float),
                 first: bound(G::GenericPair, Integer, String, first: 1, second: "a"),
                 second: bound(G::GenericPair, T::Boolean, Float, first: true, second: 1.5))
    assert_equal "int | string | bool | float", G.extract(pair, _types: { A: Integer, B: String, C: T::Boolean, D: Float })
  end

  def test_generic_calls_parse_as_explicit
    assert_equal G::StringIntPair.new(my_string: "x", my_int: 3), G.parse_as('{"my_string": "x", "my_int": 3}', _types: { T: G::StringIntPair })
    assert_equal 42, G.parse_as("42", _types: { T: Integer })
  end

  def test_generic_calls_second_of_explicit
    assert_equal "hi", G.second_of(bound(G::GenericPair, Integer, String, first: 1, second: "hi"), _types: { T: String })
    pair = G::StringIntPair.new(my_string: "z", my_int: 9)
    assert_equal pair, G.second_of(bound(G::GenericPair, Integer, G::StringIntPair, first: 0, second: pair), _types: { T: G::StringIntPair })
  end

  def test_generic_calls_list_head_explicit
    linked = bound(G::GenericRecursive, Integer, value: 7, next_: bound(G::GenericRecursive, Integer, value: 8, next_: nil))
    assert_equal 7, G.list_head(linked, _types: { T: Integer })
  end

  def test_generic_calls_choose_explicit
    assert_equal 1, G.choose(1, 2, _types: { T: Integer })
    assert_equal "a", G.choose("a", "b", _types: { T: String })
  end

  def test_generic_calls_read_items_explicit
    container = bound(G::ContainerShapes, Integer, item: 1, items: [1, 2, 3], by_key: { "k" => 4 }, maybe: nil, mixed: nil)
    assert_equal [1, 2, 3], G.read_items(container, _types: { T: Integer })
  end

  def test_generic_calls_wrap_explicit
    box = G.wrap(5, _types: { T: Integer })
    assert_instance_of G::GenericBox, box
    assert_equal 5, box.value
  end

  def test_generic_calls_make_int_box_reified
    box = G.make_int_box
    assert_instance_of G::GenericBox, box
    assert_equal [Types.token(Integer)], Types.instance_args(box)
    assert_equal 7, box.value
  end

  def test_generic_calls_make_int_container_reified
    container = G.make_int_container
    assert_instance_of G::ContainerShapes, container
    assert_equal [Types.token(Integer)], Types.instance_args(container)
    assert_equal 1, container.item
    assert_equal [1, 2, 3], container.items
    assert_equal({ "k" => 4 }, container.by_key)
    assert_nil container.maybe
    assert_equal 5, container.mixed
  end

  def test_generic_calls_make_nested_box_reified
    outer = G.make_nested_box
    assert_instance_of G::GenericBox, outer
    assert_equal [Types.generic(G::GenericBox, Integer)], Types.instance_args(outer)
    assert_instance_of G::GenericBox, outer.value
    assert_equal [Types.token(Integer)], Types.instance_args(outer.value)
    assert_equal 9, outer.value.value
  end

  def test_generic_calls_make_int_str_bool_triple_reified
    triple = G.make_int_str_bool_triple
    assert_instance_of G::GenericTriple, triple
    assert_equal [Types.token(Integer), Types.token(String), Types.token(T::Boolean)], Types.instance_args(triple)
    assert_equal 1, triple.first
    assert_equal ["a", "b"], triple.second
    assert_equal({ "k" => true }, triple.third)
  end
end
