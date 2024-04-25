begin
  ruby_version = /(\d+\.\d+)/.match(RUBY_VERSION)
  require_relative "#{ruby_version}/baml/ruby_ffi"
rescue LoadError
  require_relative "baml/ruby_ffi"
end