# frozen_string_literal: true

require "bundler"
require "fileutils"
require "open3"
require "rbconfig"
require "tmpdir"

runtime, target, cli, sorbet_bundle = ARGV
unless runtime && target && cli
  raise "usage: gem_smoke.rb <native library> <target triple> <baml-cli> [Sorbet bundle path]"
end

test_root = File.expand_path("..", __dir__)
repo_root = File.expand_path("../../../..", test_root)
gem_root = File.join(repo_root, "baml_language/sdks/ruby/bridge_ruby")
# Use the bundle's exact dependency archives, including transitive dependencies.
# The install itself must not see the bundle or any already-installed gems.
archives = Bundler.load.specs.reject { |spec| spec.name == "bundler" }.map(&:cache_file)

Dir.mktmpdir("baml-gem-") do |root|
  raise "clean app must be outside the checkout" if root.start_with?("#{repo_root}/")

  app = File.join(root, "app")
  gem_home = File.join(root, "gems")
  baml_home = File.join(root, "baml")
  packages = File.join(root, "packages")
  FileUtils.mkdir_p([app, gem_home, packages])
  FileUtils.cp(archives, packages)
  File.write(File.join(app, "baml.toml"), "[package]\nname = \"ruby_smoke\"\n")
  FileUtils.mkdir_p(File.join(app, "baml_src"))
  File.write(File.join(app, "baml_src/main.baml"), 'function HelloWorld() -> string { "hello world" }')

  Bundler.with_unbundled_env do
    env = {
      "GEM_HOME" => gem_home, "GEM_PATH" => gem_home,
      "RUBYLIB" => nil, "RUBYOPT" => nil, "RUBYGEMS_GEMDEPS" => nil,
      "BAML_HOME" => baml_home, "BAML_BRIDGE_PATH" => nil,
      "DEV_BAML_CLI_DISABLE_AGENT_DETECTION" => "1", "BAML_AGENT_SKILL_CHECK" => "off"
    }
    run = lambda do |directory, *args|
      stdout, stderr, status = Open3.capture3(env, RbConfig.ruby, *args, chdir: directory)
      raise "#{args.inspect} failed (#{status.exitstatus}):\n#{stdout}\n#{stderr}" unless status.success?

      stdout
    end

    [["generate", "add", "ruby/sorbet"], ["generate", "--output-dir", app]].each do |args|
      stdout, stderr, status = Open3.capture3(env, cli, *args, chdir: app)
      raise "baml-cli #{args.join(' ')} failed (#{status.exitstatus}):\n#{stdout}\n#{stderr}" unless status.success?
    end
    raise "CLI did not generate Ruby" unless File.file?(File.join(app, "baml_sdk.rb"))

    package = File.join(packages, "baml-bridge.gem")
    run.call(gem_root, "-S", "gem", "build", "--norc", "baml-bridge.gemspec", "--output", package)
    run.call(packages, "-S", "gem", "install", "--norc", "--local", "--no-document", package)

    version = run.call(app, "-rbaml/bridge/version", "-e", "print Baml::Bridge::TOOLCHAIN_VERSION")
    cached_library = File.join(baml_home, "bridges", version, target, File.basename(runtime))
    FileUtils.mkdir_p(File.dirname(cached_library))
    FileUtils.cp(runtime, cached_library)

    output = run.call(app, "-I", app, "-e", <<~RUBY)
      require "baml_sdk"
      raise "missing embedded metadata" unless BamlSdk::METADATA&.include?("ruby/sorbet")
      puts BamlSdk.hello_world
    RUBY
    raise "unexpected clean-app output: #{output.inspect}" unless output == "hello world\n"

    # Check the actual loaded files as well as the isolated environment.
    run.call(app, "-I", app, "-e", <<~RUBY)
      require "baml_sdk"
      raise "repository load path" if ($LOAD_PATH + $LOADED_FEATURES).any? { |path| path.start_with?(#{repo_root.dump}) }
      %w[baml-bridge ffi google-protobuf sorbet-runtime].each do |name|
        raise "external gem: \#{name}" unless Gem.loaded_specs.fetch(name).full_gem_path.start_with?(#{gem_home.dump})
      end
      spec = Gem.loaded_specs.fetch("baml-bridge")
      raise "gem version does not match stamped runtime" unless spec.version == Gem::Version.new(Baml::Bridge::BRIDGE_RUNTIME_VERSION)
      %w[README.md LICENSE rbi/baml.rbi lib/baml_bridge/cffi/v1/baml_inbound_pb.rb].each do |file|
        raise "missing packaged file: \#{file}" unless File.file?(File.join(spec.full_gem_path, file))
      end
    RUBY

    if sorbet_bundle
      # Check the app as a Tapioca user would: Tapioca's default config ignores
      # every vendor/ directory, and the installed gem supplies the runtime RBI.
      gem_path = run.call(app, "-e", 'print Gem::Specification.find_by_name("baml-bridge").full_gem_path')
      FileUtils.mkdir_p(File.join(app, "sorbet"))
      File.write(File.join(app, "sorbet/config"), "--dir\n.\n--ignore=vendor/\n#{File.join(gem_path, 'rbi/baml.rbi')}\n")
      File.write(File.join(app, "typed.rb"), <<~RUBY)
        # typed: strict
        T.let(BamlSdk.hello_world, String)
        T.let(BamlSdk.hello_world_async, Baml::Call[String])
      RUBY
      sorbet_env = {
        "BUNDLE_GEMFILE" => File.join(test_root, "test/static/Gemfile"),
        "BUNDLE_PATH" => sorbet_bundle, "BUNDLE_FROZEN" => "true"
      }
      stdout, stderr, status = Open3.capture3(sorbet_env, RbConfig.ruby, "-S", "bundle", "exec", "srb", "tc", chdir: app)
      raise "Sorbet rejected the clean app (#{status.exitstatus}):\n#{stdout}\n#{stderr}" unless status.success?
    end
  end
end

puts "Installed baml-bridge gem: clean app returned hello world"
puts "Sorbet accepted the clean app with the installed RBI" if sorbet_bundle
