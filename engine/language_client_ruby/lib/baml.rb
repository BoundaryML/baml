require "baml/version"

begin
  RUBY_VERSION =~ /(\d+\.\d+)/
  require "baml/#{$1}/baml"
rescue LoadError
  require "baml/baml"
end

module Baml
  class Error < StandardError; end

  LATIN_TEXT = "loirem"
end
