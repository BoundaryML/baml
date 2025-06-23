ARG PYTHON_VERSION=3.10
FROM python:${PYTHON_VERSION}-slim

WORKDIR /app

# Install build dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    build-essential \
    curl \
    pkg-config \
    git \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Install uv properly
ADD https://astral.sh/uv/install.sh /uv-installer.sh

RUN sh /uv-installer.sh && rm /uv-installer.sh

ENV PATH="/root/.local/bin/:$PATH"

# Install maturin using uv
RUN uv pip install --system maturin

# Create and activate virtual environment
RUN uv venv
ENV VIRTUAL_ENV=/app/.venv
ENV PATH="$VIRTUAL_ENV/bin:$PATH"

# Copy the entire BAML repository
COPY . /app/baml/

# Build and install the Python package
WORKDIR /app/baml
RUN uv run maturin develop --manifest-path engine/language_client_python/Cargo.toml

# Create a test script
COPY integ-tests/python/docker-tests/test.py ./test.py
COPY integ-tests/python/docker-tests/test-project ./test-project

# Run tests
CMD ["python", "test-project/main.py"]
