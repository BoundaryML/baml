FROM golang:1.24 AS build

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y \
    gcc \
    git \
    make \
    && rm -rf /var/lib/apt/lists/*

# Install Orchestrion for APM auto-instrumentation
# This enables zero-code-change Datadog APM integration
RUN go install github.com/DataDog/orchestrion@v1.4.0

# Copy the entire BAML repository (needed for replace directive in go.mod)
COPY . .

# Set working directory to integ-tests/go
WORKDIR /build/integ-tests/go

# Download dependencies (uses replace directive to ../../ for BAML)
RUN go mod download

# Build with Orchestrion instrumentation
# This should trigger the SIGSEGV when BAML tries to initialize
RUN orchestrion pin && orchestrion go build -o app ./orchestrion-repro

# Start a new stage from Ubuntu
FROM ubuntu:22.04 AS run

# Install CA certificates for HTTPS requests (needed for BAML library download)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from build stage
COPY --from=build /build/integ-tests/go/app ./app

# Make sure the binary is executable
RUN chmod +x ./app

# Command to run the executable
CMD ["/app/app"]
