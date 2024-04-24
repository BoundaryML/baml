# frozen_string_literal: true

require "test/unit"
require_relative "../dist/baml"

class BlankTest < Test::Unit::TestCase

  def test_blank?
    BamlRuntimeFfi.new
  end

end
