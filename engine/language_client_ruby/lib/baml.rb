begin
  ruby_version = /(\d+\.\d+)/.match(RUBY_VERSION)
  require_relative "baml/#{ruby_version}/ruby_ffi"
rescue LoadError
  require_relative "baml/ruby_ffi"
end
require_relative "stream"

module Baml
end