require "mkmf"
require "rb_sys/mkmf"

# no idea what create_rust_makefile actually takes as its arg, i thought it did
# the below, but i don't think that's right anymore:
#
# create_rust_makefile takes, as its argument, ${CRATE_NAME}/${GEM_NAME} where:
#
#    - CRATE_NAME is the name of the crate in ext/${GEM_NAME}/Cargo.toml
#    - GEM_NAME is the name of the gem in ext/baml
create_rust_makefile("ruby_ffi") do |r|
  r.extra_cargo_args += ["--package", "ruby_ffi"]
end
