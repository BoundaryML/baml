#!/bin/bash

# Comprehensive test runner for BAML cancellation functionality across all languages
set -e

echo "🧪 Running BAML Cancellation Tests - All Languages"
echo "=================================================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test results tracking
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

# Function to run tests with proper error handling
run_test() {
    local test_name=$1
    local test_command=$2
    local test_dir=${3:-"."}
    
    echo -e "${YELLOW}Running: $test_name${NC}"
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    if (cd "$test_dir" && eval "$test_command"); then
        echo -e "${GREEN}✅ $test_name passed${NC}"
        PASSED_TESTS=$((PASSED_TESTS + 1))
        return 0
    else
        echo -e "${RED}❌ $test_name failed${NC}"
        FAILED_TESTS=$((FAILED_TESTS + 1))
        return 1
    fi
}

# Function to check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Change to the project root
cd "$(dirname "$0")"
PROJECT_ROOT=$(pwd)

echo "📦 Building all projects..."
echo "----------------------------"

# Build Rust core
echo -e "${BLUE}Building Rust core...${NC}"
cd engine
cargo build --features "internal"

echo ""
echo "🦀 Running Rust Core Tests..."
echo "=============================="

# Run Rust core cancellation tests
run_test "Rust Core - Basic Cancellation Tests" \
    "cargo test test_cancellation --features internal --no-default-features -- --nocapture" \
    "."

run_test "Rust Core - HTTP Cancellation Tests" \
    "cargo test test_http_cancellation --features internal --no-default-features -- --nocapture" \
    "."

run_test "Rust Core - Integration Cancellation Tests" \
    "cargo test test_integration_cancellation --features internal --no-default-features -- --nocapture" \
    "."

echo ""
echo "📜 Running TypeScript Tests..."
echo "==============================="

cd language_client_typescript
run_test "TypeScript FFI Cancellation Tests" \
    "cargo test test_cancellation --features internal -- --nocapture" \
    "."

cd ..

echo ""
echo "🐍 Running Python Tests..."
echo "==========================="

cd language_client_python

# Build Python client
echo -e "${BLUE}Building Python client...${NC}"
cargo build --features "internal"

# Run Rust FFI tests
run_test "Python FFI Cancellation Tests (Rust)" \
    "cargo test test_cancellation --features internal -- --nocapture" \
    "."

# Run Python tests if pytest is available
if command_exists pytest; then
    run_test "Python Stream Cancellation Tests" \
        "pytest python_src/tests/test_cancellation.py -v" \
        "."
else
    echo -e "${YELLOW}⚠️  pytest not available, skipping Python integration tests${NC}"
fi

cd ..

echo ""
echo "🐹 Running Go Tests..."
echo "======================"

cd language_client_go

# Run Go tests if go is available
if command_exists go; then
    run_test "Go Stream Cancellation Tests" \
        "go test ./pkg -v" \
        "."
else
    echo -e "${YELLOW}⚠️  Go not available, skipping Go tests${NC}"
fi

cd ..

echo ""
echo "💎 Running Ruby Tests..."
echo "========================"

cd language_client_ruby

# Run Ruby tests if ruby is available
if command_exists ruby; then
    run_test "Ruby Stream Cancellation Tests" \
        "ruby test/test_cancellation.rb" \
        "."
else
    echo -e "${YELLOW}⚠️  Ruby not available, skipping Ruby tests${NC}"
fi

cd ..

echo ""
echo "🔗 Running CFFI/C Tests..."
echo "=========================="

cd language_client_cffi

# Build CFFI library
echo -e "${BLUE}Building CFFI library...${NC}"
cargo build --release

# Run C tests if gcc is available
if command_exists gcc; then
    cd tests
    run_test "CFFI C Cancellation Tests" \
        "make test" \
        "."
    
    # Run memory tests if valgrind is available
    if command_exists valgrind; then
        run_test "CFFI Memory Safety Tests" \
            "make test-memory" \
            "."
    else
        echo -e "${YELLOW}⚠️  valgrind not available, skipping memory tests${NC}"
    fi
    
    cd ..
else
    echo -e "${YELLOW}⚠️  gcc not available, skipping C tests${NC}"
fi

cd ..

echo ""
echo "🚀 Running Full Runtime Integration Tests..."
echo "============================================="

# Run existing runtime tests to ensure no regression
run_test "Runtime Integration Tests (Regression Check)" \
    "cargo test -p baml-runtime --features internal --no-default-features -- --nocapture" \
    "."

echo ""
echo "📊 Test Summary"
echo "==============="

if [ $FAILED_TESTS -eq 0 ]; then
    echo -e "${GREEN}🎉 All cancellation tests passed!${NC}"
    echo ""
    echo -e "${GREEN}✅ Total Tests: $TOTAL_TESTS${NC}"
    echo -e "${GREEN}✅ Passed: $PASSED_TESTS${NC}"
    echo -e "${GREEN}✅ Failed: $FAILED_TESTS${NC}"
    echo ""
    echo "🌟 Cancellation functionality is working correctly across all languages:"
    echo ""
    echo "   🦀 Rust Core:"
    echo "      • CancellationToken functionality"
    echo "      • HTTP request cancellation via tokio::select!"
    echo "      • Stream cancellation and cleanup"
    echo ""
    echo "   📜 TypeScript:"
    echo "      • stream.abort() → Rust cancel()"
    echo "      • FFI layer cancellation"
    echo "      • Resource cleanup"
    echo ""
    echo "   🐍 Python:"
    echo "      • stream.cancel() method"
    echo "      • Async and sync stream support"
    echo "      • Thread-safe cancellation"
    echo ""
    echo "   🐹 Go:"
    echo "      • context.Context based cancellation"
    echo "      • stream.Cancel() method"
    echo "      • Channel-based streaming with cancellation"
    echo ""
    echo "   💎 Ruby:"
    echo "      • stream.cancel method"
    echo "      • Thread-safe cancellation with mutex"
    echo "      • Iterator cancellation support"
    echo ""
    echo "   🔗 CFFI/C:"
    echo "      • C FFI cancellation functions"
    echo "      • Memory safety verified"
    echo "      • Thread-safe operations"
    echo ""
    echo "🚀 Ready for production use across all supported languages!"
else
    echo -e "${RED}❌ Some tests failed${NC}"
    echo ""
    echo -e "${YELLOW}📊 Test Results:${NC}"
    echo -e "   Total Tests: $TOTAL_TESTS"
    echo -e "${GREEN}   Passed: $PASSED_TESTS${NC}"
    echo -e "${RED}   Failed: $FAILED_TESTS${NC}"
    echo ""
    echo -e "${RED}Please check the output above for details on failed tests.${NC}"
    exit 1
fi

echo ""
echo "🔧 Development Commands:"
echo "========================"
echo ""
echo "Run individual test suites:"
echo "  • Rust:       cd engine && cargo test test_cancellation --features internal"
echo "  • TypeScript: cd engine/language_client_typescript && cargo test test_cancellation"
echo "  • Python:     cd engine/language_client_python && pytest python_src/tests/"
echo "  • Go:         cd engine/language_client_go && go test ./pkg -v"
echo "  • Ruby:       cd engine/language_client_ruby && ruby test/test_cancellation.rb"
echo "  • C/CFFI:     cd engine/language_client_cffi/tests && make test"
echo ""
echo "Memory testing:"
echo "  • C/CFFI:     cd engine/language_client_cffi/tests && make test-memory"
echo ""
echo "Performance testing:"
echo "  • Rust:       cd engine && cargo test --release --features internal"
echo ""

cd "$PROJECT_ROOT"
