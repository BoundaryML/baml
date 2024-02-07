# Run a python test script
FROM continuumio/miniconda3
WORKDIR /myapp

# Initalize a conda environment
RUN conda create -n envbaml

RUN conda init

# Testing binaries
ENV CAPTURE_DIR="/usr/src/logs"
ENV BAML_TEST_LOGS="/usr/src/logs/baml_test"
RUN mkdir -p /usr/src/logs
COPY script.sh /usr/src/test.sh
RUN chmod +x /usr/src/test.sh
# Copy the compiled binary from the builder stage
COPY --from=baml_builder /baml_source_code/target/release/baml /usr/tools/baml
ENV PATH="/usr/tools:${PATH}"

CMD ["/usr/src/test.sh"]
