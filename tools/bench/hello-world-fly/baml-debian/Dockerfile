FROM debian:bookworm-slim@sha256:5ae3c39ebd15e229dcedd5cee596b2497182493d41ff162e824ba13fc1b2b867 AS build
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
ENV BAML_PROFILE=0 BAML_TELEMETRY_DISABLED=1 BAML_LOG=off
COPY install-baml.sh baml.toml ./
COPY baml_src ./baml_src
RUN sh install-baml.sh x86_64-unknown-linux-gnu && /opt/baml/bin/baml-cli pack main --output /app/hello --agent-skill-check off
FROM debian:bookworm-slim@sha256:5ae3c39ebd15e229dcedd5cee596b2497182493d41ff162e824ba13fc1b2b867
RUN apt-get update && apt-get install -y --no-install-recommends libstdc++6 ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
ENV BAML_PROFILE=0 BAML_TELEMETRY_DISABLED=1 BAML_LOG=off
COPY --from=build /app/hello ./hello
EXPOSE 8080
ENTRYPOINT ["/app/hello"]
