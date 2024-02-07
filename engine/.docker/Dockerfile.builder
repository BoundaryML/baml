FROM lukemathwalker/cargo-chef:latest-rust-1-slim-buster AS chef
WORKDIR /baml_source_code

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder 
COPY --from=planner /baml_source_code/recipe.json recipe.json
RUN apt update && apt install -y pkg-config libssl-dev build-essential
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json
# Build application
COPY . .
RUN cargo build --release
