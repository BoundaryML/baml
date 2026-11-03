# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"

class MainTest < Minitest::Test
  def source
    File.read(File.join(__dir__, "baml_sdk/docs.rb"))
  end

  # Ruby comments, like Java's Javadoc, are a source-only documentation surface.
  def doc_for(name)
    lines = source.split("class #{name} <", 2).first.lines
    lines.pop while lines.last&.strip == ""
    lines.reverse.take_while { |line| line.lstrip.start_with?("#") }
      .reverse.map { |line| line.sub(/^\s*# ?/, "").rstrip }.join("\n")
  end

  def test_main_imports_symbols_reachable
    [BamlSdk::Docs::Doc, BamlSdk::Docs::Note, BamlSdk::Docs::Priority, BamlSdk::Docs::Sentiment].each do |symbol|
      assert_kind_of Class, symbol
    end
  end

  def test_main_class_doc_summary_and_attributes_section
    assert_equal "A document with a title and an optional body.\n\nAttributes:\n" \
      "    title: Title shown in lists and search results.\n    body: Free-form body text.", doc_for("Doc")
  end

  def test_main_undocumented_field_listed_as_bare_name_under_attributes
    doc = doc_for("Note")
    assert doc.start_with?("A multi-line summary.\nContinuation line"), doc
    assert_includes doc, "\n\nAttributes:\n    id: Stable identifier"
    assert doc.end_with?("\n    text"), doc
  end

  def test_main_enum_doc_summary_and_members_section
    assert_equal "Sentiment labels surfaced by the model.\n\nMembers:\n" \
      "    HAPPY: Smiling face.\n    SAD: Frowning face.\n    NEUTRAL", doc_for("Sentiment")
  end

  def test_main_enum_summary_only_omits_members_section
    doc = doc_for("Priority")
    assert_equal "Pin the \"summary only, no member rollup\" case: this enum has a\n" \
      "class-level `///` but every variant is bare.", doc
    refute_includes doc, "Members:"
    assert_equal %w[HIGH LOW MEDIUM], BamlSdk::Docs::Priority.values.map(&:serialize).sort
  end

  def test_main_no_inline_field_or_variant_doc_artifacts
    refute_includes source, "# Title shown in lists"
    refute_includes source, "# Smiling face"
    refute_match(/(?:const :|= new\().*#/, source)
    assert_equal 4, source.scan(/(?:^\s*#.*\n)+(?=\s*class )/).length
  end
end
