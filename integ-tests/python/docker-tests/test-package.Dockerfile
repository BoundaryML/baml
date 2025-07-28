FROM --platform=linux/amd64 python:3.12

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

# Install Go 1.22
RUN curl -L https://go.dev/dl/go1.22.10.linux-amd64.tar.gz -o go1.22.10.linux-amd64.tar.gz && \
    rm -rf /usr/local/go && \
    tar -C /usr/local -xzf go1.22.10.linux-amd64.tar.gz && \
    rm go1.22.10.linux-amd64.tar.gz

ENV PATH="/usr/local/go/bin:${PATH}"
ENV GOPATH="/root/go"
ENV PATH="${GOPATH}/bin:${PATH}"

# Install protobuf Go plugin
RUN go install google.golang.org/protobuf/cmd/protoc-gen-go@latest

# Install uv properly
ADD https://astral.sh/uv/install.sh /uv-installer.sh

RUN sh /uv-installer.sh && rm /uv-installer.sh

ENV PATH="/root/.local/bin/:$PATH"

# Install maturin using uv
RUN uv pip install --system maturin
