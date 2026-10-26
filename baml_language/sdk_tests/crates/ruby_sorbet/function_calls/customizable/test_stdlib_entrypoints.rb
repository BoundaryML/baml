# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"

ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class StdlibEntrypointsTest < Minitest::Test
  def test_stdlib_entrypoints_native_argv_callable_as_entry_point
    assert_instance_of Array, BamlSdk::Baml::Sys.argv
  end

  def test_stdlib_entrypoints_sysop_fs_exists_callable_as_entry_point
    assert_equal true, BamlSdk::Baml::Fs.exists(".")
  end

  def test_stdlib_entrypoints_compiler_intrinsics_are_not_emitted_as_entry_points
    {
      "__vendor/log.rb" => %w[info debug warn error],
      "baml/events.rb" => ["send"]
    }.each do |file, names|
      path = File.join(__dir__, "baml_sdk", file)
      next unless File.file?(path)

      source = File.read(path)
      names.each do |name|
        wire = file == "__vendor/log.rb" ? "log.#{name}" : "baml.events.#{name}"
        refute_includes source, "\"#{wire}\""
        refute_match(/def self\.#{name}(?:_async)?\(/, source)
      end
    end
  end
end
