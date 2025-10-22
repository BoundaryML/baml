.PHONY: build build-docker run-docker clean test

# Build with orchestrion locally
build:
	orchestrion go build -o app ./orchestrion-repro

# Build the Docker image (must run from repo root)
build-docker:
	cd ../.. && docker build -t baml-orchestrion-repro -f integ-tests/go/orchestrion-repro/Dockerfile .

# Run the Docker container
run-docker: build-docker
	docker run --rm baml-orchestrion-repro

# Build and run with debug logging
debug-docker:
	cd ../.. && docker build --progress=plain -t baml-orchestrion-repro -f integ-tests/go/orchestrion-repro/Dockerfile .
	docker run --rm -e BAML_LOG=DEBUG -e ORCHESTRION_LOG_LEVEL=DEBUG baml-orchestrion-repro

# Clean up
clean:
	rm -f app
	docker rmi -f baml-orchestrion-repro 2>/dev/null || true

# Test locally without orchestrion
test:
	cd orchestrion-repro && go run main.go
