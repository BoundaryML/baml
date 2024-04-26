begin
  ruby_version = /(\d+\.\d+)/.match(RUBY_VERSION)
  require_relative "#{ruby_version}/baml/ruby_ffi"
rescue LoadError
  require_relative "baml/ruby_ffi"
end

require 'ffi'

module Baml 
  extend FFI::Library
  ffi_lib File.join(File.dirname(__FILE__), 'baml', 'ruby_ffi.so')
  attach_function :hello_from_rust, [], :pointer, blocking: true
end
