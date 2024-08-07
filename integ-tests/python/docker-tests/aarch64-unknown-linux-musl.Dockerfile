FROM ghcr.io/rust-cross/rust-musl-cross:aarch64-musl as base
RUN yum install python3-pip -y

ADD ../baml_py-0.53.0-cp38-abi3-manylinux_2_17_aarch64.manylinux2014_aarch64.whl  ./baml_py-0.53.0-cp38-abi3-manylinux_2_17_aarch64.manylinux2014_aarch64.whl 
RUN pip3 install ./baml_py-0.53.0-cp38-abi3-manylinux_2_17_aarch64.manylinux2014_aarch64.whl 

ENV RUST_LOG=trace
RUN baml-cli --help