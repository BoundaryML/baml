FROM rust:1.98.0-bookworm@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922 AS compiler
RUN apt-get update && apt-get install -y --no-install-recommends cmake clang libclang-dev libssl-dev pkg-config protobuf-compiler && rm -rf /var/lib/apt/lists/*
WORKDIR /src/baml_language
COPY baml_language/ ./
COPY release/platforms.json /src/release/platforms.json
COPY skills/baml-core/SKILL.md /src/skills/baml-core/SKILL.md
ENV CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/usr/local/cargo/git --mount=type=cache,target=/src/baml_language/target \
    cargo build --locked -p baml_cli --bin baml-cli -j 2 && cp target/debug/baml-cli /tmp/baml-cli && \
    mkdir -p /tmp/cargo-cache && cp -a /usr/local/cargo/registry /usr/local/cargo/git /tmp/cargo-cache/

FROM node:22-bookworm-slim@sha256:83f487e0a63425e5b4d146fb5e5be574bcbe1b7b843d3ebafdd95eaf7767a7e5
RUN apt-get update && apt-get install -y --no-install-recommends \
    git bubblewrap ca-certificates gh build-essential curl jq util-linux coreutils rsync cmake clang libclang-dev libssl-dev pkg-config protobuf-compiler \
    && rm -rf /var/lib/apt/lists/* \
    && npm install -g @anthropic-ai/claude-code@2.1.261 @infisical/cli@0.43.128
COPY --from=compiler /tmp/baml-cli /opt/baml/bin/baml-cli
COPY --from=compiler /usr/local/rustup /opt/rustup
COPY --from=compiler /usr/local/cargo/bin /opt/cargo/bin
COPY --from=compiler /tmp/cargo-cache /opt/cargo
RUN ln -s baml-cli /opt/baml/bin/baml
WORKDIR /app
COPY tools/miniatb/baml.toml tools/miniatb/*.baml ./
COPY tools/miniatb/connections/ connections/
COPY tools/miniatb/deployment/ deployment/
RUN chmod +x deployment/claude deployment/askpass deployment/entrypoint
ENV BAML_TELEMETRY_DISABLED=1
ENV PATH="/opt/baml/bin:${PATH}"
RUN node --test deployment/*.test.mjs && baml check --agent-skill-check off && baml test --agent-skill-check off
CMD ["/app/deployment/entrypoint"]
