FROM ubuntu:24.04

# Install dependencies
RUN apt update && DEBIAN_FRONTEND=noninteractive apt install -y \
    sudo \
    curl \
    git \
    build-essential \
    clang \
    cmake \
    pkg-config \
    zlib1g \
    zlib1g-dev \
    libssl-dev \
    libffi-dev \
    libyaml-dev

# Add non-privileged user to sudoers
ARG USER=ubuntu
RUN adduser $USER sudo && echo '%sudo ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers
USER $USER

# Install Mise and set up Ruby dynamically linked library path.
#
# Rest of tools can be installed with Mise using scripts/setup-dev.sh
RUN curl https://mise.run | sh \
    && echo 'eval "$(~/.local/bin/mise activate bash)"' >> ~/.bashrc \
    && echo 'export LD_LIBRARY_PATH=$LD_LIBRARY_PATH:$HOME/.local/share/mise/installs/ruby/3.2/lib' >> ~/.bash_profile

COPY mise.toml /workspaces/baml/mise.toml

WORKDIR /workspaces/baml/

# This is a little clunky, but the problem is that the pipx backend in Mise
# requires pipx to be installed. Read here:
#
# https://mise.jdx.dev/dev-tools/backends/pipx.html
#
# So this command does this:
# 1. Setup Mise project.
# 2. Install Python.
# 3. Install pipx with the Mise Python version.
# 4. Install rest of tools.
ENV PATH="/home/${USER}/.local/bin:/home/${USER}/.local/share/mise/shims:${PATH}"
RUN mise settings add idiomatic_version_file_enable_tools "[]" \
    && mise trust \
    && mise install python \
    && pip install --user pipx \
    && mise install

# Tool versions are managed with Mise in the Docker container as well because
# it's a pain to maintain in sync both the Dockerfile and mise.toml file. If you
# want to fully manage dependencies with Docker, here's the previous version of
# this Dockerfile before Mise was set up (it's missing some stuff like protobuf):
#
# https://github.com/BoundaryML/baml/blob/e8c98595c3b9048ee40b8a99d2f735ce9610cf74/.devcontainer/Dockerfile
