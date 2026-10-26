# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"

class ImportTest < Minitest::Test
  def test_main_root_imports_cleanly
    assert_kind_of Module, BamlSdk
  end
end
