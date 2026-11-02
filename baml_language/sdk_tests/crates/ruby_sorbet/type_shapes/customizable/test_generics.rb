# frozen_string_literal: true

require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class GenericsTest < Minitest::Test
  G = BamlSdk::Generics

  def test_generic_generic
    wrapper = G.make_wrapper_methods("hello")
    assert_equal "hello", wrapper.get_value_or_marker
  end

  def test_generic_generic_wrapper_get_value
    wrapper = G.make_wrapper_methods("hello")
    assert_equal "hello", wrapper.get_value
  end

  # Bind expected values so equality checks their concrete BAML arguments too.
  def test_generics_round_trip_wrapper_int
    wrapper = Baml.bind_types(G::Wrapper[Integer].new(value: 5), Integer)
    assert_equal wrapper, G.round_trip_wrapper_int(wrapper)
  end

  def test_generics_round_trip_generic_linked_list_int
    tail = Baml.bind_types(G::GenericLinkedList[Integer].new(value: 2, next_: nil), Integer)
    linked = Baml.bind_types(G::GenericLinkedList[Integer].new(value: 1, next_: tail), Integer)
    assert_equal linked, G.round_trip_generic_linked_list_int(linked)
  end

  def test_generics_round_trip_generic_binary_tree_int
    tree = Baml.bind_types(G::GenericBinaryTree[Integer].new(value: 1, left: nil, right: nil), Integer)
    assert_equal tree, G.round_trip_generic_binary_tree_int(tree)
  end

  def test_generics_round_trip_box_int
    wrapped = Baml.bind_types(G::Wrapper[Integer].new(value: 4), Integer)
    box = Baml.bind_types(G::Box[Integer].new(value: 3, wrapped: wrapped), Integer)
    assert_equal box, G.round_trip_box_int(box)
  end

  def test_generics_round_trip_nested_generics
    wrapped = Baml.bind_types(G::Wrapper[Integer].new(value: 1), Integer)
    linked = Baml.bind_types(G::GenericLinkedList[Integer].new(value: 9, next_: nil), Integer)
    nested = G::NestedGenerics.new(
      ww: Baml.bind_types(G::Wrapper[G::Wrapper[Integer]].new(value: wrapped), Baml::Types.generic(G::Wrapper, Integer)),
      wl: Baml.bind_types(G::Wrapper[T::Array[Integer]].new(value: [1, 2]), Baml::Types.list(Integer)),
      wr: Baml.bind_types(G::Wrapper[G::GenericLinkedList[Integer]].new(value: linked), Baml::Types.generic(G::GenericLinkedList, Integer))
    )
    assert_equal nested, G.round_trip_nested_generics(nested)
  end

  def test_generics_round_trip_differing_instantiation
    wrapped = Baml.bind_types(G::Wrapper[Integer].new(value: 1), Integer)
    linked = Baml.bind_types(G::GenericLinkedList[G::Wrapper[Integer]].new(value: wrapped, next_: nil), Baml::Types.generic(G::Wrapper, Integer))
    different = G::DifferingInstantiation.new(list: linked)
    assert_equal different, G.round_trip_differing_instantiation(different)
  end
end
