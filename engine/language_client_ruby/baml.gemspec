# frozen_string_literal: true

Gem::Specification.new do |spec|
  spec.name = "baml"
  spec.version = "0.1.0"
  spec.authors = ["BoundaryML"]
  spec.email = ["contact@boundaryml.com"]

  spec.summary = "Unified BoundaryML LLM client"
  spec.description = "A gem for users to interact with BoundaryML's Language Model clients (LLM) in Ruby."
  spec.homepage = "https://github.com/BoundaryML/baml"
  spec.license = "MIT"
  spec.required_ruby_version = ">= 2.7.0"

  # Specify which files should be added to the gem when it is released.
  # The `git ls-files -z` loads the files in the RubyGem that have been added into git.
  spec.files = Dir["lib/**/*.rb", "ext/**/*.{rs,toml,lock,rb}"]
  spec.bindir = "exe"
  spec.executables = []
  spec.require_paths = ["lib"]
  spec.extensions = ["ext/baml/extconf.rb"]

  # For more information and examples about making a new gem, check out our
  # guide at: https://bundler.io/guides/creating_gem.html
end