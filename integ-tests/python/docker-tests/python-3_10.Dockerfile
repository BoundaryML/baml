ARG PYTHON_VERSION=3.10
FROM python:${PYTHON_VERSION} as base

RUN apt-get update

ADD ../baml_py-0.53.0-cp38-abi3-manylinux_2_17_aarch64.manylinux2014_aarch64.whl  ./baml_py-0.53.0-cp38-abi3-manylinux_2_17_aarch64.manylinux2014_aarch64.whl 
RUN pip install ./baml_py-0.53.0-cp38-abi3-manylinux_2_17_aarch64.manylinux2014_aarch64.whl 

ENV RUST_LOG=trace
RUN baml-cli --help