# The atb2 runner: one machine that serves "@bammy babysit" requests and runs
# the feedback pipeline on a timer. Everything the pipeline shells out to is
# here: git + gh (PRs), cargo (the gate, canary's own baml-cli), node + the
# Claude Code CLI (the agent). canary's baml-cli itself is built on the
# volume at first boot (entrypoint).
#
# State lives on the volume at /data (ATB2_HOME): the cached canary clone,
# the shared cargo target, worktrees, run dirs. First boot builds canary's
# baml-cli there (minutes); later boots are warm.
#
# Build context is the repo root (fly.toml sets it): tools/atb2 is copied in,
# nothing else.

FROM rust:1-bookworm

ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl git jq openssh-client pkg-config libssl-dev \
      python3 clang lld cmake \
    && rm -rf /var/lib/apt/lists/*

# gh
RUN curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg \
      -o /usr/share/keyrings/githubcli-archive-keyring.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
      > /etc/apt/sources.list.d/github-cli.list \
    && apt-get update && apt-get install -y --no-install-recommends gh \
    && rm -rf /var/lib/apt/lists/*

# node (Debian's package, no third-party install script) + the Claude Code
# CLI the agent runs as; pin the CLI when bumping the image
RUN apt-get update && apt-get install -y --no-install-recommends nodejs npm \
    && rm -rf /var/lib/apt/lists/* \
    && npm install -g @anthropic-ai/claude-code

RUN useradd -m -u 1000 -s /bin/bash atb2 && mkdir -p /data && chown atb2:atb2 /data
ENV ATB2_HOME=/data \
    HOME=/home/atb2 \
    PATH=/home/atb2/.local/bin:/data/target/debug:/usr/local/cargo/bin:$PATH \
    CARGO_HOME=/usr/local/cargo \
    RUSTUP_HOME=/usr/local/rustup \
    GIT_TERMINAL_PROMPT=0

WORKDIR /app
COPY tools/atb2/baml.toml ./
COPY tools/atb2/baml_src ./baml_src
COPY tools/atb2/deploy/entrypoint.sh /usr/local/bin/atb2-entrypoint
RUN chmod +x /usr/local/bin/atb2-entrypoint && chown -R atb2:atb2 /app

USER atb2
ENTRYPOINT ["atb2-entrypoint"]
# fly.toml picks the process: babysit_loop() or pipeline_loop()
CMD ["babysit_loop()"]
