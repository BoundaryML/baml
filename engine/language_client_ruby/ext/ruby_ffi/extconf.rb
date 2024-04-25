require "mkmf"
require "rb_sys/mkmf"

# create_rust_makefile takes, as its argument, ${CRATE_NAME}/${GEM_NAME} where:
#
#    - CRATE_NAME is the name of the crate in ext/${GEM_NAME}/Cargo.toml
#    - GEM_NAME is the name of the gem in ext/baml
create_rust_makefile("ruby_ffi") do |r|
  #r.profile = ENV.fetch("RB_SYS_CARGO_PROFILE", :dev).to_sym
  r.ext_dir = "engine/language_client_ruby/ext/ruby_ffi"
end
