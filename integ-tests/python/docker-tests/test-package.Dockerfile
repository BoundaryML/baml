FROM python:3.12

WORKDIR /app

# Install build dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    build-essential \
    curl \
    pkg-config \
    git \
    bash \
    bash-completion \
    readline-common \
    cmake \
    vim \
    && rm -rf /var/lib/apt/lists/*

# Configure bash with history and completion
RUN echo 'export HISTSIZE=1000' >> /root/.bashrc && \
    echo 'export HISTFILESIZE=2000' >> /root/.bashrc && \
    echo 'export HISTCONTROL=ignoredups:erasedups' >> /root/.bashrc && \
    echo 'source /etc/bash_completion' >> /root/.bashrc && \
    echo 'alias ll="ls -la"' >> /root/.bashrc && \
    echo 'alias la="ls -A"' >> /root/.bashrc

# Set bash as default shell
SHELL ["/bin/bash", "-c"]

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Install uv properly
ADD https://astral.sh/uv/install.sh /uv-installer.sh

RUN sh /uv-installer.sh && rm /uv-installer.sh

ENV PATH="/root/.local/bin/:$PATH"

# Install maturin using uv
RUN uv pip install --system maturin
