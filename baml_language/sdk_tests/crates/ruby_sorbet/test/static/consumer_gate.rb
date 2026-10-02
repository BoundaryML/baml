# frozen_string_literal: true

# Run with plain Ruby, not `bundle exec` from the runtime bundle. This process
# selects the checker bundle without changing the runtime tests' environment.
root = File.expand_path("../..", __dir__)
workspace = File.expand_path("../../..", root)
target = File.expand_path(ENV.fetch("CARGO_TARGET_DIR", "target"), workspace)
ENV["BUNDLE_GEMFILE"] = File.join(__dir__, "Gemfile")
ENV["BUNDLE_PATH"] = File.join(target, "ruby-sorbet-static-bundle")
ENV["BUNDLE_FROZEN"] = "true"
require "bundler/setup"
require "open3"

spec = Gem.loaded_specs.fetch("sorbet-static")
raise "Expected sorbet-static 0.6.13506, got #{spec.version}" unless spec.version.to_s == "0.6.13506"
checker = File.join(spec.full_gem_path, "libexec", "sorbet")
raise "Missing bundled Sorbet binary: #{checker}" unless File.executable?(checker)
version, status = Open3.capture2e(checker, "--version")
raise "Unexpected checker version: #{version}" unless status.success? && version.start_with?("Sorbet typechecker 0.6.13506 ")
puts version

# Every generated declaration and bytecode file is checked, in place. No
# generated SDK signatures are supplied by an RBI or a copied test model.
sources = lambda do |directory|
  files = [File.join(directory, "baml_sdk.rb"), *Dir.glob(File.join(directory, "baml_sdk", "**", "*.rb")).sort]
  raise "Missing generated SDK: #{directory}" unless files.length > 1 && files.all? { |file| File.file?(file) }
  files.each do |file|
    raise "Expected strict generated source: #{file}" unless File.open(file, &:readline).strip == "# typed: strict"
  end
  files
end

check = lambda do |files, consumer, expected = nil|
  command = [checker, "--no-config", "--color=never", "--no-error-sections", "--no-error-count",
             File.join(__dir__, "bridge.rbi"), *files, consumer]
  stdout, stderr, result = Open3.capture3(*command)
  output = stdout + stderr
  if expected.nil?
    raise "Baseline #{consumer} failed (#{result.exitstatus}):\n#{output}" unless result.success? && output.empty?
    puts "PASS #{File.basename(consumer)} (#{files.length} generated files, no diagnostics)"
    next
  end

  # Assert the complete diagnostic set, including the source path, line and
  # code. Reject unrelated diagnostics and non-diagnostic checker failures.
  diagnostics = []
  output.each_line do |line|
    if (match = line.match(/\A(.+):(\d+): .+ https:\/\/srb\.help\/(\d+)\n?\z/))
      diagnostics << [File.expand_path(match[1]), match[2].to_i, match[3].to_i]
    elsif !line.strip.empty? && !line.match?(/\A\s*(?:\d+ \|.*|\^+)\s*\z/)
      raise "Unexpected checker output:\n#{output}"
    end
  end
  unless result.exitstatus == 100 && diagnostics == [expected]
    raise "Expected #{expected.inspect}, got #{diagnostics.inspect} (exit #{result.exitstatus}):\n#{output}"
  end
  puts "PASS #{File.basename(consumer)}:#{expected[1]} error #{expected[2]}"
  puts output
end

function_sources = sources.call(File.join(root, "function_calls", "generated"))
positive = File.join(__dir__, "function_calls.rb")
check.call(function_sources, positive)
check.call(sources.call(File.join(root, "llm_functions", "generated")), File.join(__dir__, "llm_functions.rb"))

# The Rust harness supplies a fresh, generator-produced recursive fixture.
recursive = ARGV.fetch(0)
check.call(sources.call(recursive), File.join(__dir__, "class_loading.rb"))

%w[wrong_argument wrong_return wrong_field_input wrong_field_result].each do |name|
  consumer = File.join(__dir__, "#{name}.rb")
  markers = File.readlines(consumer).each_with_index.filter_map do |line, index|
    match = line.match(/# error: (\d+)/)
    [File.expand_path(consumer), index + 1, match[1].to_i] if match
  end
  raise "Expected exactly one error marker in #{consumer}" unless markers.length == 1
  check.call([*function_sources, positive], consumer, markers.fetch(0))
end
puts "Sorbet consumer gate: 3 clean baselines and 4 exact negative diagnostics"
