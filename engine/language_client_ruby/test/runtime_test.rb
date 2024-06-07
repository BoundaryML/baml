# frozen_string_literal: true

require_relative "../lib/baml"

require 'minitest/autorun'
require 'minitest/reporters'

describe "runtime sanity check" do
  it "can build runtime" do
    baml = Baml::Ffi::BamlRuntime.from_directory("/Users/sam/baml/integ-tests/baml_src", {})
    # assert_equal(baml.always_error("input"), "0.1.0")

  end
end

Minitest::Reporters.use! Minitest::Reporters::SpecReporter.new
