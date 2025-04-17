# frozen_string_literal: true

require_relative '../lib/baml'

require 'minitest/autorun'
require 'minitest/reporters'

describe 'runtime sanity check' do
  it 'can build runtime' do
    # Construct the path relative to the current file's directory (__dir__)
    # Go up three levels to the project root, then into integ-tests/baml_src
    baml_src_dir = File.expand_path('../../../integ-tests/baml_src', __dir__)
    Baml::Ffi::BamlRuntime.from_directory(baml_src_dir, {})
    # assert_equal(baml.always_error("input"), "0.1.0")
  end
end

Minitest::Reporters.use! Minitest::Reporters::SpecReporter.new
