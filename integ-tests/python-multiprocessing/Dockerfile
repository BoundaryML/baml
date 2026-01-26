FROM python:3.12-slim-bookworm

# Install system dependencies for Rust and building
RUN apt-get update && apt-get install -y \
    curl \
    build-essential \
    pkg-config \
    libssl-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Install uv
RUN curl -LsSf https://astral.sh/uv/install.sh | sh
ENV PATH="/root/.local/bin:${PATH}"

# Copy the entire BAML repository
WORKDIR /build
COPY . .

# Set working directory to the test project
WORKDIR /build/integ-tests/python-multiprocessing

# Update pyproject.toml to use absolute path in container
RUN sed -i 's|path = "../../engine/language_client_python"|path = "/build/engine/language_client_python"|' pyproject.toml

# Create venv and install dependencies
RUN uv sync

# Install maturin for building Rust extensions
RUN uv pip install maturin

# Build the Rust extension (editable install)
RUN uv run maturin develop --uv --manifest-path /build/engine/language_client_python/Cargo.toml

# Set environment
ENV PYTHONPATH=/build/integ-tests/python-multiprocessing
ENV PYTHONUNBUFFERED=1
ENV VIRTUAL_ENV=/build/integ-tests/python-multiprocessing/.venv
ENV PATH="/build/integ-tests/python-multiprocessing/.venv/bin:${PATH}"

WORKDIR /build/integ-tests/python-multiprocessing

# Default command
CMD ["python", "-m", "app.manage", "rqworker", "async"]
