# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"

class ClassLoadingTest < Minitest::Test
  # SDK_PARITY_LINT(skip): Ruby generator forward, recursive and enum loading regression
  def test_recursive_classes_and_later_enum_keep_runtime_types
    mood = BamlSdk::Zeta::Mood::Happy
    node = BamlSdk::Alpha::Node.new(node_label: "root", mood: mood, children: [], links: {})
    link = BamlSdk::Zeta::Link.new(node: node, weight: 1)
    child = BamlSdk::Alpha::Node.new(
      node_label: "child", next_node: node, link: link, mood: mood, children: [node], links: {"root" => link}
    )
    assert_same node, child.next_node
    assert_same node, child.children.fetch(0)
    assert_same link, child.link
    assert_same link, child.links.fetch("root")
    assert_same node, link.node
    assert_same mood, child.mood
    assert_equal "happy", mood.serialize
    assert_nil node.next_node
    assert_nil node.link
    assert_raises(TypeError) { BamlSdk::Zeta::Link.new(node: "not a Node", weight: 1) }
    assert_raises(TypeError) { BamlSdk::Zeta::Link.new(weight: "not an Integer") }
    assert_raises(TypeError) { BamlSdk::Alpha::Node.new(node_label: "bad", mood: "happy", children: [], links: {}) }

    # Registration must refer to the same class objects whose fields were added.
    protocol = Baml::Bridge.const_get(:Protocol)
    assert_equal %w[user.alpha.Node user.zeta.Link user.zeta.Mood], protocol.instance_variable_get(:@types).keys.sort
    assert_same BamlSdk::Alpha::Node, protocol.registered_type("user.alpha.Node")
    assert_same BamlSdk::Zeta::Link, protocol.registered_type("user.zeta.Link")
    assert_same BamlSdk::Zeta::Mood, protocol.registered_type("user.zeta.Mood")
    assert_equal({"nodeLabel" => :node_label, "nextNode" => :next_node, "link" => :link,
                  "mood" => :mood, "children" => :children, "links" => :links},
                 protocol.instance_variable_get(:@fields).fetch(BamlSdk::Alpha::Node))
    assert_equal({"node" => :node, "weight" => :weight}, protocol.instance_variable_get(:@fields).fetch(BamlSdk::Zeta::Link))
    assert_nil protocol.instance_variable_get(:@fields).fetch(BamlSdk::Zeta::Mood)
  end
end
