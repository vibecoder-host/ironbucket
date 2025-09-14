#!/bin/bash

# Load test utilities
source "$(dirname "$0")/test-utils.sh"

# Performance testing with warp
test_performance() {
    echo "IronBucket Performance Testing"
    echo "=============================="

    # Load environment and check dependencies
    load_test_env
    check_dependencies
    check_ironbucket_running

    # Check if warp is available
    if [ ! -f "./warp" ]; then
        echo -e "${YELLOW}Downloading warp benchmark tool...${NC}"
        wget -q https://github.com/minio/warp/releases/latest/download/warp_Linux_x86_64.tar.gz
        tar -xzf warp_Linux_x86_64.tar.gz
        chmod +x warp
        rm warp_Linux_x86_64.tar.gz
    fi

    # Parse endpoint to get host and port
    ENDPOINT_HOST=$(echo "$S3_ENDPOINT" | sed 's|http://||' | sed 's|https://||')

    echo -e "\n${YELLOW}Configuration:${NC}"
    echo "  Endpoint: $ENDPOINT_HOST"
    echo "  Duration: $WARP_DURATION"
    echo "  Object Size: $WARP_OBJECT_SIZE"
    echo "  Concurrency: $WARP_CONCURRENCY"

    local passed=0
    local failed=0

    # Test 1: Mixed workload
    echo -e "\n${YELLOW}▶ Mixed Workload Test${NC}"
    if ./warp mixed \
        --host="$ENDPOINT_HOST" \
        --access-key="$S3_ACCESS_KEY" \
        --secret-key="$S3_SECRET_KEY" \
        --obj.size="$WARP_OBJECT_SIZE" \
        --duration="$WARP_DURATION" \
        --concurrent="$WARP_CONCURRENCY" \
        --bucket="warp-mixed-$$"; then
        echo -e "${GREEN}✓ Mixed workload test completed${NC}"
        ((passed++))
    else
        echo -e "${RED}✗ Mixed workload test failed${NC}"
        ((failed++))
    fi

    # Test 2: PUT performance
    echo -e "\n${YELLOW}▶ PUT Performance Test${NC}"
    if ./warp put \
        --host="$ENDPOINT_HOST" \
        --access-key="$S3_ACCESS_KEY" \
        --secret-key="$S3_SECRET_KEY" \
        --obj.size="$WARP_OBJECT_SIZE" \
        --duration="$WARP_DURATION" \
        --concurrent="$WARP_CONCURRENCY" \
        --bucket="warp-put-$$"; then
        echo -e "${GREEN}✓ PUT performance test completed${NC}"
        ((passed++))
    else
        echo -e "${RED}✗ PUT performance test failed${NC}"
        ((failed++))
    fi

    # Test 3: GET performance
    echo -e "\n${YELLOW}▶ GET Performance Test${NC}"
    # First upload some objects
    local get_bucket="warp-get-$$"
    ./warp put \
        --host="$ENDPOINT_HOST" \
        --access-key="$S3_ACCESS_KEY" \
        --secret-key="$S3_SECRET_KEY" \
        --obj.size="$WARP_OBJECT_SIZE" \
        --duration="10s" \
        --concurrent="$WARP_CONCURRENCY" \
        --bucket="$get_bucket" >/dev/null 2>&1

    if ./warp get \
        --host="$ENDPOINT_HOST" \
        --access-key="$S3_ACCESS_KEY" \
        --secret-key="$S3_SECRET_KEY" \
        --obj.size="$WARP_OBJECT_SIZE" \
        --duration="$WARP_DURATION" \
        --concurrent="$WARP_CONCURRENCY" \
        --bucket="$get_bucket"; then
        echo -e "${GREEN}✓ GET performance test completed${NC}"
        ((passed++))
    else
        echo -e "${RED}✗ GET performance test failed${NC}"
        ((failed++))
    fi

    # Cleanup test buckets
    echo -e "\n${YELLOW}Cleaning up test buckets...${NC}"
    for bucket in warp-mixed-$$ warp-put-$$ warp-get-$$; do
        cleanup_test_bucket "$bucket" 2>/dev/null || true
    done

    # Print summary
    print_summary $passed $failed
}

# Show usage if --help is provided
if [ "$1" = "--help" ] || [ "$1" = "-h" ]; then
    echo "Usage: $0"
    echo ""
    echo "Run performance tests on IronBucket using warp benchmark tool."
    echo ""
    echo "Configuration is loaded from .env file. Key variables:"
    echo "  S3_ENDPOINT     - S3 endpoint URL"
    echo "  S3_ACCESS_KEY   - Access key for authentication"
    echo "  S3_SECRET_KEY   - Secret key for authentication"
    echo "  WARP_DURATION   - Test duration (e.g., 30s, 1m)"
    echo "  WARP_OBJECT_SIZE - Object size for tests (e.g., 1KB, 10KB, 1MB)"
    echo "  WARP_CONCURRENCY - Number of concurrent operations"
    exit 0
fi

# Run the tests
test_performance