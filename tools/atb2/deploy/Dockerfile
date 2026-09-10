# The atb2 runner: one machine, one process, that serves "@bammy babysit"
# requests and runs the feedback pipeline on a timer. Everything the pipeline shells out to is
# here: git + gh (PRs), cargo (the gate, canary's own baml-cli), node + the
# Claude Code CLI (the agent). canary's baml-cli itself is built on the
# volume at first boot (entrypoint).
#
# State lives on the volume at /data (ATB2_HOME): the isolated bootstrap cache,
# runtime cargo target, worktrees, run dirs. First boot builds canary's
# baml-cli there (minutes); later boots are warm.
#
# Build context is tools/atb2 (the repo root's .dockerignore keeps everything
# out): `docker build -f tools/atb2/deploy/Dockerfile tools/atb2`.

FROM node:22-bookworm-slim@sha256:83f487e0a63425e5b4d146fb5e5be574bcbe1b7b843d3ebafdd95eaf7767a7e5 AS node
FROM rust:1.98.0-bookworm

ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl git jq openssh-client pkg-config libssl-dev \
      python3 clang lld cmake util-linux bubblewrap \
    && rm -rf /var/lib/apt/lists/*

# Immutable gate executables and the toolchain required by canary. Workspace
# caches are private; they cannot install replacements into this trusted PATH.
RUN rustup component add clippy rustfmt rust-analyzer \
    && rustup target add wasm32-unknown-unknown \
    && cargo install --locked --version 0.9.143 cargo-nextest \
    && cargo install --locked --version 1.48.0 cargo-insta \
    && rm -rf /usr/local/cargo/registry /usr/local/cargo/git

# gh
RUN curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg \
      -o /usr/share/keyrings/githubcli-archive-keyring.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
      > /etc/apt/sources.list.d/github-cli.list \
    && apt-get update && apt-get install -y --no-install-recommends gh \
    && rm -rf /var/lib/apt/lists/*

# Claude Code requires Node >=22. Copy a digest-pinned runtime, without an installer script.
COPY --from=node --chown=root:root /usr/local/bin/node /usr/local/bin/node
COPY --from=node --chown=root:root /usr/local/lib/node_modules/npm /usr/local/lib/node_modules/npm
RUN ln -s ../lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
    && ln -s ../lib/node_modules/npm/bin/npx-cli.js /usr/local/bin/npx \
    && npm install -g @anthropic-ai/claude-code@2.1.261 @infisical/cli@0.43.128

RUN useradd -m -u 1000 -s /bin/bash atb2 \
    && useradd -m -u 1001 -s /bin/bash builder \
    && mkdir -p /data \
    && chmod -R go-w /usr/local/cargo /usr/local/rustup
ENV ATB2_HOME=/data \
    HOME=/home/atb2 \
    PATH=/home/atb2/.local/bin:/data/target/debug:/usr/local/cargo/bin:$PATH \
    CARGO_HOME=/data/cargo \
    RUSTUP_HOME=/data/rustup \
    GIT_TERMINAL_PROMPT=0

WORKDIR /app
COPY baml.toml ./
COPY baml_src ./baml_src
COPY deploy/cache-cli.py deploy/cli-cache-service.py deploy/build-version.py /usr/local/lib/atb2/
COPY deploy/sessions.py deploy/session_home.py /usr/local/lib/atb2/
COPY deploy/reconcile-releases.py /usr/local/lib/atb2/
COPY deploy/traces.py /usr/local/lib/atb2/
RUN mkdir -p /usr/local/lib/atb2/traced-bin \
    && printf '#!/bin/sh\nexec /usr/bin/python3 -I /usr/local/lib/atb2/traces.py "$@"\n' > /usr/local/lib/atb2/traced-bin/claude \
    && chmod 755 /usr/local/lib/atb2/traced-bin/claude
COPY deploy/sandbox.py deploy/push.py deploy/push-scan.py /usr/local/lib/atb2/
COPY deploy/entrypoint.sh /usr/local/bin/atb2-entrypoint
COPY deploy/build-cli.sh /usr/local/bin/atb2-build-cli
COPY deploy/bootstrap.sh /usr/local/bin/atb2-bootstrap
COPY deploy/launch-runtime.py /usr/local/bin/atb2-launch-runtime.py
RUN chmod 755 /usr/local/bin/atb2-entrypoint /usr/local/bin/atb2-build-cli /usr/local/bin/atb2-bootstrap

USER root
ENTRYPOINT ["atb2-bootstrap"]
# fly.toml picks the expression; runner_loop() serves requests and runs the pipeline
CMD ["runner_loop()"]
