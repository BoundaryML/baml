require "mkmf"
require "rb_sys/mkmf"

# create_rust_makefile takes, as its argument, ${CRATE_NAME}/${GEM_NAME} where:
#
#    - CRATE_NAME is the name of the crate in ext/${GEM_NAME}/Cargo.toml
#    - GEM_NAME is the name of the gem in ext/baml
create_rust_makefile("baml/baml")
