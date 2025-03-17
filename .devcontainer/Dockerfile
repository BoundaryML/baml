FROM ubuntu:24.04

# Install dependencies
RUN apt update && DEBIAN_FRONTEND=noninteractive apt install -y \
    build-essential \
    clang \
    pkg-config \
    zlib1g \
    zlib1g-dev \
    libssl-dev \
    libffi-dev \
    libyaml-dev \
    python3 \
    python3-pip \
    pipx \
    curl \
    sudo \
    git

# Add non-privileged user to sudoers
ARG USER=ubuntu
RUN adduser $USER sudo && echo '%sudo ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers
USER $USER

# Install Poetry
RUN pipx install poetry

# Install Mise (Ruby, Node, PNPM)
RUN curl https://mise.run | sh \
    && ~/.local/bin/mise --verbose use -g --yes ruby@3.1 node@20.14 pnpm@9.9 \
    && echo 'export PATH="$HOME/.local/bin:$HOME/.local/share/mise/shims:$PATH"' >> ~/.bash_profile \
    && echo 'export LD_LIBRARY_PATH=$LD_LIBRARY_PATH:$HOME/.local/share/mise/installs/ruby/3.1/lib' >> ~/.bash_profile

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# Install WASM tools
RUN cargo install wasm-bindgen-cli@0.2.92 wasm-pack

# Install Infisical
RUN curl -1sLf 'https://dl.cloudsmith.io/public/infisical/infisical-cli/setup.deb.sh' | sudo -E bash \
    && sudo apt update && sudo apt install -y infisical
