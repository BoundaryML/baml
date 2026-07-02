# Fleet image — runs the BAML supervisor (api + ingress + processors). The whole
# fleet runs on the pinned `baml` runtime; the LLM gateway lives on the separate
# proxy image. Secret injection is the language-agnostic with-infisical wrapper.
FROM debian:trixie-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl git xz-utils \
    && curl -1sLf 'https://artifacts-cli.infisical.com/setup.deb.sh' | bash \
    && apt-get update && apt-get install -y --no-install-recommends infisical \
    && rm -rf /var/lib/apt/lists/*

# Install the baml wrapper + bootstrap the nightly toolchain (we pin nightly for
# server-side SSE streaming; see baml.toml).
ENV BAML_HOME=/opt/baml
# Pin the SERVICE toolchain to the version the code is tested on — a floating
# `--channel nightly` broke the build when the stdlib moved under us
# (baml.unstable.string removed on 20260702). The agent-under-test still gets
# the latest nightly per-run via the proxy's ensure_agent_baml.
RUN curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s -- --version 0.12.2-nightly.20260624.a --no-modify-path --yes
ENV PATH="/opt/baml/bin:${PATH}"

WORKDIR /app
COPY baml.toml ./
COPY baml_src ./baml_src
COPY docker/with-infisical /usr/local/bin/with-infisical
RUN chmod +x /usr/local/bin/with-infisical && baml check

ENV PYTHONUNBUFFERED=1
ENTRYPOINT ["/usr/local/bin/with-infisical"]
CMD ["baml", "run", "Supervisor"]
