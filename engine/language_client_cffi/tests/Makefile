# Makefile for BAML CFFI Cancellation Tests

CC = gcc
CFLAGS = -Wall -Wextra -std=c99 -pthread -O2
LDFLAGS = -pthread -lrt

# Directories
SRC_DIR = ..
INCLUDE_DIR = $(SRC_DIR)/include
LIB_DIR = $(SRC_DIR)/lib
TARGET_DIR = ../target/release

# Files
TEST_SRC = test_cancellation.c
TEST_BIN = test_cancellation
BAML_LIB = $(TARGET_DIR)/libbaml_cffi.a

# Default target
all: $(TEST_BIN)

# Build the test binary
$(TEST_BIN): $(TEST_SRC) $(BAML_LIB)
	$(CC) $(CFLAGS) -I$(INCLUDE_DIR) -o $(TEST_BIN) $(TEST_SRC) $(BAML_LIB) $(LDFLAGS)

# Build the BAML CFFI library first
$(BAML_LIB):
	cd $(SRC_DIR) && cargo build --release

# Run tests
test: $(TEST_BIN)
	./$(TEST_BIN)

# Run tests with memory checking (if valgrind is available)
test-memory: $(TEST_BIN)
	@if command -v valgrind >/dev/null 2>&1; then \
		echo "Running tests with valgrind..."; \
		valgrind --leak-check=full --show-leak-kinds=all --track-origins=yes ./$(TEST_BIN); \
	else \
		echo "Valgrind not available, running normal tests..."; \
		./$(TEST_BIN); \
	fi

# Clean build artifacts
clean:
	rm -f $(TEST_BIN)
	rm -f *.o
	rm -f core

# Clean everything including Rust artifacts
clean-all: clean
	cd $(SRC_DIR) && cargo clean

# Help target
help:
	@echo "Available targets:"
	@echo "  all         - Build test binary"
	@echo "  test        - Build and run tests"
	@echo "  test-memory - Run tests with memory checking (valgrind)"
	@echo "  clean       - Clean test artifacts"
	@echo "  clean-all   - Clean all artifacts including Rust"
	@echo "  help        - Show this help"

.PHONY: all test test-memory clean clean-all help
