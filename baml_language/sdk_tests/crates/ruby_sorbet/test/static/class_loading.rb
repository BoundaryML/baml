# typed: strict
require "sorbet-runtime"
require "baml_sdk"

mood = T.let(BamlSdk::Zeta::Mood::Happy, BamlSdk::Zeta::Mood)
node = BamlSdk::Alpha::Node.new(node_label: "root", mood: mood, children: [], links: {})
link = BamlSdk::Zeta::Link.new(node: node, weight: 1)
child = BamlSdk::Alpha::Node.new(
  node_label: "child", next_node: node, link: link, mood: mood, children: [node], links: {"root" => link}
)
T.let(child.node_label, String)
T.let(child.next_node, T.nilable(BamlSdk::Alpha::Node))
T.let(child.link, T.nilable(BamlSdk::Zeta::Link))
T.let(child.mood, BamlSdk::Zeta::Mood)
T.let(child.children, T::Array[BamlSdk::Alpha::Node])
T.let(child.links, T::Hash[String, BamlSdk::Zeta::Link])
T.let(link.node, T.nilable(BamlSdk::Alpha::Node))
T.let(link.weight, Integer)
