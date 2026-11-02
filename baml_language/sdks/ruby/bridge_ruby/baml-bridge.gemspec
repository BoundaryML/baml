# frozen_string_literal: true

require_relative "lib/baml/bridge/version"

Gem::Specification.new do |spec|
  spec.name = "baml-bridge"
  spec.version = Baml::Bridge::BRIDGE_RUNTIME_VERSION
  spec.authors = ["BoundaryML"]
  spec.email = ["contact@boundaryml.com"]
  spec.summary = "Runtime for generated BAML Ruby/Sorbet SDKs"
  spec.homepage = "https://github.com/BoundaryML/baml"
  spec.license = "Apache-2.0"
  spec.required_ruby_version = ">= 3.2"

  spec.files = Dir.chdir(__dir__) { Dir["lib/**/*", "rbi/**/*", "README.md", "LICENSE"].select { |path| File.file?(path) } }
  spec.require_paths = ["lib"]

  spec.add_dependency "ffi", ">= 1.17.4", "< 2"
  spec.add_dependency "google-protobuf", "~> 4.0"
  spec.add_dependency "sorbet-runtime", "~> 0.5"
end
