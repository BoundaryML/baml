begin
  ruby_version = /(\d+\.\d+)/.match(RUBY_VERSION)
  require_relative "#{ruby_version}/baml"
rescue LoadError
  require_relative "baml"
end

module Baml
  class Error < StandardError; end

  LATIN_TEXT = "loirem"
end
