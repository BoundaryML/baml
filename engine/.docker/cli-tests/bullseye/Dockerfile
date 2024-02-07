# The final base image
FROM debian:bullseye-slim

# Copy the compiled binary from the builder stage
COPY --from=baml_builder /baml_source_code/target/release/baml /usr/bin/baml

# Run the binary
CMD ["/usr/bin/baml", "--version"]
