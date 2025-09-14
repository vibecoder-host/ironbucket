#!/bin/bash

# Run all IronBucket tests

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}═══════════════════════════════════════${NC}"
echo -e "${BLUE}     IronBucket Complete Test Suite     ${NC}"
echo -e "${BLUE}═══════════════════════════════════════${NC}"
echo ""

# Track overall results
TOTAL_PASSED=0
TOTAL_FAILED=0

# Change to tests directory
cd "$(dirname "$0")"

# Check if .env exists
if [ ! -f ".env" ]; then
    echo -e "${YELLOW}Warning: .env file not found, copying from .env.example${NC}"
    cp .env.example .env
fi

# Function to run a test and track results
run_test_suite() {
    local test_name="$1"
    local test_script="$2"
    local optional="${3:-false}"

    if [ ! -f "$test_script" ]; then
        echo -e "${YELLOW}Skipping $test_name (script not found)${NC}"
        return
    fi

    echo -e "\n${BLUE}Running: $test_name${NC}"
    echo -e "${BLUE}────────────────────────────────────────${NC}"

    if bash "$test_script"; then
        echo -e "${GREEN}✓ $test_name completed successfully${NC}"
        ((TOTAL_PASSED++))
    else
        if [ "$optional" = "true" ]; then
            echo -e "${YELLOW}⚠ $test_name failed (optional test)${NC}"
        else
            echo -e "${RED}✗ $test_name failed${NC}"
            ((TOTAL_FAILED++))
        fi
    fi
}

# Run test suites
run_test_suite "Basic S3 Operations" "./test-s3-operations.sh"
run_test_suite "Metadata Persistence" "./test-metadata-persistence.sh"

# Performance tests are optional (they take longer)
if [ "${RUN_PERFORMANCE_TESTS:-false}" = "true" ]; then
    run_test_suite "Performance Benchmarks" "./test-performance.sh" true
else
    echo -e "\n${YELLOW}Skipping performance tests (set RUN_PERFORMANCE_TESTS=true to enable)${NC}"
fi

# Final summary
echo -e "\n${BLUE}═══════════════════════════════════════${NC}"
echo -e "${BLUE}          Overall Test Summary          ${NC}"
echo -e "${BLUE}═══════════════════════════════════════${NC}"

if [ $TOTAL_FAILED -eq 0 ]; then
    echo -e "${GREEN}✓ All test suites passed! ($TOTAL_PASSED/$TOTAL_PASSED)${NC}"
    exit 0
else
    echo -e "${RED}✗ Some test suites failed!${NC}"
    echo -e "  ${GREEN}Passed: $TOTAL_PASSED${NC}"
    echo -e "  ${RED}Failed: $TOTAL_FAILED${NC}"
    exit 1
fi