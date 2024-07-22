FROM ghcr.io/rust-cross/manylinux_2_28-cross:aarch64
WORKDIR /baml

RUN curl https://mise.run | sh
RUN echo 'eval "$(~/.local/bin/mise activate bash)"' >> ~/.bashrc

COPY .mise.toml /baml/.mise.toml 

#RUN ~/.local/bin/mise install ruby