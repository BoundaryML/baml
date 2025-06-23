FROM --platform=linux/arm64 python:3.10-slim

# Copy requirements if present, install dependencies
COPY requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt || true

# Install build dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    build-essential \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Install uv for Python package management
RUN curl -LsSf https://astral.sh/uv/install.sh | sh

# Copy the wheel file
COPY dist/*.whl ./

# Install the package
RUN uv pip install ./*.whl

# Copy the rest of the code
COPY . .

# Set the default command to run main.py
CMD ["python", "main.py"]
