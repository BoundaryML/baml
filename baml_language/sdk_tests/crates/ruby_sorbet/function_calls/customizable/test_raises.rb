# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"

class RaisesTest < Minitest::Test
  def doc_for(marker)
    source = File.read(File.join(__dir__, "baml_sdk/raises_test.rb"))
    assert_includes source, marker
    lines = source.split(marker, 2).first.lines
    lines.pop while lines.last && (lines.last.strip.empty? || lines.last.lstrip.start_with?("sig "))
    lines.reverse.take_while { |line| line.lstrip.start_with?("#") }
      .reverse.map { |line| line.sub(/^\s*# ?/, "").rstrip }.join("\n")
  end

  def test_raises_imports_symbols_reachable
    assert_kind_of Class, BamlSdk::RaisesTest::DocLoader
    %i[inferred_throw load_doc pure_len reparse].each { |name| assert_respond_to BamlSdk::RaisesTest, name }
  end

  def test_raises_union_throws_lists_all_names
    assert doc_for("def self.load_doc(").end_with?("Raises:\n    ParseError, TimeoutError")
  end

  def test_raises_async_sibling_also_has_raises
    assert doc_for("def self.load_doc_async(").end_with?("Raises:\n    ParseError, TimeoutError")
  end

  def test_raises_single_throws
    assert doc_for("def self.reparse(").end_with?("Raises:\n    ParseError")
  end

  def test_raises_summary_precedes_raises_block
    doc = doc_for("def self.load_doc(")
    assert doc.start_with?("Load a document from a path."), doc
    assert_includes doc, "\n\nRaises:\n"
  end

  def test_raises_inferred_contract_without_clause_still_raises
    assert doc_for("def self.inferred_throw(").end_with?("Raises:\n    ParseError")
  end

  def test_raises_non_throwing_function_has_no_raises_block
    refute_includes doc_for("def self.pure_len("), "Raises:"
  end

  def test_raises_method_raises_block_in_pyi
    # Ruby has no separate stub: both method comments live in generated source.
    load = doc_for("def load(")
    assert_includes load, "Raises:"
    assert_includes load, "ParseError"
    create = doc_for("def self.create(")
    assert_includes create, "Raises:"
    assert_includes create, "TimeoutError"
  end
end
