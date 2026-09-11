FROM debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171 AS build
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
ENV BAML_PROFILE=0 BAML_TELEMETRY_DISABLED=1 BAML_LOG=off
COPY scripts/install-baml.sh apps/baml-only/baml.toml ./
COPY apps/baml-only/baml_src ./baml_src
ARG TARGETARCH
RUN sh install-baml.sh "$TARGETARCH" gnu && /opt/baml/bin/baml-cli pack main --output /app/hello --agent-skill-check off
FROM amazonlinux:2023@sha256:181f98c48832fe926f8ca3b6ffeafcce128e96e77b93d08fbe9a9bc9403ce284
RUN dnf install -y libstdc++ ca-certificates && dnf clean all
WORKDIR /app
ENV BAML_PROFILE=0 BAML_TELEMETRY_DISABLED=1 BAML_LOG=off
COPY --from=build /app/hello ./hello
EXPOSE 8080
ENTRYPOINT ["/app/hello"]
